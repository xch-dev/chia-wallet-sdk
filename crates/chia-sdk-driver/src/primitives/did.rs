use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    did::DidSolution, singleton::SingletonSolution, LineageProof, Memos, Proof,
};
use chia_sdk_types::{run_puzzle, Condition, Conditions};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{tree_hash, ToTreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DidLayer, DriverError, HashedPtr, Layer, Puzzle, Singleton, SingletonInfo, SingletonLayer,
    Spend, SpendContext, SpendWithConditions,
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
pub type Did = Singleton<DidInfo>;

impl Did {
    /// Creates a new [`Did`] that represents a child of this one.
    pub fn child(&self, p2_puzzle_hash: Bytes32, metadata: HashedPtr, amount: u64) -> Did {
        self.child_with(
            DidInfo {
                metadata,
                p2_puzzle_hash,
                ..self.info
            },
            amount,
        )
    }

    /// Spends this DID coin with the provided inner spend.
    /// The spend is added to the [`SpendContext`] for convenience.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        inner_spend: Spend,
    ) -> Result<Option<Self>, DriverError> {
        let layers = self.info.into_layers(inner_spend.puzzle);

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

                    let child = self.child(hint, self.info.metadata, create_coin.amount);

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
    ) -> Result<Did, DriverError>
    where
        I: SpendWithConditions,
    {
        let new_info = DidInfo {
            p2_puzzle_hash,
            ..self.info
        };

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

        Ok(self.child(p2_puzzle_hash, new_info.metadata, self.coin.amount))
    }

    /// Updates the metadata of this DID.
    ///
    /// Because DID coins aren't wrapped automatically, and due to the way they are parsed in wallets,
    /// an additional update spend is needed. This additional spend is not handled by this method, so
    /// you will need to do it manually.
    ///
    /// This spend requires a [`Layer`] that supports [`SpendWithConditions`]. If it doesn't, you can
    /// use [`Did::spend_with`] instead.
    pub fn update_with_metadata<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        metadata: HashedPtr,
        extra_conditions: Conditions,
    ) -> Result<Did, DriverError>
    where
        I: SpendWithConditions,
    {
        let new_inner_puzzle_hash = DidInfo {
            metadata,
            ..self.info
        }
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
    ) -> Result<Did, DriverError>
    where
        I: SpendWithConditions,
    {
        self.update_with_metadata(ctx, inner, self.info.metadata, extra_conditions)
    }

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
            DidLayer::<HashedPtr, Puzzle>::parse_puzzle(allocator, singleton_layer.inner_puzzle)?
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
        let did_layer_hashed = did_layer.with_metadata(metadata_hash);

        let parent_inner_puzzle_hash = did_layer_hashed.tree_hash().into();
        let layers = SingletonLayer::new(singleton_layer.launcher_id, did_layer);

        let mut info = DidInfo::from_layers(&layers);
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

    /// Parses a [`Did`] and its p2 spend from a coin spend.
    ///
    /// If the puzzle is not a DID, this will return [`None`] instead of an error.
    /// However, if the puzzle should have been a DID but had a parsing error, this will return an error.
    #[allow(clippy::type_complexity)]
    pub fn parse(
        allocator: &Allocator,
        coin: Coin,
        puzzle: Puzzle,
        solution: NodePtr,
    ) -> Result<Option<(Self, Option<(Puzzle, NodePtr)>)>, DriverError>
    where
        Self: Sized,
    {
        let Some((did_info, p2_puzzle)) = DidInfo::parse(allocator, puzzle)? else {
            return Ok(None);
        };

        let singleton_solution = SingletonLayer::<Puzzle>::parse_solution(allocator, solution)?;

        let did_solution = DidLayer::<HashedPtr, Puzzle>::parse_solution(
            allocator,
            singleton_solution.inner_solution,
        )?;

        Ok(Some((
            Self::new(coin, singleton_solution.lineage_proof, did_info),
            if let DidSolution::Spend(p2_solution) = did_solution {
                Some((p2_puzzle, p2_solution))
            } else {
                None
            },
        )))
    }
}

#[cfg(test)]
mod tests {
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
        metadata: impl ToClvm<Allocator> + FromClvm<Allocator>,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let metadata = ctx.alloc_hashed(&metadata)?;

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let launcher = Launcher::new(alice.coin.coin_id(), 1);
        let (create_did, did) = launcher.create_did(
            ctx,
            recovery_list_hash,
            num_verifications_required,
            metadata,
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

        let new_metadata = ctx.alloc_hashed(&"New Metadata")?;
        let updated_did =
            did.update_with_metadata(ctx, &alice_p2, new_metadata, Conditions::default())?;

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

        let did = Did::parse_child(
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
