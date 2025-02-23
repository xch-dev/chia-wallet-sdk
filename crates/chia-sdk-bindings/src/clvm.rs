use std::sync::{Arc, RwLock};

use bindy::Result;
use chia_protocol::Program as SerializedProgram;
use chia_sdk_driver::SpendContext;
use clvm_traits::clvm_quote;
use clvmr::{
    serde::{node_from_bytes, node_from_bytes_backrefs},
    NodePtr,
};

use crate::{CoinSpend, Program, Spend};

#[derive(Default, Clone)]
pub struct Clvm(Arc<RwLock<SpendContext>>);

impl Clvm {
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn add_coin_spend(&self, coin_spend: CoinSpend) -> Result<()> {
        self.0.write().unwrap().insert(coin_spend.into());
        Ok(())
    }

    pub fn coin_spends(&self) -> Result<Vec<CoinSpend>> {
        Ok(self
            .0
            .write()
            .unwrap()
            .take()
            .into_iter()
            .map(CoinSpend::from)
            .collect())
    }

    pub fn deserialize(&self, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.write().unwrap();
        let ptr = node_from_bytes(&mut ctx.allocator, &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn deserialize_with_backrefs(&self, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.write().unwrap();
        let ptr = node_from_bytes_backrefs(&mut ctx.allocator, &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn delegated_spend(&self, conditions: Vec<Program>) -> Result<Spend> {
        let delegated_puzzle = self.0.write().unwrap().alloc(&clvm_quote!(conditions
            .into_iter()
            .map(|p| p.1)
            .collect::<Vec<_>>()))?;
        Ok(Spend {
            puzzle: Program(self.0.clone(), delegated_puzzle),
            solution: Program(self.0.clone(), NodePtr::NIL),
        })
    }

    // pub fn new_f64(&mut self, value: f64) -> Result<NodePtr> {
    //     if value.is_infinite() {
    //         return Err(Error::Infinite);
    //     }

    //     if value.is_nan() {
    //         return Err(Error::NaN);
    //     }

    //     if value.fract() != 0.0 {
    //         return Err(Error::Fractional);
    //     }

    //     if value > 9_007_199_254_740_991.0 {
    //         return Err(Error::TooLarge);
    //     }

    //     if value < -9_007_199_254_740_991.0 {
    //         return Err(Error::TooSmall);
    //     }

    //     #[allow(clippy::cast_possible_truncation)]
    //     let value = value as i64;

    //     if (0..=67_108_863).contains(&value) {
    //         Ok(self
    //             .0
    //             .allocator
    //             .new_small_number(value.try_into().unwrap())?)
    //     } else {
    //         Ok(self.0.allocator.new_number(value.into())?)
    //     }
    // }

    // pub fn new_bigint(&mut self, value: BigInt) -> Result<NodePtr> {
    //     Ok(self.0.allocator.new_number(value)?)
    // }

    // pub fn new_string(&mut self, value: String) -> Result<NodePtr> {
    //     Ok(self.0.allocator.new_atom(value.as_bytes())?)
    // }

    // pub fn new_bool(&mut self, value: bool) -> Result<NodePtr> {
    //     #[allow(clippy::cast_lossless)]
    //     Ok(self.0.allocator.new_small_number(value as u32)?)
    // }

    // pub fn new_atom(&mut self, value: Bytes) -> Result<NodePtr> {
    //     Ok(self.0.allocator.new_atom(&value)?)
    // }

    // pub fn new_list(&mut self, value: Vec<NodePtr>) -> Result<NodePtr> {
    //     let mut result = NodePtr::NIL;

    //     for item in value.into_iter().rev() {
    //         result = self.0.allocator.new_pair(item, result)?;
    //     }

    //     Ok(result)
    // }

    // pub fn new_pair(&mut self, first: NodePtr, second: NodePtr) -> Result<NodePtr> {
    //     Ok(self.0.allocator.new_pair(first, second)?)
    // }

    // pub fn standard_spend(&mut self, synthetic_key: PublicKey, spend: Spend) -> Result<Spend> {
    //     Ok(StandardLayer::new(synthetic_key.0).delegated_inner_spend(&mut self.0, spend)?)
    // }

    // pub fn spend_standard_coin(
    //     &mut self,
    //     coin: Coin,
    //     synthetic_key: PublicKey,
    //     spend: Spend,
    // ) -> Result<()> {
    //     let spend = self.standard_spend(synthetic_key, spend)?;
    //     let puzzle_reveal = self.serialize(spend.puzzle)?;
    //     let solution = self.serialize(spend.solution)?;
    //     self.insert_coin_spend(CoinSpend::new(coin, puzzle_reveal, solution));
    //     Ok(())
    // }

    // pub fn spend_cat_coins(&mut self, cat_spends: Vec<CatSpend>) -> Result<()> {
    //     Cat::spend_all(&mut self.0, &cat_spends)?;
    //     Ok(())
    // }

    // pub fn parse_puzzle(&self, value: NodePtr) -> Result<Puzzle> {
    //     let puzzle = SdkPuzzle::parse(&self.0.allocator, value);
    //     Ok(Puzzle::from(puzzle))
    // }

    // pub fn parse_nft(&self, puzzle: Puzzle) -> Result<Option<(NftInfo<NodePtr>, Puzzle)>> {
    //     let puzzle = SdkPuzzle::from(puzzle);

    //     let Some((nft_info, p2_puzzle)) = NftInfo::<NodePtr>::parse(&self.0.allocator, puzzle)?
    //     else {
    //         return Ok(None);
    //     };

    //     Ok(Some((nft_info, Puzzle::from(p2_puzzle))))
    // }

    // pub fn mint_nfts(
    //     &mut self,
    //     parent_coin_id: Bytes32,
    //     nft_mints: Vec<NftMint<HashedPtr>>,
    // ) -> Result<(Vec<Nft<HashedPtr>>, Vec<NodePtr>)> {
    //     let mut nfts = Vec::new();
    //     let mut parent_conditions = Vec::new();

    //     for (i, nft_mint) in nft_mints.into_iter().enumerate() {
    //         let (conditions, nft) =
    //             Launcher::new(parent_coin_id, i as u64 * 2 + 1).mint_nft(&mut self.0, nft_mint)?;

    //         nfts.push(nft);

    //         for condition in conditions {
    //             let condition = condition.to_clvm(&mut self.0.allocator)?;
    //             parent_conditions.push(condition);
    //         }
    //     }

    //     Ok((nfts, parent_conditions))
    // }
}

// #[derive(Debug, Clone, Copy)]
// pub struct Puzzle {
//     pub puzzle_hash: Bytes32,
//     pub ptr: NodePtr,
//     pub mod_hash: Bytes32,
//     pub args: Option<NodePtr>,
// }

// impl From<SdkPuzzle> for Puzzle {
//     fn from(value: SdkPuzzle) -> Self {
//         match value {
//             SdkPuzzle::Curried(curried) => Self {
//                 puzzle_hash: curried.curried_puzzle_hash.into(),
//                 ptr: curried.curried_ptr,
//                 mod_hash: curried.mod_hash.into(),
//                 args: Some(curried.args),
//             },
//             SdkPuzzle::Raw(raw) => Self {
//                 puzzle_hash: raw.puzzle_hash.into(),
//                 ptr: raw.ptr,
//                 mod_hash: raw.puzzle_hash.into(),
//                 args: None,
//             },
//         }
//     }
// }

// impl From<Puzzle> for SdkPuzzle {
//     fn from(value: Puzzle) -> Self {
//         if let Some(args) = value.args {
//             SdkPuzzle::Curried(CurriedPuzzle {
//                 curried_puzzle_hash: value.puzzle_hash.into(),
//                 curried_ptr: value.ptr,
//                 mod_hash: value.mod_hash.into(),
//                 args,
//             })
//         } else {
//             SdkPuzzle::Raw(RawPuzzle {
//                 puzzle_hash: value.puzzle_hash.into(),
//                 ptr: value.ptr,
//             })
//         }
//     }
// }
