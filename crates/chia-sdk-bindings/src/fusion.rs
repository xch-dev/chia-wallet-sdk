use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{
    self as sdk, FusionDirection, FusionLayer, Launcher, SingletonInfo, SpendContext,
};
use chia_sdk_types::puzzles::FUSION_HINT;
use clvm_utils::TreeHash;

use crate::{AsProgram, AsPtr, Clvm, Nft, Offer, Program, Proof, conditions_to_programs};

/// The hint that identifies fusion singletons on-chain.
pub fn fusion_hint() -> Result<Bytes32> {
    Ok(FUSION_HINT)
}

#[derive(Clone)]
pub struct Fusion {
    pub coin: Coin,
    pub proof: Proof,
    pub launcher_id: Bytes32,
    pub side_a: Vec<Bytes32>,
    pub side_b: Vec<Bytes32>,
}

#[derive(Clone)]
pub struct FusionUnlock {
    pub nft: Nft,
    pub p2_puzzle_hash: Bytes32,
}

#[derive(Clone)]
pub struct FusionSwap {
    pub fusion: Fusion,
    pub locked: Vec<Nft>,
    pub unlocked: Vec<Nft>,
}

#[derive(Clone)]
pub struct LaunchedFusion {
    pub fusion: Fusion,
    pub parent_conditions: Vec<Program>,
}

#[derive(Clone)]
pub struct ParsedFusionChild {
    pub fusion: Fusion,
    pub direction: FusionDirection,
}

impl Fusion {
    fn info(&self) -> Result<FusionLayer> {
        Ok(FusionLayer::new(
            self.launcher_id,
            self.side_a.clone(),
            self.side_b.clone(),
        )?)
    }

    fn to_sdk(&self) -> Result<sdk::Fusion> {
        Ok(sdk::Fusion::new(
            self.coin,
            self.proof.clone().into(),
            self.info()?,
        ))
    }

    /// Recovers a fusion from the spend of its launcher coin. Returns `null` if it didn't launch one,
    /// and throws if it launched one that's invalid.
    pub fn parse_launcher(launcher_coin: Coin, launcher_solution: Program) -> Result<Option<Self>> {
        let ctx = launcher_solution.0.lock().unwrap();
        Ok(sdk::Fusion::parse_launcher(&ctx, launcher_coin, launcher_solution.1)?.map(Into::into))
    }

    /// Parses the child of a fusion from its parent's spend, along with the direction of the swap,
    /// which tells you which side is locked now.
    pub fn parse_child(
        parent_coin: Coin,
        parent_puzzle: Program,
        parent_solution: Program,
    ) -> Result<Option<ParsedFusionChild>> {
        let ctx = parent_puzzle.0.lock().unwrap();
        let puzzle = sdk::Puzzle::parse(&ctx, parent_puzzle.1);

        Ok(
            sdk::Fusion::parse_child(&ctx, parent_coin, puzzle, parent_solution.1)?.map(
                |(fusion, direction)| ParsedFusionChild {
                    fusion: fusion.into(),
                    direction,
                },
            ),
        )
    }

    /// The puzzle hash that NFTs are sent to in order to lock them.
    pub fn p2_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.info()?.p2_puzzle_hash())
    }

    pub fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.info()?.inner_puzzle_hash())
    }

    pub fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.info()?.puzzle_hash())
    }

    /// The conditions that an NFT on either side must output in order to be locked. They remove the
    /// NFT's assigned owner, since requested NFTs can't have one in an offer.
    pub fn lock_nft_conditions(&self, clvm: Clvm, nft: Nft) -> Result<Vec<Program>> {
        let mut ctx = clvm.0.lock().unwrap();
        let nft = nft.as_ptr(&ctx);
        let conditions = self.to_sdk()?.lock_nft_conditions(&mut ctx, &nft)?;
        conditions_to_programs(&mut ctx, &clvm.0, conditions)
    }

    /// Spends the fusion to swap the two sides. The offered NFTs must be in the settlement puzzle,
    /// and the NFTs being unlocked must be locked by the fusion. The spends are added to `clvm`.
    ///
    /// The spend that moves an offered NFT into the settlement puzzle must assert the payment that
    /// it expects in return, as offers do. Otherwise, anyone who sees that spend can take the NFT.
    pub fn swap(
        &self,
        clvm: Clvm,
        direction: FusionDirection,
        offered: Vec<Nft>,
        unlocks: Vec<FusionUnlock>,
        offer_nonce: Bytes32,
    ) -> Result<FusionSwap> {
        let mut ctx = clvm.0.lock().unwrap();

        let offered: Vec<_> = offered.iter().map(|nft| nft.as_ptr(&ctx)).collect();
        let unlocks: Vec<_> = unlocks
            .iter()
            .map(|unlock| sdk::FusionUnlock::new(unlock.nft.as_ptr(&ctx), unlock.p2_puzzle_hash))
            .collect();

        let swap = self
            .to_sdk()?
            .swap(&mut ctx, direction, &offered, &unlocks, offer_nonce)?;

        Ok(swap_result(swap, &clvm.0))
    }

    /// Takes an offer of the NFTs on one side, which requests the NFTs on the other side. `unlocks`
    /// are the locked NFTs, which must have been created with the same `clvm` as the offer. The
    /// spends are added to that `clvm`, and should be passed to `Offer.take`.
    pub fn take_offer(&self, offer: Offer, unlocks: Vec<Nft>) -> Result<FusionSwap> {
        let mut ctx = offer.clvm.lock().unwrap();

        let unlocks: Vec<_> = unlocks.iter().map(|nft| nft.as_ptr(&ctx)).collect();
        let swap = self
            .to_sdk()?
            .take_offer(&mut ctx, &offer.inner, &unlocks)?;

        Ok(swap_result(swap, &offer.clvm))
    }
}

fn swap_result(swap: sdk::FusionSwap, clvm: &Arc<Mutex<SpendContext>>) -> FusionSwap {
    let nfts = |nfts: Vec<sdk::Nft>| nfts.iter().map(|nft| nft.as_program(clvm)).collect();

    FusionSwap {
        fusion: swap.fusion.into(),
        locked: nfts(swap.locked),
        unlocked: nfts(swap.unlocked),
    }
}

impl From<sdk::Fusion> for Fusion {
    fn from(value: sdk::Fusion) -> Self {
        Self {
            coin: value.coin,
            proof: value.proof.into(),
            launcher_id: value.info.launcher_id,
            side_a: value.info.side_a().to_vec(),
            side_b: value.info.side_b().to_vec(),
        }
    }
}

impl Clvm {
    /// Launches a fusion that swaps the NFTs on each side. The launcher coin is created by the
    /// parent with `parent_conditions`, and it's hinted so that the fusion can be found on-chain.
    pub fn launch_fusion(
        &self,
        parent_coin_id: Bytes32,
        side_a: Vec<Bytes32>,
        side_b: Vec<Bytes32>,
    ) -> Result<LaunchedFusion> {
        let mut ctx = self.0.lock().unwrap();

        let (conditions, fusion) = Launcher::for_fusion(&mut ctx, parent_coin_id)?
            .launch_fusion(&mut ctx, side_a, side_b)?;

        Ok(LaunchedFusion {
            fusion: fusion.into(),
            parent_conditions: conditions_to_programs(&mut ctx, &self.0, conditions)?,
        })
    }
}
