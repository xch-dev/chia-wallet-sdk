use chia::{
    bls::PublicKey,
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::{Bytes32, Coin},
};
use chia_wallet_sdk::{
    driver::{DriverError, Layer, Puzzle, Spend, SpendContext},
    types::{Condition, Conditions},
};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::SpendContextExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MOfNLayer {
    pub m: usize,
    pub public_key_list: Vec<PublicKey>,
}

impl Layer for MOfNLayer {
    type Solution = P2MOfNDelegateDirectSolution<NodePtr, NodePtr>;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        CurriedProgram {
            program: ctx.p2_m_of_n_delegate_direct_puzzle()?,
            args: P2MOfNDelegateDirectArgs::new(self.m, self.public_key_list.clone()),
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
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
    pub fn new(m: usize, public_key_list: Vec<PublicKey>) -> Self {
        Self { m, public_key_list }
    }

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

pub const P2_M_OF_N_DELEGATE_DIRECT_PUZZLE: [u8; 453] = hex!("ff02ffff01ff02ffff03ffff09ff05ffff02ff16ffff04ff02ffff04ff17ff8080808080ffff01ff02ff0cffff04ff02ffff04ffff02ff0affff04ff02ffff04ff17ffff04ff0bff8080808080ffff04ffff02ff1effff04ff02ffff04ff2fff80808080ffff04ff2fffff04ff5fff80808080808080ffff01ff088080ff0180ffff04ffff01ffff31ff02ffff03ff05ffff01ff04ffff04ff08ffff04ff09ffff04ff0bff80808080ffff02ff0cffff04ff02ffff04ff0dffff04ff0bffff04ff17ffff04ff2fff8080808080808080ffff01ff02ff17ff2f8080ff0180ffff02ffff03ff05ffff01ff02ffff03ff09ffff01ff04ff13ffff02ff0affff04ff02ffff04ff0dffff04ff1bff808080808080ffff01ff02ff0affff04ff02ffff04ff0dffff04ff1bff808080808080ff0180ff8080ff0180ffff02ffff03ff05ffff01ff10ffff02ff16ffff04ff02ffff04ff0dff80808080ffff02ffff03ff09ffff01ff0101ff8080ff018080ff8080ff0180ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ffff02ff1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");

pub const P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    0f199d5263ac1a62b077c159404a71abd3f9691cc57520bf1d4c5cb501504457
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2MOfNDelegateDirectArgs {
    pub m: usize,
    pub public_key_list: Vec<PublicKey>,
}

impl P2MOfNDelegateDirectArgs {
    pub fn new(m: usize, public_key_list: Vec<PublicKey>) -> Self {
        Self { m, public_key_list }
    }

    pub fn curry_tree_hash(m: usize, public_key_list: Vec<PublicKey>) -> TreeHash {
        CurriedProgram {
            program: P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH,
            args: Self::new(m, public_key_list),
        }
        .tree_hash()
    }

    pub fn selectors_for_used_pubkeys(
        public_key_list: &[PublicKey],
        used_pubkeys: &[PublicKey],
    ) -> Vec<bool> {
        public_key_list
            .iter()
            .map(|pubkey| used_pubkeys.contains(pubkey))
            .collect()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct P2MOfNDelegateDirectSolution<P, S> {
    pub selectors: Vec<bool>,
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}
