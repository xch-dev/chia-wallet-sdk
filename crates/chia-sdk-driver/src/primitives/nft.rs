use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive};
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    nft::{NftOwnershipLayerSolution, NftStateLayerSolution},
    offer::{NotarizedPayment, SettlementPaymentsSolution},
    singleton::{SingletonArgs, SingletonSolution},
    LineageProof, Proof,
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    conditions::{NewMetadataOutput, TradePrice, TransferNft},
    run_puzzle, Condition, Conditions,
};
use chia_sha2::Sha256;
use clvm_traits::{clvm_list, FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DriverError, Layer, NftOwnershipLayer, NftStateLayer, Puzzle, RoyaltyTransferLayer,
    SettlementLayer, SingletonLayer, Spend, SpendContext, SpendWithConditions,
};

mod did_owner;
mod metadata_update;
mod nft_info;
mod nft_launcher;
mod nft_mint;

pub use did_owner::*;
pub use metadata_update::*;
pub use nft_info::*;
pub use nft_mint::*;

/// Everything that is required to spend an NFT coin.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nft<M> {
    /// The coin that holds this NFT.
    pub coin: Coin,
    /// The lineage proof for the singleton.
    pub proof: Proof,
    /// The info associated with the NFT, including the metadata.
    pub info: NftInfo<M>,
}

impl<M> Nft<M> {
    pub fn new(coin: Coin, proof: Proof, info: NftInfo<M>) -> Self {
        Nft { coin, proof, info }
    }

    pub fn with_metadata<N>(self, metadata: N) -> Nft<N> {
        Nft {
            coin: self.coin,
            proof: self.proof,
            info: self.info.with_metadata(metadata),
        }
    }
}

impl<M> Nft<M>
where
    M: ToTreeHash,
{
    /// Returns the lineage proof that would be used by the child.
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    /// Creates a new spendable NFT for the child.
    pub fn wrapped_child<N>(
        &self,
        p2_puzzle_hash: Bytes32,
        owner: Option<Bytes32>,
        metadata: N,
    ) -> Nft<N>
    where
        M: Clone,
        N: ToTreeHash,
    {
        let info = self
            .info
            .clone()
            .with_p2_puzzle_hash(p2_puzzle_hash)
            .with_owner(owner)
            .with_metadata(metadata);

        let inner_puzzle_hash = info.inner_puzzle_hash();

        Nft {
            coin: Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(info.launcher_id, inner_puzzle_hash).into(),
                self.coin.amount,
            ),
            proof: Proof::Lineage(self.child_lineage_proof()),
            info,
        }
    }
}

impl<M> Nft<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
{
    /// Creates a coin spend for this NFT.
    pub fn spend(&self, ctx: &mut SpendContext, inner_spend: Spend) -> Result<(), DriverError> {
        let layers = self.info.clone().into_layers(inner_spend.puzzle);

        let puzzle = layers.construct_puzzle(ctx)?;
        let solution = layers.construct_solution(
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

        ctx.spend(self.coin, Spend::new(puzzle, solution))?;

        Ok(())
    }

    /// Spends this NFT with an inner puzzle that supports being spent with conditions.
    pub fn spend_with<I>(
        &self,
        ctx: &mut SpendContext,
        inner: &I,
        conditions: Conditions,
    ) -> Result<(), DriverError>
    where
        I: SpendWithConditions,
    {
        let inner_spend = inner.spend_with_conditions(ctx, conditions)?;
        self.spend(ctx, inner_spend)
    }

    /// Transfers this NFT to a new p2 puzzle hash, with new metadata.
    pub fn transfer_with_metadata<I, N>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        metadata_update: Spend,
        extra_conditions: Conditions,
    ) -> Result<Nft<N>, DriverError>
    where
        I: SpendWithConditions,
        N: ToClvm<Allocator> + FromClvm<Allocator> + ToTreeHash,
        M: ToTreeHash,
    {
        let memos = ctx.hint(p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions
                .create_coin(p2_puzzle_hash, self.coin.amount, Some(memos))
                .update_nft_metadata(metadata_update.puzzle, metadata_update.solution),
        )?;

        let metadata_updater_solution = ctx.alloc(&clvm_list!(
            self.info.metadata.clone(),
            self.info.metadata_updater_puzzle_hash,
            metadata_update.solution
        ))?;
        let ptr = ctx.run(metadata_update.puzzle, metadata_updater_solution)?;
        let output = ctx.extract::<NewMetadataOutput<N, NodePtr>>(ptr)?;

        Ok(self.wrapped_child(
            p2_puzzle_hash,
            self.info.current_owner,
            output.metadata_info.new_metadata,
        ))
    }

