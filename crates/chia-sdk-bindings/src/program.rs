use std::sync::{Arc, RwLock};

use bindy::{Error, Result};
use chia_protocol::{Bytes, Program as SerializedProgram};
use chia_puzzle_types::nft;
use chia_sdk_driver::SpendContext;
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm};
use clvm_utils::{tree_hash, TreeHash};
use clvmr::{
    run_program,
    serde::{node_to_bytes, node_to_bytes_backrefs},
    ChiaDialect, NodePtr, SExp, MEMPOOL_MODE,
};
use num_bigint::BigInt;

use crate::{CurriedProgram, NftMetadata, Output, Pair, Puzzle};

#[derive(Clone)]
pub struct Program(pub(crate) Arc<RwLock<SpendContext>>, pub(crate) NodePtr);

impl Program {
    pub fn serialize(&self) -> Result<SerializedProgram> {
        let ctx = self.0.read().unwrap();
        Ok(node_to_bytes(&ctx.allocator, self.1)?.into())
    }

    pub fn serialize_with_backrefs(&self) -> Result<SerializedProgram> {
        let ctx = self.0.read().unwrap();
        Ok(node_to_bytes_backrefs(&ctx.allocator, self.1)?.into())
    }

    pub fn run(&self, solution: Self, max_cost: u64, mempool_mode: bool) -> Result<Output> {
        let mut flags = 0;

        if mempool_mode {
            flags |= MEMPOOL_MODE;
        }

        let mut ctx = self.0.write().unwrap();

        let reduction = run_program(
            &mut ctx.allocator,
            &ChiaDialect::new(flags),
            self.1,
            solution.1,
            max_cost,
        )?;

        Ok(Output {
            value: Program(self.0.clone(), reduction.1),
            cost: reduction.0,
        })
    }

    pub fn curry(&self, args: Vec<Program>) -> Result<Program> {
        let mut ctx = self.0.write().unwrap();

        let mut args_ptr = ctx.allocator.one();

        for arg in args.into_iter().rev() {
            args_ptr = ctx.allocator.encode_curried_arg(arg.1, args_ptr)?;
        }

        let ptr = ctx.alloc(&clvm_utils::CurriedProgram {
            program: self.1,
            args: args_ptr,
        })?;

        Ok(Program(self.0.clone(), ptr))
    }

    pub fn uncurry(&self) -> Result<Option<CurriedProgram>> {
        let ctx = self.0.read().unwrap();

        let Ok(value) =
            clvm_utils::CurriedProgram::<NodePtr, NodePtr>::from_clvm(&ctx.allocator, self.1)
        else {
            return Ok(None);
        };

        let mut args = Vec::new();
        let mut args_ptr = value.args;

        while let Ok((first, rest)) = ctx.allocator.decode_curried_arg(&args_ptr) {
            args.push(first);
            args_ptr = rest;
        }

        if ctx.allocator.small_number(args_ptr) != Some(1) {
            return Ok(None);
        }

        Ok(Some(CurriedProgram {
            program: Program(self.0.clone(), value.program),
            args: args
                .into_iter()
                .map(|ptr| Program(self.0.clone(), ptr))
                .collect(),
        }))
    }

    pub fn tree_hash(&self) -> Result<TreeHash> {
        let ctx = self.0.read().unwrap();
        Ok(tree_hash(&ctx.allocator, self.1))
    }

    pub fn length(&self) -> Result<u32> {
        let ctx = self.0.read().unwrap();

        let SExp::Atom = ctx.allocator.sexp(self.1) else {
            return Err(Error::AtomExpected);
        };

        Ok(ctx.allocator.atom_len(self.1) as u32)
    }

    pub fn first(&self) -> Result<Program> {
        let ctx = self.0.read().unwrap();

        let SExp::Pair(first, _) = ctx.allocator.sexp(self.1) else {
            return Err(Error::PairExpected);
        };

        Ok(Program(self.0.clone(), first))
    }

    pub fn rest(&self) -> Result<Program> {
        let ctx = self.0.read().unwrap();

        let SExp::Pair(_, rest) = ctx.allocator.sexp(self.1) else {
            return Err(Error::PairExpected);
        };

        Ok(Program(self.0.clone(), rest))
    }

