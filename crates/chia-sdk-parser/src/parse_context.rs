#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin};
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{Allocator, NodePtr};

use crate::ParseError;

#[derive(Debug, Clone, Copy)]
pub struct ParseContext {
    mod_hash: Bytes32,
    args: NodePtr,
    solution: NodePtr,
    parent_coin: Coin,
    coin: Coin,
}

impl ParseContext {
    #[must_use]
    pub fn mod_hash(&self) -> Bytes32 {
        self.mod_hash
    }

    #[must_use]
    pub fn args(&self) -> NodePtr {
        self.args
    }

    #[must_use]
    pub fn solution(&self) -> NodePtr {
        self.solution
    }

    #[must_use]
    pub fn parent_coin(&self) -> Coin {
        self.parent_coin
    }

    #[must_use]
    pub fn coin(&self) -> Coin {
        self.coin
    }
}

pub fn parse_puzzle(
    allocator: &Allocator,
    parent_puzzle: NodePtr,
    parent_solution: NodePtr,
    parent_coin: Coin,
    coin: Coin,
) -> Result<ParseContext, ParseError> {
    let CurriedProgram { program, args } = CurriedProgram::from_clvm(allocator, parent_puzzle)?;

    Ok(ParseContext {
        mod_hash: tree_hash(allocator, program).into(),
        args,
        solution: parent_solution,
        parent_coin,
        coin,
    })
}
