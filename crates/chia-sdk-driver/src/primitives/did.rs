use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    did::DidSolution,
    singleton::{SingletonArgs, SingletonSolution},
    LineageProof, Memos, Proof,
};
use chia_sdk_types::{run_puzzle, Condition, Conditions};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DidLayer, DriverError, Layer, Puzzle, SingletonLayer, Spend, SpendContext, SpendWithConditions,
};

mod did_info;
mod did_launcher;

pub use did_info::*;

/// Contains all information needed to spend the outer puzzles of DID coins.
/// The [`DidInfo`] is used to construct the puzzle, but the [`Proof`] is needed for the solution.
///
/// The only thing missing to create a valid coin spend is the inner puzzle and solution.
/// However, this is handled separately to provide as much flexibility as possible.
///
/// This type should contain all of the information you need to store in a database for later.
/// As long as you can figure out what puzzle the p2 puzzle hash corresponds to and spend it,
/// you have enough information to spend the DID coin.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Did<M> {
    /// The coin that this [`Did`] represents. Its puzzle hash should match the [`DidInfo::puzzle_hash`].
    pub coin: Coin,

    /// The proof is needed by the singleton puzzle to prove that this coin is a legitimate singleton.
    /// It's typically obtained by looking up and parsing the parent coin.
    ///
    /// Note that while the proof will be a [`LineageProof`] for most coins, for the first singleton
    /// in the lineage it will be an [`EveProof`](chia_puzzle_types::EveProof) instead.
    /// However, the eve coin is typically unhinted and spent in the same transaction as it was created,
    /// so this is not relevant for database storage or syncing unspent coins.
    pub proof: Proof,

    /// The information needed to construct the outer puzzle of a DID. See [`DidInfo`] for more details.
    pub info: DidInfo<M>,
}

impl<M> Did<M> {
    pub fn new(coin: Coin, proof: Proof, info: DidInfo<M>) -> Self {
        Self { coin, proof, info }
    }

    pub fn with_metadata<N>(self, metadata: N) -> Did<N> {
        Did {
            coin: self.coin,
            proof: self.proof,
            info: self.info.with_metadata(metadata),
        }
    }
}

impl<M> Did<M>
where
    M: ToTreeHash,
{
    /// Creates a [`LineageProof`] for which would be valid for any children created by this [`Did`].
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    /// Creates a new [`Did`] that represents a child of this one.
    pub fn child<N>(&self, p2_puzzle_hash: Bytes32, metadata: N, amount: u64) -> Did<N>
    where
        M: Clone,
        N: ToTreeHash,
    {
        let mut info = self.info.clone().with_metadata(metadata);
        info.p2_puzzle_hash = p2_puzzle_hash;
        self.child_with(info, amount)
    }

    /// Creates a new [`Did`] that represents a child of this one.
    ///
    /// You can specify the [`DidInfo`] to use for the child manually.
    /// In most cases, you will want to use [`Did::child`] instead.
    ///
    /// It's important to use the right [`DidInfo`] beforehand, otherwise
    /// the puzzle hash of the child will not match the one expected by the coin.
    pub fn child_with<N>(&self, info: DidInfo<N>, amount: u64) -> Did<N>
    where
        N: ToTreeHash,
    {
        Did::new(
            Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(info.launcher_id, info.inner_puzzle_hash()).into(),
                amount,
            ),
            Proof::Lineage(self.child_lineage_proof()),
            info,
        )
    }
}

