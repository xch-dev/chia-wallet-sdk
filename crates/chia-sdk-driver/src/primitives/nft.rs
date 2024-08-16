use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend, Program};
use chia_puzzles::{
    nft::{NftOwnershipLayerSolution, NftStateLayerSolution},
    standard::DEFAULT_HIDDEN_PUZZLE_HASH,
    LineageProof, Proof,
};
use chia_sdk_types::{Condition, CreateCoin, NewNftOwner};
use clvm_traits::{clvm_list, FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash, TreeHash};
use clvmr::{sha2::Sha256, Allocator, NodePtr};

use crate::{Conditions, DriverError, NftOwnershipLayer, NftStateLayer, SingletonLayer, Spend};

#[derive(Debug, Clone, Copy)]
pub struct Nft<M = NodePtr> {
    pub coin: Coin,

    // singleton layer
    pub launcher_id: Bytes32,

    // state layer
    pub metadata: M,

    // ownership layer
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,

    // innermost (owner) layer
    pub p2_puzzle_hash: TreeHash,
    pub p2_puzzle: Option<NodePtr>,
}

impl<M> Nft<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        coin: Coin,
        launcher_id: Bytes32,
        metadata: M,
        current_owner: Option<Bytes32>,
        royalty_puzzle_hash: Bytes32,
        royalty_ten_thousandths: u16,
        p2_puzzle_hash: TreeHash,
        p2_puzzle: Option<NodePtr>,
    ) -> Self {
        Nft {
            coin,
            launcher_id,
            metadata,
            current_owner,
            royalty_puzzle_hash,
            royalty_ten_thousandths,
            p2_puzzle_hash,
            p2_puzzle,
        }
    }

    #[must_use]
    pub fn with_coin(mut self, coin: Coin) -> Self {
        self.coin = coin;
        self
    }

    #[must_use]
    pub fn with_p2_puzzle(mut self, p2_puzzle: NodePtr) -> Self {
        self.p2_puzzle = Some(p2_puzzle);
        self
    }

    pub fn from_parent_spend(
        allocator: &mut Allocator,
        cs: &CoinSpend,
    ) -> Result<Option<Self>, DriverError>
    where
        M: ToTreeHash,
    {
        let puzzle_ptr = cs
            .puzzle_reveal
            .to_clvm(allocator)
            .map_err(DriverError::ToClvm)?;
        let solution_ptr = cs
            .solution
            .to_clvm(allocator)
            .map_err(DriverError::ToClvm)?;

        let res = SingletonLayer::<NftStateLayer<M, NftOwnershipLayer<TransparentLayer>>>::from_parent_spend(
            allocator,
            puzzle_ptr,
            solution_ptr,
        )?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(Nft {
                coin: Coin::new(cs.coin.coin_id(), res.tree_hash().into(), 1),
                launcher_id: res.launcher_id,
                metadata: res.inner_puzzle.metadata,
                current_owner: res.inner_puzzle.inner_puzzle.current_owner,
                royalty_puzzle_hash: res.inner_puzzle.inner_puzzle.royalty_puzzle_hash,
                royalty_ten_thousandths: res.inner_puzzle.inner_puzzle.royalty_ten_thousandths,
                p2_puzzle_hash: res.inner_puzzle.inner_puzzle.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.inner_puzzle.inner_puzzle.puzzle,
            })),
        }
    }

    pub fn from_puzzle(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let res =
            SingletonLayer::<NftStateLayer<M, NftOwnershipLayer<TransparentLayer>>>::from_puzzle(
                allocator, puzzle,
            )?;

        match res {
            None => Ok(None),
            Some(res) => Ok(Some(Nft {
                coin,
                launcher_id: res.launcher_id,
                metadata: res.inner_puzzle.metadata,
                current_owner: res.inner_puzzle.inner_puzzle.current_owner,
                royalty_puzzle_hash: res.inner_puzzle.inner_puzzle.royalty_puzzle_hash,
                royalty_ten_thousandths: res.inner_puzzle.inner_puzzle.royalty_ten_thousandths,
                p2_puzzle_hash: res.inner_puzzle.inner_puzzle.inner_puzzle.puzzle_hash,
                p2_puzzle: res.inner_puzzle.inner_puzzle.inner_puzzle.puzzle,
            })),
        }
    }

    pub fn get_layered_object(
        &self,
        p2_puzzle: Option<NodePtr>,
    ) -> SingletonLayer<NftStateLayer<M, NftOwnershipLayer<TransparentLayer>>>
    where
        M: Clone,
    {
        SingletonLayer {
            launcher_id: self.launcher_id,
            inner_puzzle: NftStateLayer {
                metadata: self.metadata.clone(),
                metadata_updater_puzzle_hash: DEFAULT_HIDDEN_PUZZLE_HASH.into(),
                inner_puzzle: NftOwnershipLayer {
                    launcher_id: self.launcher_id,
                    current_owner: self.current_owner,
                    royalty_puzzle_hash: self.royalty_puzzle_hash,
                    royalty_ten_thousandths: self.royalty_ten_thousandths,
                    inner_puzzle: TransparentLayer {
                        puzzle_hash: self.p2_puzzle_hash,
                        puzzle: match self.p2_puzzle {
                            Some(p2_puzzle) => Some(p2_puzzle),
                            None => p2_puzzle,
                        },
                    },
                },
            },
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        lineage_proof: Proof,
        inner_spend: Spend,
    ) -> Result<(CoinSpend, Nft<M>, Proof), DriverError>
    where
        M: Clone + ToTreeHash,
    {
        let thing = self.get_layered_object(Some(inner_spend.puzzle()));

        let puzzle_ptr = thing.construct_puzzle(ctx)?;
        let puzzle =
            Program::from_clvm(ctx.allocator(), puzzle_ptr).map_err(DriverError::FromClvm)?;

        let solution_ptr = thing.construct_solution(
            ctx,
            SingletonLayerSolution {
                lineage_proof,
                amount: self.coin.amount,
                inner_solution: NftStateLayerSolution {
                    inner_solution: NftOwnershipLayerSolution {
                        inner_solution: inner_spend.solution(),
                    },
                },
            },
        )?;
        let solution =
            Program::from_clvm(ctx.allocator(), solution_ptr).map_err(DriverError::FromClvm)?;

        let cs = CoinSpend {
            coin: self.coin,
            puzzle_reveal: puzzle,
            solution,
        };
        let lineage_proof = thing.lineage_proof_for_child(self.coin.parent_coin_info, 1);
        Ok((
            cs.clone(),
            Nft::from_parent_spend(ctx.allocator_mut(), &cs)?.ok_or(DriverError::MissingChild)?,
            Proof::Lineage(lineage_proof),
        ))
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

impl<M> Nft<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + Clone + ToTreeHash,
{
    pub fn singleton_inner_puzzle_hash(&self) -> TreeHash {
        self.get_layered_object(None).inner_puzzle_hash()
    }

    pub fn lineage_proof_for_child(
        &self,
        my_parent_name: Bytes32,
        my_parent_amount: u64,
    ) -> LineageProof {
        self.get_layered_object(None)
            .lineage_proof_for_child(my_parent_name, my_parent_amount)
    }
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
