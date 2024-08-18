use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::{NftOwnershipLayerSolution, NftStateLayerSolution},
    singleton::{SingletonArgs, SingletonSolution},
    LineageProof, Proof,
};
use chia_sdk_types::{
    run_puzzle, Condition, Conditions, NewMetadataCondition, NewMetadataOutput, NewNftOwner,
};
use clvm_traits::{clvm_list, FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash};
use clvmr::{sha2::Sha256, Allocator, NodePtr};

use crate::{
    DriverError, Layer, NftOwnershipLayer, NftStateLayer, Primitive, Puzzle, RoyaltyTransferLayer,
    SingletonLayer, Spend, SpendContext, StandardLayer,
};

use super::NftInfo;

/// Everything that is required to spend an NFT coin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nft<M> {
    /// The coin that holds this NFT.
    pub coin: Coin,
    /// The lineage proof for the singleton.
    pub proof: Proof,
    /// The info associated with the NFT, including the metadata.
    pub info: NftInfo<M>,
}

impl<M> Nft<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator>,
{
    pub fn new(coin: Coin, proof: Proof, info: NftInfo<M>) -> Self {
        Nft { coin, proof, info }
    }

    /// Creates a coin spend for this NFT.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        inner_spend: Spend,
    ) -> Result<CoinSpend, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let layers = self.info.clone().into_layers(inner_spend.puzzle);

        let puzzle_ptr = layers.construct_puzzle(ctx)?;
        let solution_ptr = layers.construct_solution(
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

        let puzzle = ctx.serialize(&puzzle_ptr)?;
        let solution = ctx.serialize(&solution_ptr)?;

        Ok(CoinSpend::new(self.coin, puzzle, solution))
    }

    /// Returns the lineage proof that would be used by the child.
    pub fn child_lineage_proof(&self) -> LineageProof
    where
        M: ToTreeHash,
    {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    pub fn transfer(
        &self,
        ctx: &mut SpendContext,
        owner_synthetic_key: PublicKey,
        p2_puzzle_hash: Bytes32,
        extra_conditions: Conditions,
    ) -> Result<(CoinSpend, Nft<M>), DriverError>
    where
        M: Clone + ToTreeHash,
    {
        let inner_spend = StandardLayer::new(owner_synthetic_key).spend(
            ctx,
            extra_conditions.create_coin(
                p2_puzzle_hash,
                self.coin.amount,
                vec![p2_puzzle_hash.into()],
            ),
        )?;

        let coin_spend = self.spend(ctx, inner_spend)?;
        let child = self.create_child(p2_puzzle_hash, None);
        Ok((coin_spend, child))
    }

    pub fn transfer_to_did(
        &self,
        ctx: &mut SpendContext,
        owner_synthetic_key: PublicKey,
        p2_puzzle_hash: Bytes32,
        new_did_owner: &NewNftOwner,
        extra_conditions: Conditions,
    ) -> Result<(CoinSpend, Conditions, Nft<M>), DriverError>
    where
        M: Clone + ToTreeHash,
    {
        let new_did_owner_ptr = Condition::Other(ctx.alloc(&new_did_owner)?);

        let inner_spend = StandardLayer::new(owner_synthetic_key).spend(
            ctx,
            extra_conditions
                .create_coin(
                    p2_puzzle_hash,
                    self.coin.amount,
                    vec![p2_puzzle_hash.into()],
                )
                .with(new_did_owner_ptr),
        )?;

        let did_conditions = Conditions::new()
            .assert_puzzle_announcement(did_puzzle_assertion(self.coin.puzzle_hash, new_did_owner));

        let coin_spend = self.spend(ctx, inner_spend)?;
        let child = self.create_child(p2_puzzle_hash, Some(new_did_owner.did_id));
        Ok((coin_spend, did_conditions, child))
    }

    /// Creates a new spendable NFT for the child.
    #[must_use]
    pub fn create_child(&self, p2_puzzle_hash: Bytes32, new_owner: Option<Option<Bytes32>>) -> Self
    where
        M: ToTreeHash + Clone,
    {
        let mut info = self.info.clone();

        info.p2_puzzle_hash = p2_puzzle_hash;

        if let Some(new_owner) = new_owner {
            info.current_owner = new_owner;
        }

        Self {
            coin: Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(info.launcher_id, info.inner_puzzle_hash()).into(),
                self.coin.amount,
            ),
            proof: Proof::Lineage(self.child_lineage_proof()),
            info,
        }
    }
}

impl<M> Primitive for Nft<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + ToTreeHash,
{
    fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
        coin: Coin,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
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
                Condition::Other(condition) => {
                    if let Ok(condition) = NewNftOwner::from_clvm(allocator, condition) {
                        new_owner = Some(condition);
                    } else if let Ok(condition) =
                        NewMetadataCondition::<NodePtr, NodePtr>::from_clvm(allocator, condition)
                    {
                        new_metadata = Some(condition);
                    }
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
            let output = run_puzzle(
                allocator,
                new_metadata.metadata_updater_reveal,
                new_metadata.metadata_updater_solution,
            )?;

            let output =
                NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)?.metadata_part;
            layers.inner_puzzle.metadata = output.new_metadata;
            layers.inner_puzzle.metadata_updater_puzzle_hash = output.new_metadata_updater_puzhash;
        }

        let mut info = NftInfo::from_layers(layers);
        info.p2_puzzle_hash = create_coin.puzzle_hash;

        Ok(Some(Self {
            coin,
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: singleton_layer.inner_puzzle.curried_puzzle_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            info,
        }))
    }
}