impl<M> Did<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + ToTreeHash + Clone,
{
    /// Spends this DID coin with the provided inner spend.
    /// The spend is added to the [`SpendContext`] for convenience.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        inner_spend: Spend,
    ) -> Result<Option<Self>, DriverError> {
        let layers = self.info.clone().into_layers(inner_spend.puzzle);

        let spend = layers.construct_spend(
            ctx,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution: DidSolution::Spend(inner_spend.solution),
            },
        )?;

        ctx.spend(self.coin, spend)?;

        let output = ctx.run(inner_spend.puzzle, inner_spend.solution)?;
        let conditions = Vec::<Condition>::from_clvm(ctx, output)?;

        for condition in conditions {
            if let Some(create_coin) = condition.into_create_coin() {
                if create_coin.amount % 2 == 1 {
                    let Memos::Some(memos) = create_coin.memos else {
                        return Ok(None);
                    };

                    let Some((hint, _)) = <(Bytes32, NodePtr)>::from_clvm(ctx, memos).ok() else {
                        return Ok(None);
                    };

                    let child = self.child(hint, self.info.metadata.clone(), create_coin.amount);

                    if child.info.inner_puzzle_hash() == create_coin.puzzle_hash.into() {
                        return Ok(Some(child));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Spends this DID coin with a [`Layer`] that supports [`SpendWithConditions`].
    /// This is a building block for built in spend methods, but can also be used to spend
    /// DID coins with conditions more easily.
    ///
    /// However, if you need full flexibility of the inner spend, you can use [`Did::spend`] instead.
    pub fn spend_with<I>(
        &self,
        ctx: &mut SpendContext,
        inner: &I,
        conditions: Conditions,
    ) -> Result<Option<Self>, DriverError>
    where
        I: SpendWithConditions,
    {
        let inner_spend = inner.spend_with_conditions(ctx, conditions)?;
        self.spend(ctx, inner_spend)
    }

    /// Transfers this DID coin to a new p2 puzzle hash.
    ///
    /// This spend requires a [`Layer`] that supports [`SpendWithConditions`]. If it doesn't, you can
    /// use [`Did::spend_with`] instead.
    pub fn transfer<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        extra_conditions: Conditions,
    ) -> Result<Did<M>, DriverError>
    where
        M: ToTreeHash,
        I: SpendWithConditions,
    {
        let mut new_info = self.info.clone();
        new_info.p2_puzzle_hash = p2_puzzle_hash;

        let memos = ctx.hint(p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions.create_coin(
                new_info.inner_puzzle_hash().into(),
                self.coin.amount,
                memos,
            ),
        )?;

        let metadata = self.info.metadata.clone();

        Ok(self.child(p2_puzzle_hash, metadata, self.coin.amount))
    }

    /// Updates the metadata of this DID.
    ///
    /// Because DID coins aren't wrapped automatically, and due to the way they are parsed in wallets,
    /// an additional update spend is needed. This additional spend is not handled by this method, so
    /// you will need to do it manually.
    ///
    /// This spend requires a [`Layer`] that supports [`SpendWithConditions`]. If it doesn't, you can
    /// use [`Did::spend_with`] instead.
    pub fn update_with_metadata<I, N>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        metadata: N,
        extra_conditions: Conditions,
    ) -> Result<Did<N>, DriverError>
    where
        I: SpendWithConditions,
        M: ToTreeHash,
        N: ToClvm<Allocator> + ToTreeHash + Clone,
    {
        let new_inner_puzzle_hash = self
            .info
            .clone()
            .with_metadata(metadata.clone())
            .inner_puzzle_hash();

        let memos = ctx.hint(self.info.p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions.create_coin(new_inner_puzzle_hash.into(), self.coin.amount, memos),
        )?;

        Ok(self.child(self.info.p2_puzzle_hash, metadata, self.coin.amount))
    }

    /// Spends the DID without changing its metadata or p2 puzzle hash.
    ///
    /// This can be done to "settle" the DID's updated metadata and make it parseable by wallets.
    /// It's also useful if you just want to emit conditions from the DID, without transferring it.
    /// For example, when assigning a DID to one or more NFTs you can use an update spend to do so.
    ///
    /// This spend requires a [`Layer`] that supports [`SpendWithConditions`]. If it doesn't, you can
    /// use [`Did::spend_with`] instead.
    pub fn update<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        extra_conditions: Conditions,
    ) -> Result<Did<M>, DriverError>
    where
        M: ToTreeHash,
        I: SpendWithConditions,
    {
        let metadata = self.info.metadata.clone();
        self.update_with_metadata(ctx, inner, metadata, extra_conditions)
    }
}