    // This is called by the individual napi and wasm crates
    pub fn to_small_int(&self) -> Result<Option<f64>> {
        let ctx = self.0.read().unwrap();

        let SExp::Atom = ctx.allocator.sexp(self.1) else {
            return Ok(None);
        };

        let number = ctx.allocator.number(self.1);

        if number > BigInt::from(9_007_199_254_740_991i64) {
            return Err(Error::TooLarge);
        }

        if number < BigInt::from(-9_007_199_254_740_991i64) {
            return Err(Error::TooSmall);
        }

        let number: u64 = number.try_into().unwrap();

        Ok(Some(number as f64))
    }

    // This is called by the individual binding crates
    pub fn to_big_int(&self) -> Result<Option<BigInt>> {
        let ctx = self.0.read().unwrap();

        let SExp::Atom = ctx.allocator.sexp(self.1) else {
            return Ok(None);
        };

        Ok(Some(ctx.allocator.number(self.1)))
    }

    pub fn to_string(&self) -> Result<Option<String>> {
        let ctx = self.0.read().unwrap();

        let SExp::Atom = ctx.allocator.sexp(self.1) else {
            return Ok(None);
        };

        let bytes = ctx.allocator.atom(self.1);

        Ok(Some(String::from_utf8(bytes.to_vec())?))
    }

    pub fn to_bool(&self) -> Result<Option<bool>> {
        let ctx = self.0.read().unwrap();

        let SExp::Atom = ctx.allocator.sexp(self.1) else {
            return Ok(None);
        };

        let Some(number) = ctx.allocator.small_number(self.1) else {
            return Ok(None);
        };

        if number != 0 && number != 1 {
            return Ok(None);
        }

        Ok(Some(number != 0))
    }

    pub fn to_atom(&self) -> Result<Option<Bytes>> {
        let ctx = self.0.read().unwrap();

        let SExp::Atom = ctx.allocator.sexp(self.1) else {
            return Ok(None);
        };

        Ok(Some(ctx.allocator.atom(self.1).to_vec().into()))
    }

    pub fn to_list(&self) -> Result<Option<Vec<Program>>> {
        let ctx = self.0.read().unwrap();

        let Some(value) = Vec::<NodePtr>::from_clvm(&ctx.allocator, self.1).ok() else {
            return Ok(None);
        };

        Ok(Some(
            value
                .into_iter()
                .map(|ptr| Program(self.0.clone(), ptr))
                .collect(),
        ))
    }

    pub fn to_pair(&self) -> Result<Option<Pair>> {
        let ctx = self.0.read().unwrap();

        let SExp::Pair(first, rest) = ctx.allocator.sexp(self.1) else {
            return Ok(None);
        };

        Ok(Some(Pair {
            first: Program(self.0.clone(), first),
            rest: Program(self.0.clone(), rest),
        }))
    }

    pub fn puzzle(&self) -> Result<Puzzle> {
        let ctx = self.0.read().unwrap();
        let value = chia_sdk_driver::Puzzle::parse(&ctx.allocator, self.1);

        Ok(match value {
            chia_sdk_driver::Puzzle::Curried(curried) => Puzzle {
                puzzle_hash: curried.curried_puzzle_hash.into(),
                program: Program(self.0.clone(), curried.curried_ptr),
                mod_hash: curried.mod_hash.into(),
                args: Some(Program(self.0.clone(), curried.args)),
            },
            chia_sdk_driver::Puzzle::Raw(raw) => Puzzle {
                puzzle_hash: raw.puzzle_hash.into(),
                program: Program(self.0.clone(), raw.ptr),
                mod_hash: raw.puzzle_hash.into(),
                args: None,
            },
        })
    }

    pub fn parse_nft_metadata(&self) -> Result<Option<NftMetadata>> {
        let ctx = self.0.read().unwrap();
        let value = nft::NftMetadata::from_clvm(&ctx.allocator, self.1);
        Ok(value.ok().map(Into::into))
    }
}
