use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{CatLayer, CurriedPuzzle, HashedPtr, Layer, RawPuzzle, SpendContext};

use crate::{
    Cat, Did, DidInfo, Nft, NftInfo, ParsedCat, ParsedDid, ParsedNft, Program, StreamedCat,
    StreamingPuzzleInfo,
};

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

    pub fn parse_cat(&self) -> Result<Option<ParsedCat>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());
        let ctx = self.program.0.lock().unwrap();

        let Some(cat) = CatLayer::<chia_sdk_driver::Puzzle>::parse_puzzle(&ctx, puzzle)? else {
            return Ok(None);
        };

        Ok(Some(ParsedCat {
            asset_id: cat.asset_id,
            p2_puzzle: Self::new(&self.program.0, cat.inner_puzzle),
        }))
    }

    pub fn parse_child_cats(
        &self,
        parent_coin: Coin,
        parent_solution: Program,
    ) -> Result<Option<Vec<Cat>>> {
        let mut ctx = self.program.0.lock().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let Some(cats) = chia_sdk_driver::Cat::parse_children(
            &mut ctx,
            parent_coin,
            parent_puzzle,
            parent_solution.1,
        )?
        else {
            return Ok(None);
        };

        Ok(Some(cats.into_iter().map(Cat::from).collect()))
    }

    pub fn parse_nft(&self) -> Result<Option<ParsedNft>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        let Some((nft_info, p2_puzzle)) =
            chia_sdk_driver::NftInfo::<HashedPtr>::parse(&ctx, puzzle)?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedNft {
            info: NftInfo::from(
                nft_info.with_metadata(Program(self.program.0.clone(), nft_info.metadata.ptr())),
            ),
            p2_puzzle: Self::new(&self.program.0, p2_puzzle),
        }))
    }

    pub fn parse_child_nft(
        &self,
        parent_coin: Coin,
        parent_solution: Program,
    ) -> Result<Option<Nft>> {
        let mut ctx = self.program.0.lock().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let Some(nft) = chia_sdk_driver::Nft::<HashedPtr>::parse_child(
            &mut ctx,
            parent_coin,
            parent_puzzle,
            parent_solution.1,
        )?
        else {
            return Ok(None);
        };

        Ok(Some(
            nft.with_metadata(Program(self.program.0.clone(), nft.info.metadata.ptr()))
                .into(),
        ))
    }

    pub fn parse_did(&self) -> Result<Option<ParsedDid>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        let Some((did_info, p2_puzzle)) =
            chia_sdk_driver::DidInfo::<HashedPtr>::parse(&ctx, puzzle)?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedDid {
            info: DidInfo::from(
                did_info.with_metadata(Program(self.program.0.clone(), did_info.metadata.ptr())),
            ),
            p2_puzzle: Self::new(&self.program.0, p2_puzzle),
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

        let Some(did) = chia_sdk_driver::Did::<HashedPtr>::parse_child(
            &mut ctx,
            parent_coin,
            parent_puzzle,
            parent_solution.1,
            coin,
        )?
        else {
            return Ok(None);
        };

        Ok(Some(
            did.with_metadata(Program(self.program.0.clone(), did.info.metadata.ptr()))
                .into(),
        ))
    }

    pub fn parse_inner_streaming_puzzle(&self) -> Result<Option<StreamingPuzzleInfo>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.lock().unwrap();

        Ok(chia_sdk_driver::StreamingPuzzleInfo::parse(&ctx, puzzle)?.map(Into::into))
    }

    pub fn parse_child_streamed_cat(
        &self,
        parent_coin: Coin,
        parent_solution: Program,
    ) -> Result<StreamedCatParsingResult> {
        let mut ctx = self.program.0.lock().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let (streamed_cat, clawback, last_payment_amount) =
            chia_sdk_driver::StreamedCat::from_parent_spend(
                &mut ctx,
                parent_coin,
                parent_puzzle,
                parent_solution.1,
            )?;

        Ok(StreamedCatParsingResult {
            streamed_cat: streamed_cat.map(std::convert::Into::into),
            last_spend_was_clawback: clawback,
            last_payment_amount_if_clawback: last_payment_amount,
        })
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

#[derive(Clone)]
pub struct StreamedCatParsingResult {
    pub streamed_cat: Option<StreamedCat>,
    pub last_spend_was_clawback: bool,
    pub last_payment_amount_if_clawback: u64,
}
