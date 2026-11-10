use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_types::{
    Condition, Conditions,
    puzzles::{
        P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH, P2MOfNDelegateDirectArgs,
        P2MOfNDelegateDirectSolution,
    },
};
use clvm_traits::{FromClvm, clvm_quote};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

/// The m-of-n [`Layer`] is a multisig puzzle that requires `m` of the keys in
/// [`public_key_list`](Self::public_key_list) to sign a delegated puzzle.
///
/// Exactly `m` keys must be selected in the solution; selecting more or fewer fails.
///
/// The puzzle is [`P2MOfNDelegateDirectArgs`] and the solution is
/// [`P2MOfNDelegateDirectSolution`], which selects the signing keys and contains the delegated
/// puzzle and solution. Each selected key must sign the delegated puzzle's tree hash with
/// `AGG_SIG_UNSAFE`, so the delegated puzzle should prevent replays, as
/// [`MOfNLayer::ensure_non_replayable`] does.
///
/// # Example
///
/// Spend a coin with one signature from a 1-of-2 multisig:
///
/// ```
/// use chia_protocol::Bytes32;
/// use chia_sdk_driver::{MOfNLayer, SpendContext};
/// use chia_sdk_test::{BlsPair, Simulator};
/// use chia_sdk_types::{Conditions, TESTNET11_CONSTANTS};
/// use clvm_utils::ToTreeHash;
///
/// let mut sim = Simulator::new();
/// let ctx = &mut SpendContext::new();
///
/// let [alice, bob] = BlsPair::range();
/// let layer = MOfNLayer::new(1, vec![alice.pk, bob.pk]);
///
/// let coin = sim.new_coin(layer.tree_hash().into(), 1_000);
///
/// layer.spend(
///     ctx,
///     coin,
///     Conditions::new().reserve_fee(1_000),
///     &[bob.pk],
///     TESTNET11_CONSTANTS.genesis_challenge,
/// )?;
/// sim.spend_coins(ctx.take(), &[bob.sk])?;
///
/// assert!(sim.coin_state(coin.coin_id()).is_some_and(|state| state.spent_height.is_some()));
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MOfNLayer {
    /// The number of signatures required. The solution must select exactly this many keys.
    pub m: usize,
    /// The public keys that can sign.
    pub public_key_list: Vec<PublicKey>,
}

impl Layer for MOfNLayer {
    type Solution = P2MOfNDelegateDirectSolution<NodePtr, NodePtr>;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(P2MOfNDelegateDirectArgs::new(
            self.m,
            self.public_key_list.clone(),
        ))
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2MOfNDelegateDirectArgs::from_clvm(allocator, puzzle.args)?;

        Ok(Some(Self {
            m: args.m,
            public_key_list: args.public_key_list,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2MOfNDelegateDirectSolution::from_clvm(
            allocator, solution,
        )?)
    }
}

impl MOfNLayer {
    /// Creates a new [`MOfNLayer`] that requires `m` of the keys in `public_key_list`.
    pub fn new(m: usize, public_key_list: Vec<PublicKey>) -> Self {
        Self { m, public_key_list }
    }

    /// Makes `conditions` specific to one coin and one network so that the signatures can't be
    /// reused.
    ///
    /// Adds `ASSERT_MY_COIN_ID` for `coin_id` unless `conditions` already contains an
    /// `ASSERT_MY_COIN_ID` or `ASSERT_MY_PARENT_ID` condition, and always adds a `REMARK` with
    /// `genesis_challenge`.
    pub fn ensure_non_replayable(
        conditions: Conditions,
        coin_id: Bytes32,
        genesis_challenge: NodePtr,
    ) -> Conditions {
        let found_condition = conditions.clone().into_iter().find(|c| {
            matches!(c, Condition::AssertMyCoinId(..))
                || matches!(c, Condition::AssertMyParentId(..))
        });

        if found_condition.is_some() {
            conditions
        } else {
            conditions.assert_my_coin_id(coin_id)
        }
        .remark(genesis_challenge)
    }

    /// Spends `coin` with this multisig, outputting `conditions`.
    ///
    /// The conditions are passed through [`MOfNLayer::ensure_non_replayable`] and used as the
    /// delegated puzzle. `used_pubkeys` are the keys that will sign, and the coin spend is
    /// added to `ctx`.
    ///
    /// # Errors
    ///
    /// Returns an error if the puzzle or solution can't be allocated.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        conditions: Conditions,
        used_pubkeys: &[PublicKey],
        genesis_challenge: Bytes32,
    ) -> Result<(), DriverError> {
        let genesis_challenge = ctx.alloc(&genesis_challenge)?;
        let spend = self.spend_with_conditions(
            ctx,
            Self::ensure_non_replayable(conditions, coin.coin_id(), genesis_challenge),
            used_pubkeys,
        )?;
        ctx.spend(coin, spend)
    }

    /// Creates a [`Spend`] of this multisig whose delegated puzzle outputs `conditions`.
    ///
    /// `used_pubkeys` are the keys that will sign. Unlike [`MOfNLayer::spend`], this doesn't add
    /// replay protection to the conditions.
    ///
    /// # Errors
    ///
    /// Returns an error if the puzzle or solution can't be allocated.
    pub fn spend_with_conditions(
        &self,
        ctx: &mut SpendContext,
        conditions: Conditions,
        used_pubkeys: &[PublicKey],
    ) -> Result<Spend, DriverError> {
        let delegated_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        self.construct_spend(
            ctx,
            P2MOfNDelegateDirectSolution {
                selectors: P2MOfNDelegateDirectArgs::selectors_for_used_pubkeys(
                    &self.public_key_list,
                    used_pubkeys,
                ),
                delegated_puzzle,
                delegated_solution: NodePtr::NIL,
            },
        )
    }
}

impl ToTreeHash for MOfNLayer {
    fn tree_hash(&self) -> TreeHash {
        P2MOfNDelegateDirectArgs::curry_tree_hash(self.m, self.public_key_list.clone())
    }
}
