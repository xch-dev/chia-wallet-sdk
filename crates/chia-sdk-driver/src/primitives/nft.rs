use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    nft::{NftOwnershipLayerSolution, NftStateLayerSolution},
    offer::{NotarizedPayment, SettlementPaymentsSolution},
    singleton::SingletonSolution,
    LineageProof, Proof,
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    conditions::{TradePrice, TransferNft},
    Conditions,
};
use chia_sha2::Sha256;
use clvm_traits::{clvm_list, ToClvm};
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};

use crate::{
    DriverError, HashedPtr, Layer, Puzzle, SettlementLayer, Singleton, SingletonInfo, Spend,
    SpendContext, SpendWithConditions,
};

mod metadata_update;
mod nft_info;
mod nft_launcher;
mod nft_mint;

pub use metadata_update::*;
pub use nft_info::*;
pub use nft_mint::*;

/// Contains all information needed to spend the outer puzzles of NFT coins.
/// The [`NftInfo`] is used to construct the puzzle, but the [`Proof`] is needed for the solution.
///
/// The only thing missing to create a valid coin spend is the inner puzzle and solution.
/// However, this is handled separately to provide as much flexibility as possible.
///
/// This type should contain all of the information you need to store in a database for later.
/// As long as you can figure out what puzzle the p2 puzzle hash corresponds to and spend it,
/// you have enough information to spend the NFT coin.
pub type Nft = Singleton<NftInfo>;

impl Nft {
    /// Creates a new [`Nft`] that represents a child of this one.
    pub fn child(
        &self,
        p2_puzzle_hash: Bytes32,
        current_owner: Option<Bytes32>,
        metadata: HashedPtr,
        amount: u64,
    ) -> Nft {
        self.child_with(
            NftInfo {
                metadata,
                current_owner,
                p2_puzzle_hash,
                ..self.info
            },
            amount,
        )
    }

    /// Spends this NFT coin with the provided inner spend.
    /// The spend is added to the [`SpendContext`] for convenience.
    pub fn spend(&self, ctx: &mut SpendContext, inner_spend: Spend) -> Result<Self, DriverError> {
        let layers = self.info.into_layers(inner_spend.puzzle);

        let spend = layers.construct_spend(
            ctx,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution: NftStateLayerSolution {
                    inner_solution: NftOwnershipLayerSolution {
                        inner_solution: inner_spend.solution,
                    },
                },
            },
        )?;

        ctx.spend(self.coin, spend)?;

        let (info, create_coin) = self.info.child_from_p2_spend(ctx, inner_spend)?;