    /// Transfers this NFT to a new p2 puzzle hash.
    ///
    /// Note: This does not update the metadata. If you update the metadata manually, the child will be incorrect.
    ///
    /// Use can use the [`Self::transfer_with_metadata`] helper method to update the metadata.
    /// Alternatively, construct a spend manually with [`Self::spend`] or [`Self::spend_with`].
    pub fn transfer<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        extra_conditions: Conditions,
    ) -> Result<Nft<M>, DriverError>
    where
        M: ToTreeHash,
        I: SpendWithConditions,
    {
        let memos = ctx.hint(p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions.create_coin(p2_puzzle_hash, self.coin.amount, Some(memos)),
        )?;

        let metadata = self.info.metadata.clone();

        Ok(self.wrapped_child(p2_puzzle_hash, self.info.current_owner, metadata))
    }

    /// Transfers this NFT to the settlement payments puzzle and includes a list of trade prices.
    pub fn lock_settlement<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        trade_prices: Vec<TradePrice>,
        extra_conditions: Conditions,
    ) -> Result<Nft<M>, DriverError>
    where
        M: ToTreeHash,
        I: SpendWithConditions,
    {
        let transfer_condition = TransferNft::new(None, trade_prices, None);

        let (conditions, nft) = self.transfer_with_condition(
            ctx,
            inner,
            SETTLEMENT_PAYMENT_HASH.into(),
            transfer_condition,
            extra_conditions,
        )?;

        assert_eq!(conditions.len(), 0);

        Ok(nft)
    }

    pub fn unlock_settlement(
        self,
        ctx: &mut SpendContext,
        notarized_payments: Vec<NotarizedPayment>,
    ) -> Result<Nft<M>, DriverError>
    where
        M: ToTreeHash,
    {
        let outputs: Vec<Bytes32> = notarized_payments
            .iter()
            .flat_map(|item| &item.payments)
            .filter_map(|payment| {
                if payment.amount % 2 == 1 {
                    Some(payment.puzzle_hash)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(outputs.len(), 1);

        let inner_spend = SettlementLayer
            .construct_spend(ctx, SettlementPaymentsSolution { notarized_payments })?;

        self.spend(ctx, inner_spend)?;

        Ok(self.wrapped_child(outputs[0], None, self.info.metadata.clone()))
    }

    /// Transfers this NFT to a new p2 puzzle hash and updates the DID owner.
    /// Returns a list of conditions to be used in the DID spend.
    ///
    /// Note: This does not update the metadata. If you update the metadata manually, the child will be incorrect.
    ///
    /// You can construct a spend manually with [`Self::spend`] or [`Self::spend_with`] if you need to update metadata
    /// while transferring to a DID. This is not a common use case, so it's not implemented by default.
    pub fn transfer_to_did<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        new_owner: Option<DidOwner>,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Nft<M>), DriverError>
    where
        M: ToTreeHash,
        I: SpendWithConditions,
    {
        let transfer_condition = TransferNft::new(
            new_owner.map(|owner| owner.did_id),
            Vec::new(),
            new_owner.map(|owner| owner.inner_puzzle_hash),
        );

        self.transfer_with_condition(
            ctx,
            inner,
            p2_puzzle_hash,
            transfer_condition,
            extra_conditions,
        )
    }

    /// Transfers this NFT to a new p2 puzzle hash and runs the transfer program with a condition.
    /// Returns a list of conditions to be used in the DID spend.
    pub fn transfer_with_condition<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        transfer_condition: TransferNft,
        extra_conditions: Conditions,
    ) -> Result<(Conditions, Nft<M>), DriverError>
    where
        M: ToTreeHash,
        I: SpendWithConditions,
    {
        let did_id = transfer_condition.did_id;

        let did_conditions = if did_id.is_some() {
            Conditions::new()
                .assert_puzzle_announcement(did_puzzle_assertion(
                    self.coin.puzzle_hash,
                    &transfer_condition,
                ))
                .create_puzzle_announcement(self.info.launcher_id.into())
        } else {
            Conditions::new()
        };

        let memos = ctx.hint(p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions
                .create_coin(p2_puzzle_hash, self.coin.amount, Some(memos))
                .with(transfer_condition),
        )?;

        let metadata = self.info.metadata.clone();

        let child = self.wrapped_child(p2_puzzle_hash, did_id, metadata);

        Ok((did_conditions, child))
    }
}

