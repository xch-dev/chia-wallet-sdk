use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    LineageProof, Memos, Proof, did::DidSolution, singleton::SingletonSolution,
};
use chia_sdk_types::{Condition, Conditions, conditions::CreateCoin, run_puzzle};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, tree_hash};
use clvmr::{Allocator, NodePtr};

use crate::{
    ClawbackV2, DidLayer, DriverError, HashedPtr, Layer, Puzzle, Singleton, SingletonInfo,
    SingletonLayer, Spend, SpendContext, SpendWithConditions,
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
            if let Some(create_coin) = condition.into_create_coin()
                && create_coin.amount % 2 == 1
            {
                let Memos::Some(memos) = create_coin.memos else {
                    return Ok(None);
                };

                let Some((hint, rest)) = <(Bytes32, NodePtr)>::from_clvm(ctx, memos).ok() else {
                    return Ok(None);
                };

                if let Some(p2_puzzle_hash) = child_p2_puzzle_hash(
                    ctx,
                    self.info,
                    create_coin.puzzle_hash,
                    create_coin.amount,
                    hint,
                    rest,
                ) {
                    return Ok(Some(self.child(
                        p2_puzzle_hash,
                        self.info.metadata,
                        create_coin.amount,
                    )));
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
    /// If the DID was sent with a [`ClawbackV2`], the p2 puzzle hash of the child will be the
    /// clawback's puzzle hash, rather than the hinted receiver's puzzle hash.
    ///
    /// The hint is verified against the puzzle hash of the created coin, and the provided `coin`
    /// must be the child that was created. Otherwise, this will return [`None`].
    ///
    /// If the child has a p2 puzzle hash that can't be determined from its memos, you can use
    /// [`Did::parse_child_trusted`] instead.
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
        let Some(parent) =
            ParentSpend::parse(allocator, parent_coin, parent_puzzle, parent_solution)?
        else {
            return Ok(None);
        };

        let Memos::Some(memos) = parent.create_coin.memos else {
            return Err(DriverError::MissingHint);
        };

        let (hint, rest) = <(Bytes32, NodePtr)>::from_clvm(allocator, memos)?;

        let Some(p2_puzzle_hash) = child_p2_puzzle_hash(
            allocator,
            parent.info,
            parent.create_coin.puzzle_hash,
            parent.create_coin.amount,
            hint,
            rest,
        ) else {
            return Ok(None);
        };

        let child = parent.child(p2_puzzle_hash);

        if child.coin != coin {
            return Ok(None);
        }

        Ok(Some(child))
    }

    /// Parses the child of an [`Did`] from the parent coin spend, using the provided p2 puzzle hash.
    ///
    /// This relies on the child having the same metadata as the parent, but unlike
    /// [`Did::parse_child`], it doesn't use the hint and supports any p2 puzzle hash.
    ///
    /// The p2 puzzle hash is trusted and isn't verified against the puzzle hash of the created coin.
    /// The coin of the returned [`Did`] is calculated from it, so if the p2 puzzle hash is wrong it
    /// will not match the actual child coin. Make sure to check this if the p2 puzzle hash came from
    /// an untrusted source, such as a hint.
    pub fn parse_child_trusted(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
        p2_puzzle_hash: Bytes32,
    ) -> Result<Option<Self>, DriverError>
    where
        Self: Sized,
    {
        let Some(parent) =
            ParentSpend::parse(allocator, parent_coin, parent_puzzle, parent_solution)?
        else {
            return Ok(None);
        };

        Ok(Some(parent.child(p2_puzzle_hash)))
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

/// The parts of a DID's parent spend that are needed to construct its child.
struct ParentSpend {
    coin: Coin,
    inner_puzzle_hash: Bytes32,
    info: DidInfo,
    create_coin: CreateCoin<NodePtr>,
}

impl ParentSpend {
    /// Parses the parent DID spend. If the parent isn't a DID or was melted, this returns [`None`].
    fn parse(
        allocator: &mut Allocator,
        coin: Coin,
        puzzle: Puzzle,
        solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let Some(singleton_layer) = SingletonLayer::<Puzzle>::parse_puzzle(allocator, puzzle)?
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

        let singleton_solution = SingletonLayer::<Puzzle>::parse_solution(allocator, solution)?;

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
            // The DID was melted, so there is no child.
            return Ok(None);
        };

        let metadata_ptr = did_layer.metadata.to_clvm(allocator)?;
        let metadata_hash = tree_hash(allocator, metadata_ptr);
        let did_layer_hashed = did_layer.with_metadata(metadata_hash);

        let inner_puzzle_hash = did_layer_hashed.tree_hash().into();
        let layers = SingletonLayer::new(singleton_layer.launcher_id, did_layer);

        Ok(Some(Self {
            coin,
            inner_puzzle_hash,
            info: DidInfo::from_layers(&layers),
            create_coin,
        }))
    }

    /// Constructs the child DID with the given p2 puzzle hash.
    fn child(&self, p2_puzzle_hash: Bytes32) -> Did {
        let info = DidInfo {
            p2_puzzle_hash,
            ..self.info
        };

        Did::new(
            Coin::new(
                self.coin.coin_id(),
                info.puzzle_hash().into(),
                self.create_coin.amount,
            ),
            Proof::Lineage(LineageProof {
                parent_parent_coin_info: self.coin.parent_coin_info,
                parent_inner_puzzle_hash: self.inner_puzzle_hash,
                parent_amount: self.coin.amount,
            }),
            info,
        )
    }
}

/// Determines the p2 puzzle hash of a DID child from the memos of the `CREATE_COIN` that created it.
///
/// Usually the hint is the p2 puzzle hash itself. However, if the DID was sent with a [`ClawbackV2`],
/// the hint is the receiver's puzzle hash and the next memo is the clawback memo, so the p2 puzzle
/// hash is the clawback's puzzle hash instead.
///
/// The memos are untrusted, so a candidate is only returned if it actually produces the puzzle hash
/// of the child's inner puzzle.
fn child_p2_puzzle_hash(
    allocator: &Allocator,
    info: DidInfo,
    inner_puzzle_hash: Bytes32,
    amount: u64,
    hint: Bytes32,
    rest: NodePtr,
) -> Option<Bytes32> {
    let produces_child = |p2_puzzle_hash: Bytes32| {
        DidInfo {
            p2_puzzle_hash,
            ..info
        }
        .inner_puzzle_hash()
            == inner_puzzle_hash.into()
    };

    if produces_child(hint) {
        return Some(hint);
    }

    let (memo, _) = <(NodePtr, NodePtr)>::from_clvm(allocator, rest).ok()?;
    let (sender_puzzle_hash, (seconds, ())) =
        <(Bytes32, (u64, ()))>::from_clvm(allocator, memo).ok()?;

    // Whether the clawback hints its outputs isn't included in the memo, so try both.
    [true, false].into_iter().find_map(|hinted| {
        let p2_puzzle_hash = ClawbackV2::new(sender_puzzle_hash, hint, seconds, amount, hinted)
            .tree_hash()
            .into();
        produces_child(p2_puzzle_hash).then_some(p2_puzzle_hash)
    })
}

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_protocol::{Bytes32, CoinSpend};
    use chia_sdk_test::Simulator;
    use clvm_traits::clvm_list;
    use rstest::rstest;

    use crate::{HashedPtr, Launcher, SpendWithConditions, StandardLayer};

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

    fn parse_child_from_spend(
        ctx: &mut SpendContext,
        coin_spend: &CoinSpend,
        coin: Coin,
    ) -> anyhow::Result<Option<Did>> {
        let puzzle = ctx.alloc(&coin_spend.puzzle_reveal)?;
        let puzzle = Puzzle::parse(ctx, puzzle);
        let solution = ctx.alloc(&coin_spend.solution)?;
        Ok(Did::parse_child(
            ctx,
            coin_spend.coin,
            puzzle,
            solution,
            coin,
        )?)
    }

    #[test]
    fn test_parse_did_child_verifies_hint() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);
        let bob = sim.bls(0);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        // Send the DID to Bob, but hint it to Alice so it would show up in her wallet.
        let bob_info = DidInfo {
            p2_puzzle_hash: bob.puzzle_hash,
            ..did.info
        };
        let memos = ctx.hint(alice.puzzle_hash)?;
        did.spend_with(
            ctx,
            &alice_p2,
            Conditions::new().create_coin(bob_info.inner_puzzle_hash().into(), 1, memos),
        )?;

        let coin_spend = ctx.take().remove(0);
        let child_coin = Coin::new(did.coin.coin_id(), bob_info.puzzle_hash().into(), 1);
        let spoofed_coin = Coin::new(
            did.coin.coin_id(),
            DidInfo {
                p2_puzzle_hash: alice.puzzle_hash,
                ..did.info
            }
            .puzzle_hash()
            .into(),
            1,
        );

        assert_eq!(parse_child_from_spend(ctx, &coin_spend, child_coin)?, None);
        assert_eq!(
            parse_child_from_spend(ctx, &coin_spend, spoofed_coin)?,
            None
        );

        Ok(())
    }

    #[rstest]
    fn test_parse_did_child_with_clawback(
        #[values(false, true)] hinted: bool,
        #[values(false, true)] claw_back: bool,
    ) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);
        let bob = sim.bls(0);
        let bob_p2 = StandardLayer::new(bob.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        // Send the DID to Bob with a clawback, hinted to Bob so it shows up in his wallet.
        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, hinted);
        let clawback_puzzle_hash: Bytes32 = clawback.tree_hash().into();
        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, clawback.memo()))?;

        let expected_did = did.child(clawback_puzzle_hash, did.info.metadata, 1);

        let spent_did = did.spend_with(
            ctx,
            &alice_p2,
            Conditions::new().create_coin(expected_did.info.inner_puzzle_hash().into(), 1, memos),
        )?;
        assert_eq!(spent_did, Some(expected_did));

        let coin_spend = ctx.take().remove(0);
        sim.spend_coins(vec![coin_spend.clone()], slice::from_ref(&alice.sk))?;
        assert!(sim.coin_state(expected_did.coin.coin_id()).is_some());

        let clawback_did = parse_child_from_spend(ctx, &coin_spend, expected_did.coin)?
            .expect("could not parse clawback did");
        assert_eq!(clawback_did, expected_did);
        assert_eq!(clawback_did.info.p2_puzzle_hash, clawback_puzzle_hash);

        // Make sure the parsed DID can actually be spent by either the sender or the receiver.
        let (owner, owner_p2, owner_sk) = if claw_back {
            (alice.puzzle_hash, alice_p2, alice.sk)
        } else {
            sim.set_next_timestamp(100)?;
            (bob.puzzle_hash, bob_p2, bob.sk)
        };

        let owner_info = DidInfo {
            p2_puzzle_hash: owner,
            ..clawback_did.info
        };
        let memos = ctx.hint(owner)?;
        let inner_spend = owner_p2.spend_with_conditions(
            ctx,
            Conditions::new().create_coin(owner_info.inner_puzzle_hash().into(), 1, memos),
        )?;
        let clawback_spend = if claw_back {
            clawback.sender_spend(ctx, inner_spend)?
        } else {
            clawback.receiver_spend(ctx, inner_spend)?
        };

        let child = clawback_did
            .spend(ctx, clawback_spend)?
            .expect("missing child");
        assert_eq!(child.info.p2_puzzle_hash, owner);

        let coin_spend = ctx.take().remove(0);
        sim.spend_coins(vec![coin_spend.clone()], &[owner_sk])?;
        assert!(sim.coin_state(child.coin.coin_id()).is_some());

        assert_eq!(
            parse_child_from_spend(ctx, &coin_spend, child.coin)?,
            Some(child)
        );

        Ok(())
    }

    #[test]
    fn test_parse_did_child_verifies_clawback_memo() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);
        let bob = sim.bls(0);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        // Send the DID to a clawback from Alice to Bob, but claim in the memo that Bob is the sender.
        let clawback = ClawbackV2::new(alice.puzzle_hash, bob.puzzle_hash, 100, 1, true);
        let spoofed_clawback = ClawbackV2::new(bob.puzzle_hash, bob.puzzle_hash, 100, 1, true);
        let child = did.child(clawback.tree_hash().into(), did.info.metadata, 1);

        let memos = ctx.memos(&clvm_list!(bob.puzzle_hash, spoofed_clawback.memo()))?;
        let spent_did = did.spend_with(
            ctx,
            &alice_p2,
            Conditions::new().create_coin(child.info.inner_puzzle_hash().into(), 1, memos),
        )?;
        assert_eq!(spent_did, None);

        let coin_spend = ctx.take().remove(0);
        let spoofed_child = did.child(spoofed_clawback.tree_hash().into(), did.info.metadata, 1);

        assert_eq!(parse_child_from_spend(ctx, &coin_spend, child.coin)?, None);
        assert_eq!(
            parse_child_from_spend(ctx, &coin_spend, spoofed_child.coin)?,
            None
        );

        Ok(())
    }

    #[test]
    fn test_parse_did_child_trusted() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);
        let bob = sim.bls(0);
        let bob_p2 = StandardLayer::new(bob.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        // Send the DID to Bob without hinting it, so the p2 puzzle hash can't be found from the memos.
        let expected_did = did.child(bob.puzzle_hash, did.info.metadata, 1);
        did.spend_with(
            ctx,
            &alice_p2,
            Conditions::new().create_coin(
                expected_did.info.inner_puzzle_hash().into(),
                1,
                Memos::None,
            ),
        )?;

        let coin_spend = ctx.take().remove(0);
        sim.spend_coins(vec![coin_spend.clone()], slice::from_ref(&alice.sk))?;

        let puzzle = ctx.alloc(&coin_spend.puzzle_reveal)?;
        let puzzle = Puzzle::parse(ctx, puzzle);
        let solution = ctx.alloc(&coin_spend.solution)?;

        assert!(matches!(
            Did::parse_child(ctx, coin_spend.coin, puzzle, solution, expected_did.coin),
            Err(DriverError::MissingHint)
        ));

        let bob_did =
            Did::parse_child_trusted(ctx, coin_spend.coin, puzzle, solution, bob.puzzle_hash)?
                .expect("could not parse did");
        assert_eq!(bob_did, expected_did);

        // The p2 puzzle hash isn't verified, but the coin won't match the actual child.
        let alice_did =
            Did::parse_child_trusted(ctx, coin_spend.coin, puzzle, solution, alice.puzzle_hash)?
                .expect("could not parse did");
        assert_eq!(alice_did.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_ne!(alice_did.coin, expected_did.coin);

        // Make sure the trusted DID can actually be spent.
        let child = bob_did.update(ctx, &bob_p2, Conditions::new())?;
        sim.spend_coins(ctx.take(), &[bob.sk])?;
        assert!(sim.coin_state(child.coin.coin_id()).is_some());

        Ok(())
    }

    #[test]
    fn test_parse_did_child_verifies_coin() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        let child = did.update(ctx, &alice_p2, Conditions::new())?;
        let coin_spend = ctx.take().remove(0);

        assert_eq!(
            parse_child_from_spend(ctx, &coin_spend, child.coin)?,
            Some(child)
        );

        // Any other coin isn't the child, even if it has the same puzzle hash.
        let unrelated_coin = Coin::new(Bytes32::default(), child.coin.puzzle_hash, 1);
        assert_eq!(
            parse_child_from_spend(ctx, &coin_spend, unrelated_coin)?,
            None
        );

        Ok(())
    }

    #[test]
    fn test_parse_melted_did_child() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;
        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        did.spend_with(ctx, &alice_p2, Conditions::new().melt_singleton())?;
        let coin_spend = ctx.take().remove(0);

        assert_eq!(parse_child_from_spend(ctx, &coin_spend, did.coin)?, None);

        // Make sure the melt is actually valid.
        sim.spend_coins(vec![coin_spend], slice::from_ref(&alice.sk))?;

        Ok(())
    }

    #[test]
    fn test_parse_did_info_launcher_id_mismatch() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();

        let info = DidInfo::new(
            Bytes32::new([1; 32]),
            None,
            1,
            HashedPtr::NIL,
            Bytes32::default(),
        );
        let mut layers = info.into_layers(HashedPtr::NIL);
        layers.inner_puzzle.launcher_id = Bytes32::new([2; 32]);

        let puzzle = layers.construct_puzzle(ctx)?;
        let puzzle = Puzzle::parse(ctx, puzzle);

        assert!(matches!(
            DidInfo::parse(ctx, puzzle),
            Err(DriverError::InvalidSingletonStruct)
        ));

        Ok(())
    }
}
