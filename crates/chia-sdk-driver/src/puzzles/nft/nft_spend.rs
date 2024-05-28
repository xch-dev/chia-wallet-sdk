use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftOwnershipLayerSolution, NftRoyaltyTransferPuzzleArgs,
        NftStateLayerArgs, NftStateLayerSolution,
    },
    singleton::SingletonArgs,
    LineageProof, Proof,
};
use chia_sdk_types::{
    conditions::{AssertPuzzleAnnouncement, NewNftOwner},
    puzzles::NftInfo,
};
use clvm_traits::{clvm_list, ToClvm};
use clvm_utils::CurriedProgram;
use clvmr::{
    sha2::{Digest, Sha256},
    NodePtr,
};

use crate::{
    puzzles::{spend_singleton, StandardSpend},
    spend_builder::{InnerSpend, P2Spend, SpendConditions},
    SpendContext, SpendError,
};

#[derive(Debug, Clone, Copy)]
pub struct NoNftOutput;

#[derive(Debug, Clone, Copy)]
pub enum NftOutput {
    SamePuzzleHash,
    NewPuzzleHash { puzzle_hash: Bytes32 },
}

#[derive(Debug, Clone)]
#[must_use]
pub struct StandardNftSpend<T> {
    standard_spend: StandardSpend,
    output: T,
    new_owner: Option<NewNftOwner>,
}

impl Default for StandardNftSpend<NoNftOutput> {
    fn default() -> Self {
        Self {
            output: NoNftOutput,
            standard_spend: StandardSpend::new(),
            new_owner: None,
        }
    }
}

impl StandardNftSpend<NoNftOutput> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(self) -> StandardNftSpend<NftOutput> {
        StandardNftSpend {
            standard_spend: self.standard_spend,
            output: NftOutput::SamePuzzleHash,
            new_owner: self.new_owner,
        }
    }

    pub fn transfer(self, puzzle_hash: Bytes32) -> StandardNftSpend<NftOutput> {
        StandardNftSpend {
            standard_spend: self.standard_spend,
            output: NftOutput::NewPuzzleHash { puzzle_hash },
            new_owner: self.new_owner,
        }
    }
}

impl<T> StandardNftSpend<T> {
    pub fn new_owner(mut self, did_id: Bytes32, did_inner_puzzle_hash: Bytes32) -> Self {
        self.new_owner = Some(NewNftOwner {
            new_owner: Some(did_id),
            trade_prices_list: Vec::new(),
            new_did_p2_puzzle_hash: Some(did_inner_puzzle_hash),
        });
        self
    }

    pub fn reset_owner(mut self) -> Self {
        self.new_owner = Some(NewNftOwner {
            new_owner: None,
            trade_prices_list: Vec::new(),
            new_did_p2_puzzle_hash: None,
        });
        self
    }

    pub fn chain(mut self, chained: SpendConditions) -> Self {
        self.standard_spend = self.standard_spend.chain(chained);
        self
    }
}

impl<T> P2Spend for StandardNftSpend<T> {
    fn raw_condition(mut self, condition: NodePtr) -> Self {
        self.standard_spend = self.standard_spend.raw_condition(condition);
        self
    }
}

impl StandardNftSpend<NftOutput> {
    pub fn finish<M>(
        mut self,
        ctx: &mut SpendContext<'_>,
        synthetic_key: PublicKey,
        mut nft_info: NftInfo<M>,
    ) -> Result<(SpendConditions, NftInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>,
    {
        let mut parent = SpendConditions::default();

        let p2_puzzle_hash = match self.output {
            NftOutput::SamePuzzleHash => nft_info.p2_puzzle_hash,
            NftOutput::NewPuzzleHash { puzzle_hash } => puzzle_hash,
        };

        if let Some(new_owner) = &self.new_owner {
            self.standard_spend = self.standard_spend.raw_condition(ctx.alloc(new_owner)?);

            let new_nft_owner_args = ctx.alloc(&clvm_list!(
                new_owner.new_owner,
                new_owner.trade_prices_list.clone(),
                new_owner.new_did_p2_puzzle_hash
            ))?;

            let mut announcement_id = Sha256::new();
            announcement_id.update(nft_info.coin.puzzle_hash);
            announcement_id.update([0xad, 0x4c]);
            announcement_id.update(ctx.tree_hash(new_nft_owner_args));

            parent = parent.raw_condition(ctx.alloc(&AssertPuzzleAnnouncement {
                announcement_id: Bytes32::new(announcement_id.finalize().into()),
            })?);
        }

        let inner_spend = self
            .standard_spend
            .create_hinted_coin(ctx, p2_puzzle_hash, nft_info.coin.amount)?
            .inner_spend(ctx, synthetic_key)?;

        let nft_spend = raw_nft_spend(ctx, &nft_info, inner_spend)?;
        ctx.spend(nft_spend);

        nft_info.current_owner = self
            .new_owner
            .map_or(nft_info.current_owner, |value| value.new_owner);

        let metadata_ptr = ctx.alloc(&nft_info.metadata)?;

        let transfer_program = NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
            nft_info.launcher_id,
            nft_info.royalty_puzzle_hash,
            nft_info.royalty_percentage,
        );

        let ownership_layer = NftOwnershipLayerArgs::curry_tree_hash(
            nft_info.current_owner,
            transfer_program,
            p2_puzzle_hash.into(),
        );

        let state_layer =
            NftStateLayerArgs::curry_tree_hash(ctx.tree_hash(metadata_ptr), ownership_layer);

        let singleton_puzzle_hash =
            SingletonArgs::curry_tree_hash(nft_info.launcher_id, state_layer);

        nft_info.proof = Proof::Lineage(LineageProof {
            parent_parent_coin_id: nft_info.coin.parent_coin_info,
            parent_inner_puzzle_hash: nft_info.nft_inner_puzzle_hash,
            parent_amount: nft_info.coin.amount,
        });

        nft_info.coin = Coin::new(
            nft_info.coin.coin_id(),
            singleton_puzzle_hash.into(),
            nft_info.coin.amount,
        );

        nft_info.nft_inner_puzzle_hash = state_layer.into();

        Ok((parent, nft_info))
    }
}

