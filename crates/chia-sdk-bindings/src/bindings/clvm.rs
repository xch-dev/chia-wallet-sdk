use chia_protocol::{Bytes, Coin, CoinSpend, Program};
use chia_sdk_driver::{Cat, CatSpend, Spend, SpendContext, StandardLayer};
use clvm_traits::{clvm_quote, ClvmDecoder, ClvmEncoder, FromClvm, ToClvm};
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{
    reduction::Reduction,
    run_program,
    serde::{node_from_bytes, node_from_bytes_backrefs, node_to_bytes, node_to_bytes_backrefs},
    Allocator, ChiaDialect, NodePtr, SExp, MEMPOOL_MODE,
};
use num_bigint::BigInt;

use crate::{Error, Result};

use super::PublicKey;

#[derive(Default)]
pub struct Clvm(SpendContext);

impl Clvm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_coin_spend(&mut self, coin_spend: CoinSpend) {
        self.0.insert(coin_spend);
    }

    pub fn take_coin_spends(&mut self) -> Vec<CoinSpend> {
        self.0.take()
    }

    pub fn delegated_spend(&mut self, conditions: Vec<NodePtr>) -> Result<Spend> {
        let delegated_puzzle = self.0.alloc(&clvm_quote!(conditions))?;
        Ok(Spend {
            puzzle: delegated_puzzle,
            solution: NodePtr::NIL,
        })
    }

    pub fn serialize(&self, value: NodePtr) -> Result<Program> {
        Ok(node_to_bytes(&self.0.allocator, value)?.into())
    }

    pub fn serialize_with_backrefs(&self, value: NodePtr) -> Result<Program> {
        Ok(node_to_bytes_backrefs(&self.0.allocator, value)?.into())
    }

    pub fn deserialize(&mut self, value: Program) -> Result<NodePtr> {
        Ok(node_from_bytes(&mut self.0.allocator, &value)?)
    }

    pub fn deserialize_with_backrefs(&mut self, value: Program) -> Result<NodePtr> {
        Ok(node_from_bytes_backrefs(&mut self.0.allocator, &value)?)
    }

    pub fn tree_hash(&self, value: NodePtr) -> Result<Bytes> {
        Ok(tree_hash(&self.0.allocator, value).to_vec().into())
    }

    pub fn length(&self, value: NodePtr) -> Result<usize> {
        let SExp::Atom = self.0.allocator.sexp(value) else {
            return Err(Error::AtomExpected);
        };

        Ok(self.0.allocator.atom_len(value))
    }

    pub fn first(&self, value: NodePtr) -> Result<NodePtr> {
        let SExp::Pair(first, _) = self.0.allocator.sexp(value) else {
            return Err(Error::PairExpected);
        };

        Ok(first)
    }

    pub fn rest(&self, value: NodePtr) -> Result<NodePtr> {
        let SExp::Pair(_, rest) = self.0.allocator.sexp(value) else {
            return Err(Error::PairExpected);
        };

        Ok(rest)
    }

    pub fn as_f64(&self, value: NodePtr) -> Result<Option<f64>> {
        let SExp::Atom = self.0.allocator.sexp(value) else {
            return Ok(None);
        };

        let number = self.0.allocator.number(value);

        if number > BigInt::from(9_007_199_254_740_991i64) {
            return Err(Error::TooLarge);
        }

        if number < BigInt::from(-9_007_199_254_740_991i64) {
            return Err(Error::TooSmall);
        }

        let number: u64 = number.try_into().unwrap();

        #[allow(clippy::cast_precision_loss)]
        Ok(Some(number as f64))
    }

    pub fn as_bigint(&self, value: NodePtr) -> Result<Option<BigInt>> {
        let SExp::Atom = self.0.allocator.sexp(value) else {
            return Ok(None);
        };

        Ok(Some(self.0.allocator.number(value)))
    }

    pub fn as_string(&self, value: NodePtr) -> Result<Option<String>> {
        let SExp::Atom = self.0.allocator.sexp(value) else {
            return Ok(None);
        };

        let bytes = self.0.allocator.atom(value);

        Ok(Some(String::from_utf8(bytes.to_vec())?))
    }

    pub fn as_bool(&self, value: NodePtr) -> Result<Option<bool>> {
        let SExp::Atom = self.0.allocator.sexp(value) else {
            return Ok(None);
        };

        let Some(number) = self.0.allocator.small_number(value) else {
            return Ok(None);
        };

        if number != 0 && number != 1 {
            return Ok(None);
        }

        Ok(Some(number != 0))
    }

    pub fn as_atom(&self, value: NodePtr) -> Result<Option<Bytes>> {
        let SExp::Atom = self.0.allocator.sexp(value) else {
            return Ok(None);
        };

        Ok(Some(self.0.allocator.atom(value).to_vec().into()))
    }

    pub fn as_list(&self, value: NodePtr) -> Result<Option<Vec<NodePtr>>> {
        let Some(value) = Vec::<NodePtr>::from_clvm(&self.0.allocator, value).ok() else {
            return Ok(None);
        };

        Ok(Some(value))
    }

    pub fn as_pair(&self, value: NodePtr) -> Result<Option<(NodePtr, NodePtr)>> {
        let SExp::Pair(first, rest) = self.0.allocator.sexp(value) else {
            return Ok(None);
        };

        Ok(Some((first, rest)))
    }

    pub fn new_f64(&mut self, value: f64) -> Result<NodePtr> {
        if value.is_infinite() {
            return Err(Error::Infinite);
        }

        if value.is_nan() {
            return Err(Error::NaN);
        }

        if value.fract() != 0.0 {
            return Err(Error::Fractional);
        }

        if value > 9_007_199_254_740_991.0 {
            return Err(Error::TooLarge);
        }

        if value < -9_007_199_254_740_991.0 {
            return Err(Error::TooSmall);
        }

        #[allow(clippy::cast_possible_truncation)]
        let value = value as i64;

        if (0..=67_108_863).contains(&value) {
            Ok(self
                .0
                .allocator
                .new_small_number(value.try_into().unwrap())?)
        } else {
            Ok(self.0.allocator.new_number(value.into())?)
        }
    }

    pub fn new_bigint(&mut self, value: BigInt) -> Result<NodePtr> {
        Ok(self.0.allocator.new_number(value)?)
    }

    pub fn new_string(&mut self, value: String) -> Result<NodePtr> {
        Ok(self.0.allocator.new_atom(value.as_bytes())?)
    }

    pub fn new_bool(&mut self, value: bool) -> Result<NodePtr> {
        #[allow(clippy::cast_lossless)]
        Ok(self.0.allocator.new_small_number(value as u32)?)
    }

    pub fn new_atom(&mut self, value: Bytes) -> Result<NodePtr> {
        Ok(self.0.allocator.new_atom(&value)?)
    }

    pub fn new_list(&mut self, value: Vec<NodePtr>) -> Result<NodePtr> {
        let mut result = NodePtr::NIL;

        for item in value.into_iter().rev() {
            result = self.0.allocator.new_pair(item, result)?;
        }

        Ok(result)
    }

    pub fn new_pair(&mut self, first: NodePtr, second: NodePtr) -> Result<NodePtr> {
        Ok(self.0.allocator.new_pair(first, second)?)
    }

    pub fn encode(&mut self, value: impl ToClvm<Allocator>) -> Result<NodePtr> {
        Ok(value.to_clvm(&mut self.0.allocator)?)
    }

    pub fn decode<T: FromClvm<Allocator>>(&self, value: NodePtr) -> Result<T> {
        Ok(T::from_clvm(&self.0.allocator, value)?)
    }

    pub fn run(
        &mut self,
        puzzle: NodePtr,
        solution: NodePtr,
        max_cost: u64,
        mempool_mode: bool,
    ) -> Result<Reduction> {
        let mut flags = 0;

        if mempool_mode {
            flags |= MEMPOOL_MODE;
        }

        Ok(run_program(
            &mut self.0.allocator,
            &ChiaDialect::new(flags),
            puzzle,
            solution,
            max_cost,
        )?)
    }

    pub fn curry(&mut self, program: NodePtr, args: Vec<NodePtr>) -> Result<NodePtr> {
        let mut args_ptr = self.0.allocator.one();

        for arg in args.into_iter().rev() {
            args_ptr = self.0.allocator.encode_curried_arg(arg, args_ptr)?;
        }

        Ok(self.0.alloc(&CurriedProgram {
            program,
            args: args_ptr,
        })?)
    }

    pub fn uncurry(&self, value: NodePtr) -> Result<Option<(NodePtr, Vec<NodePtr>)>> {
        let Ok(value) = CurriedProgram::<NodePtr, NodePtr>::from_clvm(&self.0.allocator, value)
        else {
            return Ok(None);
        };

        let mut args = Vec::new();
        let mut args_ptr = value.args;

        while let Ok((first, rest)) = self.0.allocator.decode_curried_arg(&args_ptr) {
            args.push(first);
            args_ptr = rest;
        }

        if self.0.allocator.small_number(args_ptr) != Some(1) {
            return Ok(None);
        }

        Ok(Some((value.program, args)))
    }

    pub fn standard_spend(&mut self, synthetic_key: PublicKey, spend: Spend) -> Result<Spend> {
        Ok(StandardLayer::new(synthetic_key.0).delegated_inner_spend(&mut self.0, spend)?)
    }

    pub fn spend_standard_coin(
        &mut self,
        coin: Coin,
        synthetic_key: PublicKey,
        spend: Spend,
    ) -> Result<()> {
        let spend = self.standard_spend(synthetic_key, spend)?;
        let puzzle_reveal = self.serialize(spend.puzzle)?;
        let solution = self.serialize(spend.solution)?;
        self.insert_coin_spend(CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    pub fn spend_cat_coins(&mut self, cat_spends: Vec<CatSpend>) -> Result<()> {
        Cat::spend_all(&mut self.0, &cat_spends)?;
        Ok(())
    }
}
