mod cat;
mod clawback;
mod clawback_v2;
mod did;
mod nft;
mod option;
mod streamed_asset;

pub use cat::*;
pub use clawback::*;
pub use clawback_v2::*;
pub use did::*;
pub use nft::*;
pub use option::*;
pub use streamed_asset::*;

use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{cat::CatArgs, standard::StandardArgs};
use chia_sdk_driver::{
    Cat, CatInfo, Clawback, CurriedPuzzle, OptionContract as SdkOptionContract, OptionInfo,
    RawPuzzle, SpendContext, StreamingPuzzleInfo,
};

use crate::{AsProgram, Program};

#[derive(Clone)]
pub struct Puzzle {
    pub puzzle_hash: Bytes32,
    pub program: Program,
    pub mod_hash: Bytes32,
    pub args: Option<Program>,
}

impl Puzzle {
    pub(crate) fn new(ctx: &Arc<Mutex<SpendContext>>, value: chia_sdk_driver::Puzzle) -> Self {
        match value {
            chia_sdk_driver::Puzzle::Curried(curried) => Puzzle {
                puzzle_hash: curried.curried_puzzle_hash.into(),
                program: Program(ctx.clone(), curried.curried_ptr),
                mod_hash: curried.mod_hash.into(),
                args: Some(Program(ctx.clone(), curried.args)),
            },
            chia_sdk_driver::Puzzle::Raw(raw) => Puzzle {
                puzzle_hash: raw.puzzle_hash.into(),
                program: Program(ctx.clone(), raw.ptr),
                mod_hash: raw.puzzle_hash.into(),
                args: None,
            },
        }
    }