pub fn raw_nft_spend<M>(
    ctx: &mut SpendContext<'_>,
    nft_info: &NftInfo<M>,
    inner_spend: InnerSpend,
) -> Result<CoinSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let transfer_program_puzzle = ctx.nft_royalty_transfer()?;

    let transfer_program = CurriedProgram {
        program: transfer_program_puzzle,
        args: NftRoyaltyTransferPuzzleArgs::new(
            nft_info.launcher_id,
            nft_info.royalty_puzzle_hash,
            nft_info.royalty_percentage,
        ),
    };

    let ownership_layer_spend =
        spend_nft_ownership_layer(ctx, nft_info.current_owner, transfer_program, inner_spend)?;

    let state_layer_spend = spend_nft_state_layer(ctx, &nft_info.metadata, ownership_layer_spend)?;

    spend_singleton(
        ctx,
        nft_info.coin,
        nft_info.launcher_id,
        nft_info.proof,
        state_layer_spend,
    )
}

pub fn spend_nft_state_layer<M>(
    ctx: &mut SpendContext<'_>,
    metadata: M,
    inner_spend: InnerSpend,
) -> Result<InnerSpend, SpendError>
where
    M: ToClvm<NodePtr>,
{
    let nft_state_layer = ctx.nft_state_layer()?;

    let puzzle = ctx.alloc(&CurriedProgram {
        program: nft_state_layer,
        args: NftStateLayerArgs::new(metadata, inner_spend.puzzle()),
    })?;

    let solution = ctx.alloc(&NftStateLayerSolution {
        inner_solution: inner_spend.solution(),
    })?;

    Ok(InnerSpend::new(puzzle, solution))
}

pub fn spend_nft_ownership_layer<P>(
    ctx: &mut SpendContext<'_>,
    current_owner: Option<Bytes32>,
    transfer_program: P,
    inner_spend: InnerSpend,
) -> Result<InnerSpend, SpendError>
where
    P: ToClvm<NodePtr>,
{
    let nft_ownership_layer = ctx.nft_ownership_layer()?;

    let puzzle = ctx.alloc(&CurriedProgram {
        program: nft_ownership_layer,
        args: NftOwnershipLayerArgs::new(current_owner, transfer_program, inner_spend.puzzle()),
    })?;

    let solution = ctx.alloc(&NftOwnershipLayerSolution {
        inner_solution: inner_spend.solution(),
    })?;

    Ok(InnerSpend::new(puzzle, solution))
}

#[cfg(test)]
mod tests {
    use crate::puzzles::{
        CreateDid, IntermediateLauncher, Launcher, MintNft, OwnerDid, StandardDidSpend,
        StandardMint,
    };

    use super::*;

    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{test_transaction, Simulator};
    use clvmr::Allocator;

    #[tokio::test]
    async fn test_nft_lineage() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = sim.secret_key().await?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 2).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (create_did, did_info) = Launcher::new(coin.coin_id(), 1)
            .create(ctx)?
            .create_standard_did(ctx, pk)?;

        StandardSpend::new()
            .chain(create_did)
            .finish(ctx, coin, pk)?;

        let (mint_nft, mut nft_info) = IntermediateLauncher::new(did_info.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_standard_nft(
                ctx,
                StandardMint {
                    metadata: (),
                    royalty_puzzle_hash: puzzle_hash,
                    royalty_percentage: 300,
                    synthetic_key: pk,
                    owner_puzzle_hash: puzzle_hash,
                    owner_did: Some(OwnerDid {
                        did_id: did_info.launcher_id,
                        did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
                    }),
                },
            )?;

        let mut did_info = StandardDidSpend::new()
            .chain(mint_nft)
            .recreate()
            .finish(ctx, pk, did_info)?;

        for i in 0..5 {
            let mut spend = StandardNftSpend::new().update();

            if i % 2 == 0 {
                spend = spend.new_owner(did_info.launcher_id, did_info.did_inner_puzzle_hash);
            }

            let (nft_spend, new_nft_info) = spend.finish(ctx, pk, nft_info)?;
            nft_info = new_nft_info;

            did_info = StandardDidSpend::new()
                .chain(nft_spend)
                .recreate()
                .finish(ctx, pk, did_info)?;
        }

        test_transaction(&peer, ctx.take_spends(), &[sk]).await;

        let coin_state = sim
            .coin_state(did_info.coin.coin_id())
            .await
            .expect("expected did coin");
        assert_eq!(coin_state.coin, did_info.coin);

        let coin_state = sim
            .coin_state(nft_info.coin.coin_id())
            .await
            .expect("expected nft coin");
        assert_eq!(coin_state.coin, nft_info.coin);

        Ok(())
    }
}