impl<M> Nft<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + ToTreeHash,
{
    pub fn parse_child(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
        M: Clone,
    {
        let Some(singleton_layer) =
            SingletonLayer::<Puzzle>::parse_puzzle(allocator, parent_puzzle)?
        else {
            return Ok(None);
        };

        let Some(inner_layers) =
            NftStateLayer::<M, NftOwnershipLayer<RoyaltyTransferLayer, Puzzle>>::parse_puzzle(
                allocator,
                singleton_layer.inner_puzzle,
            )?
        else {
            return Ok(None);
        };

        let parent_solution = SingletonLayer::<
            NftStateLayer<M, NftOwnershipLayer<RoyaltyTransferLayer, Puzzle>>,
        >::parse_solution(allocator, parent_solution)?;

        let inner_puzzle = inner_layers.inner_puzzle.inner_puzzle;
        let inner_solution = parent_solution.inner_solution.inner_solution.inner_solution;

        let output = run_puzzle(allocator, inner_puzzle.ptr(), inner_solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let mut create_coin = None;
        let mut new_owner = None;
        let mut new_metadata = None;

        for condition in conditions {
            match condition {
                Condition::CreateCoin(condition) if condition.amount % 2 == 1 => {
                    create_coin = Some(condition);
                }
                Condition::TransferNft(condition) => {
                    new_owner = Some(condition);
                }
                Condition::UpdateNftMetadata(condition) => {
                    new_metadata = Some(condition);
                }
                _ => {}
            }
        }

        let Some(create_coin) = create_coin else {
            return Err(DriverError::MissingChild);
        };

        let mut layers = SingletonLayer::new(singleton_layer.launcher_id, inner_layers);

        if let Some(new_owner) = new_owner {
            layers.inner_puzzle.inner_puzzle.current_owner = new_owner.did_id;
        }

        if let Some(new_metadata) = new_metadata {
            let metadata_updater_solution = clvm_list!(
                layers.inner_puzzle.metadata.clone(),
                layers.inner_puzzle.metadata_updater_puzzle_hash,
                new_metadata.updater_solution
            )
            .to_clvm(allocator)?;

            let output = run_puzzle(
                allocator,
                new_metadata.updater_puzzle_reveal,
                metadata_updater_solution,
            )?;

            let output =
                NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)?.metadata_info;
            layers.inner_puzzle.metadata = output.new_metadata;
            layers.inner_puzzle.metadata_updater_puzzle_hash = output.new_updater_puzzle_hash;
        }

        let mut info = NftInfo::from_layers(layers);
        info.p2_puzzle_hash = create_coin.puzzle_hash;

        Ok(Some(Self {
            coin: Coin::new(
                parent_coin.coin_id(),
                SingletonArgs::curry_tree_hash(info.launcher_id, info.inner_puzzle_hash()).into(),
                create_coin.amount,
            ),
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: singleton_layer.inner_puzzle.curried_puzzle_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            info,
        }))
    }
}

pub fn did_puzzle_assertion(nft_full_puzzle_hash: Bytes32, new_nft_owner: &TransferNft) -> Bytes32 {
    let mut allocator = Allocator::new();

    let new_nft_owner_args = clvm_list!(
        new_nft_owner.did_id,
        &new_nft_owner.trade_prices,
        new_nft_owner.did_inner_puzzle_hash
    )
    .to_clvm(&mut allocator)
    .unwrap();

    let mut hasher = Sha256::new();
    hasher.update(nft_full_puzzle_hash);
    hasher.update([0xad, 0x4c]);
    hasher.update(tree_hash(&allocator, new_nft_owner_args));

    Bytes32::new(hasher.finalize())
}

pub fn calculate_nft_trace_price(amount: u64, nft_count: usize) -> Option<u64> {
    let amount = BigDecimal::from(amount);
    let nft_count = BigDecimal::from(nft_count as u64);
    floor(amount / nft_count).to_u64()
}

pub fn calculate_nft_royalty(trade_price: u64, royalty_percentage: u16) -> Option<u64> {
    let trade_price = BigDecimal::from(trade_price);
    let royalty_percentage = BigDecimal::from(royalty_percentage);
    let percent = royalty_percentage / BigDecimal::from(10_000);
    floor(trade_price * percent).to_u64()
}

#[allow(clippy::needless_pass_by_value)]
fn floor(amount: BigDecimal) -> BigDecimal {
    amount.with_scale_round(0, RoundingMode::Floor)
}

#[cfg(test)]
mod tests {
    use crate::{IntermediateLauncher, Launcher, NftMint, StandardLayer};