impl<M> Did<M>
where
    M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
{
    /// Parses the child of an [`Did`] from the parent coin spend.
    ///
    /// This relies on the child being hinted and having the same metadata as the parent.
    /// If this is not the case, the DID cannot be parsed or spent without additional context.
    pub fn parse_child(
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

        if singleton_layer.launcher_id != did_layer.launcher_id {
            return Err(DriverError::InvalidSingletonStruct);
        }

        let singleton_solution =
            SingletonLayer::<Puzzle>::parse_solution(allocator, parent_solution)?;

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

        let Memos::Some(memos) = create_coin.memos else {
            return Err(DriverError::MissingHint);
        };

        let (hint, _) = <(Bytes32, NodePtr)>::from_clvm(allocator, memos)?;

        let metadata_ptr = did_layer.metadata.to_clvm(allocator)?;
        let metadata_hash = tree_hash(allocator, metadata_ptr);
        let did_layer_hashed = did_layer.clone().with_metadata(metadata_hash);

        let parent_inner_puzzle_hash = did_layer_hashed.tree_hash().into();
        let layers = SingletonLayer::new(singleton_layer.launcher_id, did_layer);

        let mut info = DidInfo::from_layers(layers);
        info.p2_puzzle_hash = hint;

        Ok(Some(Self {
            coin,
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash,
                parent_amount: parent_coin.amount,
            }),
            info,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use chia_protocol::Bytes32;
    use chia_sdk_test::Simulator;
    use clvm_traits::clvm_list;
    use rstest::rstest;

    use crate::{HashedPtr, Launcher, StandardLayer};

    use super::*;

    #[test]
    fn test_create_and_update_simple_did() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let launcher = Launcher::new(alice.coin.coin_id(), 1);
        let (create_did, did) = launcher.create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert_eq!(did.info.recovery_list_hash, None);
        assert_eq!(did.info.num_verifications_required, 1);
        assert_eq!(did.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }

    #[rstest]
    fn test_create_and_update_did(
        #[values(None, Some(Bytes32::default()))] recovery_list_hash: Option<Bytes32>,
        #[values(0, 1, 3)] num_verifications_required: u64,
        #[values((), "Atom".to_string(), clvm_list!("Complex".to_string(), 42), 100)]
        metadata: impl ToClvm<Allocator>
            + FromClvm<Allocator>
            + ToTreeHash
            + Clone
            + PartialEq
            + fmt::Debug,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let launcher = Launcher::new(alice.coin.coin_id(), 1);
        let (create_did, did) = launcher.create_did(
            ctx,
            recovery_list_hash,
            num_verifications_required,
            metadata.clone(),
            &alice_p2,
        )?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), &[alice.sk])?;

        assert_eq!(did.info.recovery_list_hash, recovery_list_hash);
        assert_eq!(
            did.info.num_verifications_required,
            num_verifications_required
        );
        assert_eq!(did.info.metadata, metadata);
        assert_eq!(did.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }

    #[test]
    fn test_transfer_did() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, alice_did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        let bob = sim.bls(1);
        let bob_p2 = StandardLayer::new(bob.pk);

        let bob_did = alice_did.transfer(ctx, &alice_p2, bob.puzzle_hash, Conditions::new())?;
        let did = bob_did.update(ctx, &bob_p2, Conditions::new())?;

        assert_eq!(did.info.p2_puzzle_hash, bob.puzzle_hash);
        assert_ne!(bob.puzzle_hash, alice.puzzle_hash);

        sim.spend_coins(ctx.take(), &[alice.sk, bob.sk])?;

        Ok(())
    }

    #[test]
    fn test_update_did_metadata() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let launcher = Launcher::new(alice.coin.coin_id(), 1);
        let (create_did, did) = launcher.create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let new_metadata = "New Metadata".to_string();
        let updated_did =
            did.update_with_metadata(ctx, &alice_p2, new_metadata.clone(), Conditions::default())?;

        assert_eq!(updated_did.info.metadata, new_metadata);

        Ok(())
    }

    #[test]
    fn test_nodeptr_metadata() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let launcher = Launcher::new(alice.coin.coin_id(), 1);
        let (create_did, did) = launcher.create_did(ctx, None, 1, HashedPtr::NIL, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let new_metadata = HashedPtr::from_ptr(ctx, ctx.one());
        let updated_did =
            did.update_with_metadata(ctx, &alice_p2, new_metadata, Conditions::default())?;

        assert_eq!(updated_did.info.metadata, new_metadata);

        Ok(())
    }

    #[test]
    fn test_parse_did() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, expected_did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let mut allocator = Allocator::new();

        let puzzle_reveal = sim
            .puzzle_reveal(expected_did.coin.parent_coin_info)
            .expect("missing puzzle")
            .to_clvm(&mut allocator)?;

        let solution = sim
            .solution(expected_did.coin.parent_coin_info)
            .expect("missing solution")
            .to_clvm(&mut allocator)?;

        let parent_coin = sim
            .coin_state(expected_did.coin.parent_coin_info)
            .expect("missing parent coin state")
            .coin;

        let puzzle = Puzzle::parse(&allocator, puzzle_reveal);

        let did = Did::<()>::parse_child(
            &mut allocator,
            parent_coin,
            puzzle,
            solution,
            expected_did.coin,
        )?
        .expect("could not parse did");

        assert_eq!(did, expected_did);

        Ok(())
    }
}