        Ok(self.child_with(info, create_coin.amount))
    }

    /// Spends this NFT coin with a [`Layer`] that supports [`SpendWithConditions`].
    /// This is a building block for built in spend methods, but can also be used to spend
    /// NFTs with conditions more easily.
    ///
    /// However, if you need full flexibility of the inner spend, you can use [`Nft::spend`] instead.
    pub fn spend_with<I>(
        &self,
        ctx: &mut SpendContext,
        inner: &I,
        conditions: Conditions,
    ) -> Result<Self, DriverError>
    where
        I: SpendWithConditions,
    {
        let inner_spend = inner.spend_with_conditions(ctx, conditions)?;
        self.spend(ctx, inner_spend)
    }

    /// Transfers this NFT coin to a new p2 puzzle hash and runs the metadata updater with the
    /// provided spend.
    ///
    /// This spend requires a [`Layer`] that supports [`SpendWithConditions`]. If it doesn't, you can
    /// use [`Nft::spend_with`] instead.
    pub fn transfer_with_metadata<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        metadata_update: Spend,
        extra_conditions: Conditions,
    ) -> Result<Nft, DriverError>
    where
        I: SpendWithConditions,
    {
        let memos = ctx.hint(p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions
                .create_coin(p2_puzzle_hash, self.coin.amount, memos)
                .update_nft_metadata(metadata_update.puzzle, metadata_update.solution),
        )
    }

    /// Transfers this NFT coin to a new p2 puzzle hash.
    ///
    /// This spend requires a [`Layer`] that supports [`SpendWithConditions`]. If it doesn't, you can
    /// use [`Nft::spend_with`] instead.
    pub fn transfer<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        extra_conditions: Conditions,
    ) -> Result<Nft, DriverError>
    where
        I: SpendWithConditions,
    {
        let memos = ctx.hint(p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions.create_coin(p2_puzzle_hash, self.coin.amount, memos),
        )
    }

    /// Transfers this NFT coin to the settlement puzzle hash and runs the transfer program to
    /// remove the assigned owner and reveal the trade prices for the offer.
    ///
    /// This spend requires a [`Layer`] that supports [`SpendWithConditions`]. If it doesn't, you can
    /// use [`Nft::spend_with`] instead.
    pub fn lock_settlement<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        trade_prices: Vec<TradePrice>,
        extra_conditions: Conditions,
    ) -> Result<Nft, DriverError>
    where
        I: SpendWithConditions,
    {
        let transfer_condition = TransferNft::new(None, trade_prices, None);

        let (conditions, nft) = self.assign_owner(
            ctx,
            inner,
            SETTLEMENT_PAYMENT_HASH.into(),
            transfer_condition,
            extra_conditions,
        )?;

        assert_eq!(conditions.len(), 0);

        Ok(nft)
    }

    /// Spends this NFT with the settlement puzzle as its inner puzzle, with the provided notarized
    /// payments. This only works if the NFT has been locked in an offer already.
    pub fn unlock_settlement(
        self,
        ctx: &mut SpendContext,
        notarized_payments: Vec<NotarizedPayment>,
    ) -> Result<Nft, DriverError> {
        let inner_spend = SettlementLayer
            .construct_spend(ctx, SettlementPaymentsSolution { notarized_payments })?;

        self.spend(ctx, inner_spend)
    }

    /// Transfers this NFT coin to a new p2 puzzle hash and runs the transfer program.
    ///
    /// This will return the conditions that must be emitted by the singleton you're assigning the NFT to.
    /// The singleton must be spent in the same spend bundle as the NFT spend and emit these conditions.
    ///
    /// However, if the NFT is being unassigned, there is no singleton spend and the conditions are empty.
    ///
    /// This spend requires a [`Layer`] that supports [`SpendWithConditions`]. If it doesn't, you can
    /// use [`Nft::spend_with`] instead.
    pub fn assign_owner<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        transfer_condition: TransferNft,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Nft), DriverError>
    where
        I: SpendWithConditions,
    {
        let launcher_id = transfer_condition.launcher_id;

        let assignment_conditions = if launcher_id.is_some() {
            Conditions::new()
                .assert_puzzle_announcement(assignment_puzzle_announcement_id(
                    self.coin.puzzle_hash,
                    &transfer_condition,
                ))
                .create_puzzle_announcement(self.info.launcher_id.into())
        } else {
            Conditions::new()
        };

        let memos = ctx.hint(p2_puzzle_hash)?;

        let child = self.spend_with(
            ctx,
            inner,
            extra_conditions
                .create_coin(p2_puzzle_hash, self.coin.amount, memos)
                .with(transfer_condition),
        )?;

        Ok((assignment_conditions, child))
    }

    /// Parses the child of an [`Nft`] from the parent coin spend.
    ///
    /// This can be used to construct a valid spendable [`Nft`] for a hinted coin.
    /// You simply need to look up the parent coin's spend, parse the child, and
    /// ensure it matches the hinted coin.
    ///
    /// This will automatically run the transfer program or metadata updater, if
    /// they are revealed in the p2 spend's output conditions. This way the returned
    /// [`Nft`] will have the correct owner (if present) and metadata.
    pub fn parse_child(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let Some((parent_info, p2_puzzle)) = NftInfo::parse(allocator, parent_puzzle)? else {
            return Ok(None);
        };

        let p2_solution =
            StandardNftLayers::<HashedPtr, Puzzle>::parse_solution(allocator, parent_solution)?
                .inner_solution
                .inner_solution
                .inner_solution;

        let (info, create_coin) =
            parent_info.child_from_p2_spend(allocator, Spend::new(p2_puzzle.ptr(), p2_solution))?;

        Ok(Some(Self {
            coin: Coin::new(
                parent_coin.coin_id(),
                info.puzzle_hash().into(),
                create_coin.amount,
            ),
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: parent_info.inner_puzzle_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            info,
        }))
    }

    /// Parses an [`Nft`] and its p2 spend from a coin spend.
    ///
    /// If the puzzle is not an NFT, this will return [`None`] instead of an error.
    /// However, if the puzzle should have been an NFT but had a parsing error, this will return an error.
    pub fn parse(
        allocator: &Allocator,
        coin: Coin,
        puzzle: Puzzle,
        solution: NodePtr,
    ) -> Result<Option<(Self, Puzzle, NodePtr)>, DriverError> {
        let Some((nft_info, p2_puzzle)) = NftInfo::parse(allocator, puzzle)? else {
            return Ok(None);
        };

        let solution = StandardNftLayers::<HashedPtr, Puzzle>::parse_solution(allocator, solution)?;

        let p2_solution = solution.inner_solution.inner_solution.inner_solution;

        Ok(Some((
            Self::new(coin, solution.lineage_proof, nft_info),
            p2_puzzle,
            p2_solution,
        )))
    }
}