#[allow(clippy::missing_panics_doc)]
pub fn did_puzzle_assertion(nft_full_puzzle_hash: Bytes32, new_nft_owner: &NewNftOwner) -> Bytes32 {
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

#[cfg(test)]
mod tests {
    use crate::{nft_mint, IntermediateLauncher, Launcher, NftMint};

    use super::*;

    use chia_bls::DerivableKey;
    use chia_puzzles::{
        nft::{NftMetadata, NFT_METADATA_UPDATER_PUZZLE_HASH},
        standard::StandardArgs,
    };
    use chia_sdk_test::{secret_key, test_transaction, Simulator};

    #[tokio::test]
    async fn test_nft_transfer() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 2).await;

        let (create_did, did) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, nft_mint(puzzle_hash, Some(&did)))?;

        let did = ctx.spend_standard_did(&did, pk, mint_nft)?;

        let other_puzzle_hash = StandardArgs::curry_tree_hash(pk.derive_unhardened(0)).into();

        let (parent_conditions, _) =
            ctx.spend_standard_nft(&nft, pk, other_puzzle_hash, None, Conditions::new())?;

        let _did_info = ctx.spend_standard_did(&did, pk, parent_conditions)?;

        test_transaction(&peer, ctx.take(), &[sk], &sim.config().constants).await;

        Ok(())
    }

    #[tokio::test]
    async fn test_nft_lineage() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 2).await;

        let (create_did, did) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let (mint_nft, mut nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, nft_mint(puzzle_hash, Some(&did)))?;

        let mut did = ctx.spend_standard_did(&did, pk, mint_nft)?;

        for i in 0..5 {
            let (spend_nft, new_nft) = ctx.spend_standard_nft(
                &nft,
                pk,
                nft.info.p2_puzzle_hash,
                if i % 2 == 0 {
                    Some(NewNftOwner::new(
                        Some(did.info.launcher_id),
                        Vec::new(),
                        Some(did.info.inner_puzzle_hash().into()),
                    ))
                } else {
                    None
                },
                Conditions::new(),
            )?;
            nft = new_nft;
            did = ctx.spend_standard_did(&did, pk, spend_nft)?;
        }

        test_transaction(&peer, ctx.take(), &[sk], &sim.config().constants).await;

        let coin_state = sim
            .coin_state(did.coin.coin_id())
            .await
            .expect("expected did coin");
        assert_eq!(coin_state.coin, did.coin);

        let coin_state = sim
            .coin_state(nft.coin.coin_id())
            .await
            .expect("expected nft coin");
        assert_eq!(coin_state.coin, nft.coin);

        Ok(())
    }

    #[test]
    fn test_parse_nft() -> anyhow::Result<()> {
        let mut ctx = SpendContext::new();

        let pk = PublicKey::default();
        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 2);

        let (create_did, did) =
            Launcher::new(parent.coin_id(), 1).create_simple_did(&mut ctx, pk)?;

        let (mint_nft, nft) = Launcher::new(did.coin.coin_id(), 1).mint_nft(
            &mut ctx,
            NftMint {
                metadata: NftMetadata::default(),
                metadata_updater_puzzle_hash: NFT_METADATA_UPDATER_PUZZLE_HASH.into(),
                royalty_ten_thousandths: 300,
                royalty_puzzle_hash: Bytes32::new([1; 32]),
                p2_puzzle_hash: puzzle_hash,
                owner: NewNftOwner {
                    did_id: Some(did.info.launcher_id),
                    trade_prices: Vec::new(),
                    did_inner_puzzle_hash: Some(did.info.inner_puzzle_hash().into()),
                },
            },
        )?;

        ctx.spend_p2_coin(parent, pk, create_did.extend(mint_nft))?;

        let coin_spends = ctx.take();

        let coin_spend = coin_spends
            .into_iter()
            .find(|cs| cs.coin.coin_id() == nft.coin.parent_coin_info)
            .unwrap();

        let puzzle_ptr = coin_spend.puzzle_reveal.to_clvm(&mut ctx.allocator)?;
        let solution_ptr = coin_spend.solution.to_clvm(&mut ctx.allocator)?;

        let puzzle = Puzzle::parse(&ctx.allocator, puzzle_ptr);
        let parsed_nft = Nft::<NftMetadata>::from_parent_spend(
            &mut ctx.allocator,
            parent,
            puzzle,
            solution_ptr,
            nft.coin,
        )?
        .unwrap();

        assert_eq!(parsed_nft.info, nft.info);

        Ok(())
    }
}
