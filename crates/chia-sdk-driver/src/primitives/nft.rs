use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::{NftOwnershipLayerSolution, NftStateLayerSolution},
    singleton::SingletonSolution,
    LineageProof, Proof,
};
use chia_sdk_types::{Condition, CreateCoin, NewNftOwner};
use clvm_traits::{clvm_list, FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash, TreeHasher};
use clvmr::{sha2::Sha256, Allocator, NodePtr};

use crate::{
    Conditions, DriverError, Layer, NftOwnershipLayer, NftStateLayer, Primitive, Puzzle,
    RoyaltyTransferLayer, SingletonLayer, Spend, SpendContext,
};

use super::NftInfo;

#[derive(Debug, Clone, Copy)]
pub struct Nft<M> {
    pub coin: Coin,
    pub proof: Proof,
    pub info: NftInfo<M>,
}

impl<M> Nft<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator>,
{
    pub fn new(coin: Coin, proof: Proof, info: NftInfo<M>) -> Self {
        Nft { coin, proof, info }
    }

    /// Converts the NFT into a layered puzzle.
    #[must_use]
    pub fn to_layers<I>(
        &self,
        p2_puzzle: I,
    ) -> SingletonLayer<NftStateLayer<M, NftOwnershipLayer<RoyaltyTransferLayer, I>>>
    where
        M: Clone,
    {
        SingletonLayer::new(
            self.info.singleton_struct,
            NftStateLayer::new(
                self.info.metadata.clone(),
                NftOwnershipLayer::new(
                    self.info.current_owner,
                    RoyaltyTransferLayer::new(
                        self.info.singleton_struct,
                        self.info.royalty_puzzle_hash,
                        self.info.royalty_ten_thousandths,
                    ),
                    p2_puzzle,
                ),
            ),
        )
    }

    /// Creates a coin spend for this NFT.
    #[must_use]
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        inner_spend: Spend,
    ) -> Result<CoinSpend, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let layers = self.to_layers(inner_spend.puzzle);

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
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.p2_puzzle_hash.into(),
            parent_amount: self.coin.amount,
        }
    }

    pub fn transfer(
        &self,
        ctx: &mut SpendContext,
        lineage_proof: Proof,
        owner_synthetic_key: PublicKey,
        new_owner_puzzle_hash: Bytes32,
        extra_conditions: Conditions,
    ) -> Result<(CoinSpend, Nft<M>, Proof), DriverError>
    where
        M: Clone + ToTreeHash,
    {
        let p2_conditions = Conditions::new()
            .condition(Condition::CreateCoin(CreateCoin::with_hint(
                new_owner_puzzle_hash,
                self.coin.amount,
                new_owner_puzzle_hash,
            )))
            .extend(extra_conditions);
        let inner_spend = p2_conditions
            .p2_spend(ctx, owner_synthetic_key)
            .map_err(DriverError::Spend)?;

        self.spend(ctx, lineage_proof, inner_spend)
    }

    pub fn transfer_to_did(
        &self,
        ctx: &mut SpendContext,
        lineage_proof: Proof,
        owner_synthetic_key: PublicKey,
        new_owner_puzzle_hash: Bytes32,
        new_did_owner: &NewNftOwner,
        extra_conditions: Conditions,
    ) -> Result<(CoinSpend, Conditions, Nft<M>, Proof), DriverError>
    // (spend, did conditions)
    where
        M: Clone + ToTreeHash,
    {
        let p2_conditions = Conditions::new()
            .conditions(&vec![
                Condition::CreateCoin(CreateCoin::with_hint(
                    new_owner_puzzle_hash,
                    self.coin.amount,
                    new_owner_puzzle_hash,
                )),
                Condition::Other(ctx.alloc(&new_did_owner)?),
            ])
            .extend(extra_conditions);
        let inner_spend = p2_conditions
            .p2_spend(ctx, owner_synthetic_key)
            .map_err(DriverError::Spend)?;

        let did_conditions = Conditions::new().assert_raw_puzzle_announcement(
            did_puzzle_assertion(self.coin.puzzle_hash, new_did_owner),
        );

        let (cs, new_nft, lineage_proof) = self.spend(ctx, lineage_proof, inner_spend)?;
        Ok((cs, did_conditions, new_nft, lineage_proof))
    }
}