    pub fn parse_cat_info(&self) -> Result<Option<ParsedCatInfo>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());
        let ctx = self.program.0.lock().unwrap();

        let Some((info, p2_puzzle)) = CatInfo::parse(&ctx, puzzle)? else {
            return Ok(None);
        };

        Ok(Some(ParsedCatInfo {
            info,
            p2_puzzle: p2_puzzle.map(|puzzle| Self::new(&self.program.0, puzzle)),
        }))
    }

    pub fn parse_cat(&self, coin: Coin, solution: Program) -> Result<Option<ParsedCat>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());
        let ctx = self.program.0.lock().unwrap();

        let Some((cat, p2_puzzle, p2_solution)) = Cat::parse(&ctx, coin, puzzle, solution.1)?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedCat {
            cat,
            p2_puzzle: Self::new(&self.program.0, p2_puzzle),
            p2_solution: Program(self.program.0.clone(), p2_solution),
        }))
    }

    pub fn parse_child_cats(
        &self,
        parent_coin: Coin,
        parent_solution: Program,
    ) -> Result<Option<Vec<Cat>>> {
        let mut ctx = self.program.0.lock().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        Ok(Cat::parse_children(
            &mut ctx,
            parent_coin,
            parent_puzzle,
            parent_solution.1,
        )?)
    }

    pub fn parse_nft_info(&self) -> Result<Option<ParsedNftInfo>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        let Some((nft_info, p2_puzzle)) = chia_sdk_driver::NftInfo::parse(&ctx, puzzle)? else {
            return Ok(None);
        };

        Ok(Some(ParsedNftInfo {
            info: nft_info.as_program(&self.program.0),
            p2_puzzle: Self::new(&self.program.0, p2_puzzle),
        }))
    }

    pub fn parse_nft(&self, coin: Coin, solution: Program) -> Result<Option<ParsedNft>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        let Some((nft_info, p2_puzzle, p2_solution)) =
            chia_sdk_driver::Nft::parse(&ctx, coin, puzzle, solution.1)?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedNft {
            nft: nft_info.as_program(&self.program.0),
            p2_puzzle: Self::new(&self.program.0, p2_puzzle),
            p2_solution: Program(self.program.0.clone(), p2_solution),
        }))
    }

    pub fn parse_child_nft(
        &self,
        parent_coin: Coin,
        parent_solution: Program,
    ) -> Result<Option<Nft>> {
        let mut ctx = self.program.0.lock().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let Some(nft) = chia_sdk_driver::Nft::parse_child(
            &mut ctx,
            parent_coin,
            parent_puzzle,
            parent_solution.1,
        )?
        else {
            return Ok(None);
        };

        Ok(Some(nft.as_program(&self.program.0)))
    }

    pub fn parse_did_info(&self) -> Result<Option<ParsedDidInfo>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        let Some((did_info, p2_puzzle)) = chia_sdk_driver::DidInfo::parse(&ctx, puzzle)? else {
            return Ok(None);
        };

        Ok(Some(ParsedDidInfo {
            info: did_info.as_program(&self.program.0),
            p2_puzzle: Self::new(&self.program.0, p2_puzzle),
        }))
    }

    pub fn parse_did(&self, coin: Coin, solution: Program) -> Result<Option<ParsedDid>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        let Some((did_info, p2_spend)) =
            chia_sdk_driver::Did::parse(&ctx, coin, puzzle, solution.1)?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedDid {
            did: did_info.as_program(&self.program.0),
            p2_spend: p2_spend.map(|(p2_puzzle, p2_solution)| ParsedDidSpend {
                puzzle: Self::new(&self.program.0, p2_puzzle),
                solution: Program(self.program.0.clone(), p2_solution),
            }),
        }))
    }

    pub fn parse_child_did(
        &self,
        parent_coin: Coin,
        parent_solution: Program,
        coin: Coin,
    ) -> Result<Option<Did>> {
        let mut ctx = self.program.0.lock().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let Some(did) = chia_sdk_driver::Did::parse_child(
            &mut ctx,
            parent_coin,
            parent_puzzle,
            parent_solution.1,
            coin,
        )?
        else {
            return Ok(None);
        };

        Ok(Some(did.as_program(&self.program.0)))
    }

    pub fn parse_option_info(&self) -> Result<Option<ParsedOptionInfo>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        let Some((info, p2_puzzle)) = OptionInfo::parse(&ctx, puzzle)? else {
            return Ok(None);
        };

        Ok(Some(ParsedOptionInfo {
            info,
            p2_puzzle: Self::new(&self.program.0, p2_puzzle),
        }))
    }

    pub fn parse_option(&self, coin: Coin, solution: Program) -> Result<Option<ParsedOption>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        let Some((option, p2_puzzle, p2_solution)) =
            SdkOptionContract::parse(&ctx, coin, puzzle, solution.1)?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedOption {
            option: option.into(),
            p2_puzzle: Self::new(&self.program.0, p2_puzzle),
            p2_solution: Program(self.program.0.clone(), p2_solution),
        }))
    }

    pub fn parse_child_option(
        &self,
        parent_coin: Coin,
        parent_solution: Program,
    ) -> Result<Option<OptionContract>> {
        let mut ctx = self.program.0.lock().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let Some(option) = SdkOptionContract::parse_child(
            &mut ctx,
            parent_coin,
            parent_puzzle,
            parent_solution.1,
        )?
        else {
            return Ok(None);
        };

        Ok(Some(option.into()))
    }

    pub fn parse_inner_streaming_puzzle(&self) -> Result<Option<StreamingPuzzleInfo>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        Ok(chia_sdk_driver::StreamingPuzzleInfo::parse(&ctx, puzzle)?)
    }

    pub fn parse_child_clawbacks(&self, parent_solution: Program) -> Result<Option<Vec<Clawback>>> {
        let mut ctx = self.program.0.lock().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        Ok(Clawback::parse_children(
            &mut ctx,
            parent_puzzle,
            parent_solution.1,
        )?)
    }
}

impl From<Puzzle> for chia_sdk_driver::Puzzle {
    fn from(value: Puzzle) -> Self {
        if let Some(args) = value.args {
            chia_sdk_driver::Puzzle::Curried(CurriedPuzzle {
                curried_puzzle_hash: value.puzzle_hash.into(),
                curried_ptr: value.program.1,
                mod_hash: value.mod_hash.into(),
                args: args.1,
            })
        } else {
            chia_sdk_driver::Puzzle::Raw(RawPuzzle {
                puzzle_hash: value.puzzle_hash.into(),
                ptr: value.program.1,
            })
        }
    }
}

pub fn standard_puzzle_hash(synthetic_key: PublicKey) -> Result<Bytes32> {
    Ok(StandardArgs::curry_tree_hash(synthetic_key).into())
}

pub fn cat_puzzle_hash(asset_id: Bytes32, inner_puzzle_hash: Bytes32) -> Result<Bytes32> {
    Ok(CatArgs::curry_tree_hash(asset_id, inner_puzzle_hash.into()).into())
}