pub fn assignment_puzzle_announcement_id(
    nft_full_puzzle_hash: Bytes32,
    new_nft_owner: &TransferNft,
) -> Bytes32 {
    let mut allocator = Allocator::new();

    let new_nft_owner_args = clvm_list!(
        new_nft_owner.launcher_id,
        &new_nft_owner.trade_prices,
        new_nft_owner.singleton_inner_puzzle_hash
    )
    .to_clvm(&mut allocator)
    .unwrap();

    let mut hasher = Sha256::new();
    hasher.update(nft_full_puzzle_hash);
    hasher.update([0xad, 0x4c]);
    hasher.update(tree_hash(&allocator, new_nft_owner_args));

    Bytes32::new(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::slice;

    use crate::{IntermediateLauncher, Launcher, NftMint, SingletonInfo, StandardLayer};

    use super::*;

    use chia_puzzle_types::nft::NftMetadata;
    use chia_sdk_test::Simulator;
    use clvm_utils::ToTreeHash;

    #[test]
    fn test_nft_transfer() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        let metadata = ctx.alloc_hashed(&NftMetadata::default())?;

        let mint = NftMint::new(
            metadata,
            alice.puzzle_hash,
            300,
            Some(TransferNft::new(
                Some(did.info.launcher_id),
                Vec::new(),
                Some(did.info.inner_puzzle_hash().into()),
            )),
        );

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, &mint)?;
        let _did = did.update(ctx, &alice_p2, mint_nft)?;
        let _nft = nft.transfer(ctx, &alice_p2, alice.puzzle_hash, Conditions::new())?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_nft_lineage() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        let metadata = ctx.alloc_hashed(&NftMetadata::default())?;

        let mint = NftMint::new(
            metadata,
            alice.puzzle_hash,
            300,
            Some(TransferNft::new(
                Some(did.info.launcher_id),
                Vec::new(),
                Some(did.info.inner_puzzle_hash().into()),
            )),
        );

        let (mint_nft, mut nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, &mint)?;

        let mut did = did.update(ctx, &alice_p2, mint_nft)?;

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        for i in 0..5 {
            let transfer_condition = TransferNft::new(
                Some(did.info.launcher_id),
                Vec::new(),
                Some(did.info.inner_puzzle_hash().into()),
            );

            let (spend_nft, new_nft) = nft.assign_owner(
                ctx,
                &alice_p2,
                alice.puzzle_hash,
                if i % 2 == 0 {
                    transfer_condition
                } else {
                    TransferNft::new(None, Vec::new(), None)
                },
                Conditions::new(),
            )?;

            nft = new_nft;
            did = did.update(ctx, &alice_p2, spend_nft)?;
        }

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[test]
    fn test_nft_metadata_update() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        let metadata = ctx.alloc_hashed(&NftMetadata {
            data_uris: vec!["example.com".to_string()],
            data_hash: Some(Bytes32::default()),
            ..Default::default()
        })?;

        let mint = NftMint::new(
            metadata,
            alice.puzzle_hash,
            300,
            Some(TransferNft::new(
                Some(did.info.launcher_id),
                Vec::new(),
                Some(did.info.inner_puzzle_hash().into()),
            )),
        );

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, &mint)?;
        let _did = did.update(ctx, &alice_p2, mint_nft)?;

        let metadata_update = MetadataUpdate::NewDataUri("another.com".to_string()).spend(ctx)?;
        let parent_nft = nft;
        let nft = nft.transfer_with_metadata(
            ctx,
            &alice_p2,
            alice.puzzle_hash,
            metadata_update,
            Conditions::new(),
        )?;

        assert_eq!(
            nft.info.metadata.tree_hash(),
            NftMetadata {
                data_uris: vec!["another.com".to_string(), "example.com".to_string()],
                data_hash: Some(Bytes32::default()),
                ..Default::default()
            }
            .tree_hash()
        );

        let child_nft = nft;
        let _nft = nft.transfer(ctx, &alice_p2, alice.puzzle_hash, Conditions::new())?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        // Ensure that the metadata update can be parsed.
        let parent_puzzle = sim
            .puzzle_reveal(parent_nft.coin.coin_id())
            .expect("missing puzzle");

        let parent_solution = sim
            .solution(parent_nft.coin.coin_id())
            .expect("missing solution");

        let parent_puzzle = parent_puzzle.to_clvm(ctx)?;
        let parent_puzzle = Puzzle::parse(ctx, parent_puzzle);
        let parent_solution = parent_solution.to_clvm(ctx)?;

        let new_child_nft = Nft::parse_child(ctx, parent_nft.coin, parent_puzzle, parent_solution)?
            .expect("child is not an NFT");

        assert_eq!(new_child_nft, child_nft);

        Ok(())
    }

    #[test]
    fn test_parse_nft() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        let mut metadata = NftMetadata::default();
        metadata.data_uris.push("example.com".to_string());

        let metadata = ctx.alloc_hashed(&metadata)?;

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(
                ctx,
                &NftMint::new(
                    metadata,
                    alice.puzzle_hash,
                    300,
                    Some(TransferNft::new(
                        Some(did.info.launcher_id),
                        Vec::new(),
                        Some(did.info.inner_puzzle_hash().into()),
                    )),
                ),
            )?;
        let _did = did.update(ctx, &alice_p2, mint_nft)?;

        let parent_coin = nft.coin;
        let expected_nft = nft.transfer(ctx, &alice_p2, alice.puzzle_hash, Conditions::new())?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let mut allocator = Allocator::new();

        let puzzle_reveal = sim
            .puzzle_reveal(parent_coin.coin_id())
            .expect("missing puzzle")
            .to_clvm(&mut allocator)?;

        let solution = sim
            .solution(parent_coin.coin_id())
            .expect("missing solution")
            .to_clvm(&mut allocator)?;

        let puzzle = Puzzle::parse(&allocator, puzzle_reveal);

        let nft = Nft::parse_child(&mut allocator, parent_coin, puzzle, solution)?
            .expect("could not parse nft");

        assert_eq!(nft, expected_nft);

        Ok(())
    }
}