impl<M> Primitive for Nft<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + ToClvm<TreeHasher>,
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

        let Some(did_layer) =
            DidLayer::<M, Puzzle>::parse_puzzle(allocator, singleton_layer.inner_puzzle)?
        else {
            return Ok(None);
        };

        if singleton_layer.singleton_struct != did_layer.singleton_struct {
            return Err(DriverError::InvalidSingletonStruct);
        }

        let singleton_solution =
            SingletonLayer::<NodePtr>::parse_solution(allocator, parent_solution)?;

        let output = run_puzzle(
            allocator,
            singleton_layer.inner_puzzle.ptr(),
            singleton_solution.inner_solution,
        )?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let Some(create_coin) = conditions
            .into_iter()
            .filter_map(Condition::into_create_coin)
            .find(|create_coin| create_coin.amount % 2 == 1)
        else {
            return Err(DriverError::MissingChild);
        };

        let Some(hint) = create_coin
            .memos
            .into_iter()
            .filter_map(|memo| memo.try_into().ok())
            .next()
        else {
            return Err(DriverError::MissingHint);
        };

        Ok(Some(Self {
            coin,
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: did_layer.tree_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            info: DidInfo::new(
                did_layer.singleton_struct,
                did_layer.recovery_list_hash,
                did_layer.num_verifications_required,
                did_layer.metadata,
                hint,
            ),
        }))
    }

    /*
    fn from_parent_spend(
            allocator: &mut Allocator,
            layer_puzzle: NodePtr,
            layer_solution: NodePtr,
        ) -> Result<Option<Self>, DriverError> {
            let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

            let Some(parent_puzzle) = parent_puzzle.as_curried() else {
                return Ok(None);
            };

            if parent_puzzle.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH {
                return Ok(None);
            }

            let parent_args =
                NftOwnershipLayerArgs::<NodePtr, NodePtr>::from_clvm(allocator, parent_puzzle.args)
                    .map_err(DriverError::FromClvm)?;

            if parent_args.mod_hash != NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into() {
                return Err(DriverError::InvalidModHash);
            }

            let parent_sol = NftOwnershipLayerSolution::<NodePtr>::from_clvm(allocator, layer_solution)
                .map_err(DriverError::FromClvm)?;

            let new_owner_maybe = NftOwnershipLayer::<IP>::new_owner_from_conditions(
                allocator,
                parent_args.inner_puzzle,
                parent_sol.inner_solution,
            )?;

            let Some(parent_transfer_puzzle) =
                Puzzle::parse(allocator, parent_args.transfer_program).as_curried()
            else {
                return Err(DriverError::NonStandardLayer);
            };

            if parent_transfer_puzzle.mod_hash != NFT_ROYALTY_TRANSFER_PUZZLE_HASH {
                return Err(DriverError::NonStandardLayer);
            }

            let parent_transfer_args =
                NftRoyaltyTransferPuzzleArgs::from_clvm(allocator, parent_transfer_puzzle.args)?;

            match IP::from_parent_spend(
                allocator,
                parent_args.inner_puzzle,
                parent_sol.inner_solution,
            )? {
                None => Ok(None),
                Some(inner_puzzle) => Ok(Some(NftOwnershipLayer::<IP> {
                    launcher_id: parent_transfer_args.singleton_struct.launcher_id,
                    current_owner: new_owner_maybe.unwrap_or(parent_args.current_owner),
                    royalty_puzzle_hash: parent_transfer_args.royalty_puzzle_hash,
                    royalty_ten_thousandths: parent_transfer_args.royalty_ten_thousandths,
                    inner_puzzle,
                })),
            }
        }





        STATE



         fn from_parent_spend(
            allocator: &mut Allocator,
            layer_puzzle: NodePtr,
            layer_solution: NodePtr,
        ) -> Result<Option<Self>, DriverError> {
            let parent_puzzle = Puzzle::parse(allocator, layer_puzzle);

            let Some(parent_puzzle) = parent_puzzle.as_curried() else {
                return Ok(None);
            };

            if parent_puzzle.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH {
                return Ok(None);
            }

            let parent_args = NftStateLayerArgs::<NodePtr, M>::from_clvm(allocator, parent_puzzle.args)
                .map_err(DriverError::FromClvm)?;

            if parent_args.mod_hash != NFT_STATE_LAYER_PUZZLE_HASH.into() {
                return Err(DriverError::InvalidModHash);
            }

            let parent_sol = NftStateLayerSolution::<NodePtr>::from_clvm(allocator, layer_solution)
                .map_err(DriverError::FromClvm)?;

            let (metadata, metadata_updater_puzzle_hash) =
                NftStateLayer::<M, IP>::new_metadata_and_updater_from_conditions(
                    allocator,
                    parent_args.inner_puzzle,
                    parent_sol.inner_solution,
                )?
                .unwrap_or((
                    parent_args.metadata,
                    parent_args.metadata_updater_puzzle_hash,
                ));

            match IP::from_parent_spend(
                allocator,
                parent_args.inner_puzzle,
                parent_sol.inner_solution,
            )? {
                None => Ok(None),
                Some(inner_puzzle) => Ok(Some(NftStateLayer::<M, IP> {
                    metadata,
                    metadata_updater_puzzle_hash,
                    inner_puzzle,
                })),
            }
        }
        */
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

    Bytes32::new(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use crate::{nft_mint, IntermediateLauncher, Launcher, NftMint};

    use super::*;

    use chia_bls::DerivableKey;
    use chia_puzzles::standard::StandardArgs;
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

        let (create_did, did, did_proof) =
            Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let (mint_nft, nft, lineage_proof) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, nft_mint(puzzle_hash, Some(&did)))?;

        let (did, did_proof) = ctx.spend_standard_did(&did, did_proof, pk, mint_nft)?;

        let other_puzzle_hash = StandardArgs::curry_tree_hash(pk.derive_unhardened(0)).into();

        let (parent_conditions, _, _) = ctx.spend_standard_nft(
            &nft,
            lineage_proof,
            pk,
            other_puzzle_hash,
            None,
            Conditions::new(),
        )?;

        let _did_info = ctx.spend_standard_did(&did, did_proof, pk, parent_conditions)?;

        test_transaction(&peer, ctx.take_spends(), &[sk], &sim.config().constants).await;

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

        let (create_did, did, did_proof) =
            Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let (mint_nft, mut nft, mut lineage_proof) =
            IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
                .create(ctx)?
                .mint_nft(ctx, nft_mint(puzzle_hash, Some(&did)))?;

        let (mut did, mut did_proof) = ctx.spend_standard_did(&did, did_proof, pk, mint_nft)?;

        for i in 0..5 {
            let (spend_nft, new_nft, new_lineage_proof) = ctx.spend_standard_nft(
                &nft,
                lineage_proof,
                pk,
                nft.p2_puzzle_hash.into(),
                if i % 2 == 0 {
                    Some(NewNftOwner::new(
                        Some(did.launcher_id),
                        Vec::new(),
                        Some(did.singleton_inner_puzzle_hash().into()),
                    ))
                } else {
                    None
                },
                Conditions::new(),
            )?;
            nft = new_nft;
            lineage_proof = new_lineage_proof;
            (did, did_proof) = ctx.spend_standard_did(&did, did_proof, pk, spend_nft)?;
        }

        test_transaction(&peer, ctx.take_spends(), &[sk], &sim.config().constants).await;

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

        let (create_did, did, _did_proof) =
            Launcher::new(parent.coin_id(), 1).create_simple_did(&mut ctx, pk)?;

        let (mint_nft, nft, _) = Launcher::new(did.coin.coin_id(), 1).mint_nft(
            &mut ctx,
            NftMint {
                metadata: (),
                royalty_percentage: 300,
                royalty_puzzle_hash: Bytes32::new([1; 32]),
                puzzle_hash,
                owner: NewNftOwner {
                    did_id: Some(did.launcher_id),
                    trade_prices: Vec::new(),
                    did_inner_puzzle_hash: Some(did.singleton_inner_puzzle_hash().into()),
                },
            },
        )?;

        ctx.spend_p2_coin(parent, pk, create_did.extend(mint_nft))?;

        let coin_spends = ctx.take_spends();

        let coin_spend = coin_spends
            .into_iter()
            .find(|cs| cs.coin.coin_id() == nft.coin.parent_coin_info)
            .unwrap();

        let mut allocator = ctx.into();

        let puzzle_ptr = coin_spend.puzzle_reveal.to_clvm(&mut allocator)?;
        let solution_ptr = coin_spend.solution.to_clvm(&mut allocator)?;

        let parsed_nft = SingletonLayer::<
            NftStateLayer<(), NftOwnershipLayer<TransparentLayer>>,
        >::from_parent_spend(&mut allocator, puzzle_ptr, solution_ptr)?
        .expect("could not parse spend :(");

        assert_eq!(parsed_nft.launcher_id, nft.launcher_id);
        // assert_eq!(parsed_nft.inner_puzzle.metadata, nft.metadata); // always ()
        assert_eq!(
            parsed_nft.inner_puzzle.inner_puzzle.current_owner,
            nft.current_owner
        );
        assert_eq!(
            parsed_nft.inner_puzzle.inner_puzzle.royalty_puzzle_hash,
            nft.royalty_puzzle_hash
        );
        assert_eq!(
            parsed_nft.inner_puzzle.inner_puzzle.royalty_percentage,
            nft.royalty_percentage
        );
        assert_eq!(
            parsed_nft
                .inner_puzzle
                .inner_puzzle
                .inner_puzzle
                .puzzle_hash,
            nft.p2_puzzle_hash
        );

        Ok(())
    }
}
