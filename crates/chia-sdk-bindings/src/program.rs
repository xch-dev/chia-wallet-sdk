use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_protocol::{Bytes, Program as SerializedProgram};
use chia_puzzle_types::nft::NftMetadata;
use chia_sdk_driver::SpendContext;
use clvm_tools_rs::classic::clvm_tools::stages::run;
use clvm_tools_rs::classic::clvm_tools::stages::stage_0::TRunProgram;
use clvm_tools_rs::classic::clvm_tools::{
    binutils::disassemble, stages::stage_2::operators::run_program_for_search_paths,
};
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm};
use clvm_utils::{tree_hash, TreeHash};
use clvmr::{
    run_program,
    serde::{node_to_bytes, node_to_bytes_backrefs},
    ChiaDialect, NodePtr, SExp, MEMPOOL_MODE,
};
use num_bigint::BigInt;

use crate::{CurriedProgram, Output, Pair, Puzzle};

#[derive(Clone)]
pub struct Program(pub(crate) Arc<Mutex<SpendContext>>, pub(crate) NodePtr);

impl Program {
    pub fn compile(&self) -> Result<Output> {
        let mut ctx = self.0.lock().unwrap();

        let invoke = run(&mut ctx);
        let input = ctx.new_pair(self.1, NodePtr::NIL)?;
        let run_program = run_program_for_search_paths("program.clsp", &[], false);
        let output = run_program.run_program(&mut ctx, invoke, input, None)?;

        Ok(Output {
            value: Program(self.0.clone(), output.1),
            cost: output.0,
        })
    }

    pub fn unparse(&self) -> Result<String> {
        let ctx = self.0.lock().unwrap();
        Ok(disassemble(&ctx, self.1, None))
    }

    pub fn serialize(&self) -> Result<SerializedProgram> {
        let ctx = self.0.lock().unwrap();
        Ok(node_to_bytes(&ctx, self.1)?.into())
    }

    pub fn serialize_with_backrefs(&self) -> Result<SerializedProgram> {
        let ctx = self.0.lock().unwrap();
        Ok(node_to_bytes_backrefs(&ctx, self.1)?.into())
    }