    use super::*;

    use chia_puzzle_types::nft::NftMetadata;
    use chia_sdk_test::Simulator;

    #[test]
    fn test_nft_transfer() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        let mint = NftMint::new(
            NftMetadata::default(),
            alice.puzzle_hash,
            300,
            Some(DidOwner::from_did_info(&did.info)),
        );

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, mint)?;
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

        let mint = NftMint::new(
            NftMetadata::default(),
            alice.puzzle_hash,
            300,
            Some(DidOwner::from_did_info(&did.info)),
        );

        let (mint_nft, mut nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, mint)?;

        let mut did = did.update(ctx, &alice_p2, mint_nft)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        for i in 0..5 {
            let did_owner = DidOwner::from_did_info(&did.info);

            let (spend_nft, new_nft) = nft.transfer_to_did(
                ctx,
                &alice_p2,
                alice.puzzle_hash,
                if i % 2 == 0 { Some(did_owner) } else { None },
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

        let mint = NftMint::new(
            NftMetadata {
                data_uris: vec!["example.com".to_string()],
                data_hash: Some(Bytes32::default()),
                ..Default::default()
            },
            alice.puzzle_hash,
            300,
            Some(DidOwner::from_did_info(&did.info)),
        );

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, mint)?;
        let _did = did.update(ctx, &alice_p2, mint_nft)?;

        let metadata_update = MetadataUpdate::NewDataUri("another.com".to_string()).spend(ctx)?;
        let parent_nft = nft.clone();
        let nft: Nft<NftMetadata> = nft.transfer_with_metadata(
            ctx,
            &alice_p2,
            alice.puzzle_hash,
            metadata_update,
            Conditions::new(),
        )?;

        assert_eq!(
            nft.info.metadata,
            NftMetadata {
                data_uris: vec!["another.com".to_string(), "example.com".to_string()],
                data_hash: Some(Bytes32::default()),
                ..Default::default()
            }
        );

        let child_nft = nft.clone();
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

        let new_child_nft =
            Nft::<NftMetadata>::parse_child(ctx, parent_nft.coin, parent_puzzle, parent_solution)?
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

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(
                ctx,
                NftMint::new(
                    metadata,
                    alice.puzzle_hash,
                    300,
                    Some(DidOwner::from_did_info(&did.info)),
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

        let nft = Nft::<NftMetadata>::parse_child(&mut allocator, parent_coin, puzzle, solution)?
            .expect("could not parse nft");

        assert_eq!(nft, expected_nft);

        Ok(())
    }
}
