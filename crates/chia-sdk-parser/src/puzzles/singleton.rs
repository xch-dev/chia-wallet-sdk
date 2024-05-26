#![allow(clippy::missing_const_for_fn)]

use chia_protocol::Bytes32;
use chia_puzzles::singleton::{
    SingletonArgs, SingletonSolution, SINGLETON_LAUNCHER_PUZZLE_HASH,
    SINGLETON_TOP_LAYER_PUZZLE_HASH,
};
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{Allocator, NodePtr};

use crate::{ParseContext, ParseError};

#[derive(Debug, Clone, Copy)]
pub struct ParseSingleton {
    args: SingletonArgs<NodePtr>,
    solution: SingletonSolution<NodePtr>,
    inner_mod_hash: Bytes32,
    inner_args: NodePtr,
    inner_solution: NodePtr,
}

impl ParseSingleton {
    #[must_use]
    pub fn args(&self) -> &SingletonArgs<NodePtr> {
        &self.args
    }

    #[must_use]
    pub fn solution(&self) -> &SingletonSolution<NodePtr> {
        &self.solution
    }

    #[must_use]
    pub fn inner_mod_hash(&self) -> Bytes32 {
        self.inner_mod_hash
    }

    #[must_use]
    pub fn inner_args(&self) -> NodePtr {
        self.inner_args
    }

    #[must_use]
    pub fn inner_solution(&self) -> NodePtr {
        self.inner_solution
    }
}

pub fn parse_singleton(
    allocator: &Allocator,
    ctx: &ParseContext,
) -> Result<Option<ParseSingleton>, ParseError> {
    if ctx.mod_hash().to_bytes() != SINGLETON_TOP_LAYER_PUZZLE_HASH.to_bytes() {
        return Ok(None);
    }

    let singleton_args = SingletonArgs::<NodePtr>::from_clvm(allocator, ctx.args())?;
    let singleton_solution = SingletonSolution::<NodePtr>::from_clvm(allocator, ctx.solution())?;

    let CurriedProgram { program, args } =
        CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, singleton_args.inner_puzzle)?;

    let singleton_mod_hash = singleton_args.singleton_struct.mod_hash.as_ref();
    let launcher_puzzle_hash = singleton_args
        .singleton_struct
        .launcher_puzzle_hash
        .as_ref();

    if singleton_mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH.to_bytes()
        || launcher_puzzle_hash != SINGLETON_LAUNCHER_PUZZLE_HASH.to_bytes()
    {
        return Err(ParseError::InvalidSingletonStruct);
    }

    let inner_solution = singleton_solution.inner_solution;

    Ok(Some(ParseSingleton {
        args: singleton_args,
        solution: singleton_solution,
        inner_mod_hash: tree_hash(allocator, program).into(),
        inner_args: args,
        inner_solution,
    }))
}