    pub fn run(&self, solution: Self, max_cost: u64, mempool_mode: bool) -> Result<Output> {
        let mut flags = 0;

        if mempool_mode {
            flags |= MEMPOOL_MODE;
        }

        let mut ctx = self.0.lock().unwrap();

        let reduction = run_program(
            &mut ctx,
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
        let mut ctx = self.0.lock().unwrap();

        let mut args_ptr = ctx.one();

        for arg in args.into_iter().rev() {
            args_ptr = ctx.encode_curried_arg(arg.1, args_ptr)?;
        }

        let ptr = ctx.alloc(&clvm_utils::CurriedProgram {
            program: self.1,
            args: args_ptr,
        })?;

        Ok(Program(self.0.clone(), ptr))
    }

    pub fn uncurry(&self) -> Result<Option<CurriedProgram>> {
        let ctx = self.0.lock().unwrap();

        let Ok(value) = clvm_utils::CurriedProgram::<NodePtr, NodePtr>::from_clvm(&ctx, self.1)
        else {
            return Ok(None);
        };

        let mut args = Vec::new();
        let mut args_ptr = value.args;

        while let Ok((first, rest)) = ctx.decode_curried_arg(&args_ptr) {
            args.push(first);
            args_ptr = rest;
        }

        if ctx.small_number(args_ptr) != Some(1) {
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
        let ctx = self.0.lock().unwrap();
        Ok(tree_hash(&ctx, self.1))
    }

    pub fn is_atom(&self) -> Result<bool> {
        let ctx = self.0.lock().unwrap();
        Ok(matches!(ctx.sexp(self.1), SExp::Atom))
    }

    pub fn is_pair(&self) -> Result<bool> {
        let ctx = self.0.lock().unwrap();
        Ok(matches!(ctx.sexp(self.1), SExp::Pair(..)))
    }

    pub fn is_null(&self) -> Result<bool> {
        let ctx = self.0.lock().unwrap();
        Ok(matches!(ctx.sexp(self.1), SExp::Atom) && ctx.atom_len(self.1) == 0)
    }

    pub fn length(&self) -> Result<u32> {
        let ctx = self.0.lock().unwrap();

        let SExp::Atom = ctx.sexp(self.1) else {
            return Err(Error::AtomExpected);
        };

        Ok(ctx.atom_len(self.1) as u32)
    }

    pub fn first(&self) -> Result<Program> {
        let ctx = self.0.lock().unwrap();

        let SExp::Pair(first, _) = ctx.sexp(self.1) else {
            return Err(Error::PairExpected);
        };

        Ok(Program(self.0.clone(), first))
    }

    pub fn rest(&self) -> Result<Program> {
        let ctx = self.0.lock().unwrap();

        let SExp::Pair(_, rest) = ctx.sexp(self.1) else {
            return Err(Error::PairExpected);
        };

        Ok(Program(self.0.clone(), rest))
    }

    // This is called by the individual napi and wasm crates
    pub fn to_small_int(&self) -> Result<Option<f64>> {
        let ctx = self.0.lock().unwrap();

        let SExp::Atom = ctx.sexp(self.1) else {
            return Ok(None);
        };

        let number = ctx.number(self.1);

        if number > BigInt::from(9_007_199_254_740_991i64) {
            return Err(Error::TooLarge);
        }

        if number < BigInt::from(-9_007_199_254_740_991i64) {
            return Err(Error::TooSmall);
        }

        let number: u64 = number.try_into().unwrap();

        Ok(Some(number as f64))
    }

    pub fn to_int(&self) -> Result<Option<BigInt>> {
        let ctx = self.0.lock().unwrap();

        let SExp::Atom = ctx.sexp(self.1) else {
            return Ok(None);
        };

        Ok(Some(ctx.number(self.1)))
    }

    pub fn to_string(&self) -> Result<Option<String>> {
        let ctx = self.0.lock().unwrap();

        let SExp::Atom = ctx.sexp(self.1) else {
            return Ok(None);
        };

        let bytes = ctx.atom(self.1);

        Ok(Some(String::from_utf8(bytes.to_vec())?))
    }

    pub fn to_bool(&self) -> Result<Option<bool>> {
        let ctx = self.0.lock().unwrap();

        let SExp::Atom = ctx.sexp(self.1) else {
            return Ok(None);
        };

        let Some(number) = ctx.small_number(self.1) else {
            return Ok(None);
        };

        if number != 0 && number != 1 {
            return Ok(None);
        }

        Ok(Some(number != 0))
    }

    pub fn to_atom(&self) -> Result<Option<Bytes>> {
        let ctx = self.0.lock().unwrap();

        let SExp::Atom = ctx.sexp(self.1) else {
            return Ok(None);
        };

        Ok(Some(ctx.atom(self.1).to_vec().into()))
    }

    pub fn to_list(&self) -> Result<Option<Vec<Program>>> {
        let ctx = self.0.lock().unwrap();

        let Some(value) = Vec::<NodePtr>::from_clvm(&ctx, self.1).ok() else {
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
        let ctx = self.0.lock().unwrap();

        let SExp::Pair(first, rest) = ctx.sexp(self.1) else {
            return Ok(None);
        };

        Ok(Some(Pair {
            first: Program(self.0.clone(), first),
            rest: Program(self.0.clone(), rest),
        }))
    }

    pub fn puzzle(&self) -> Result<Puzzle> {
        let ctx = self.0.lock().unwrap();
        let value = chia_sdk_driver::Puzzle::parse(&ctx, self.1);
        Ok(Puzzle::new(&self.0, value))
    }

    pub fn parse_nft_metadata(&self) -> Result<Option<NftMetadata>> {
        let ctx = self.0.lock().unwrap();
        let value = NftMetadata::from_clvm(&**ctx, self.1);
        Ok(value.ok())
    }
}
