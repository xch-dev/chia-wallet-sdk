use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{CurriedPuzzle, HashedPtr, RawPuzzle};

use crate::{Nft, NftInfo, ParsedNft, Program};

#[derive(Clone)]
pub struct Puzzle {
    pub puzzle_hash: Bytes32,
    pub program: Program,
    pub mod_hash: Bytes32,
    pub args: Option<Program>,
}

impl Puzzle {
    pub fn parse_nft(&self) -> Result<Option<ParsedNft>> {
        let puzzle = chia_sdk_driver::Puzzle::from(self.clone());

        let ctx = self.program.0.read().unwrap();

        let Some((nft_info, p2_puzzle)) =
            chia_sdk_driver::NftInfo::<HashedPtr>::parse(&ctx, puzzle)?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedNft {
            info: NftInfo {
                launcher_id: nft_info.launcher_id,
                metadata: Program(self.program.0.clone(), nft_info.metadata.ptr()),
                metadata_updater_puzzle_hash: nft_info.metadata_updater_puzzle_hash,
                current_owner: nft_info.current_owner,
                royalty_puzzle_hash: nft_info.royalty_puzzle_hash,
                royalty_ten_thousandths: nft_info.royalty_ten_thousandths,
                p2_puzzle_hash: nft_info.p2_puzzle_hash,
            },
            p2_puzzle: Program(self.program.0.clone(), p2_puzzle.ptr()),
        }))
    }

    pub fn parse_child_nft(
        &self,
        parent_coin: Coin,
        parent_puzzle: Program,
        parent_solution: Program,
    ) -> Result<Option<Nft>> {
        let mut ctx = self.program.0.write().unwrap();

        let parent_puzzle = chia_sdk_driver::Puzzle::parse(&ctx, parent_puzzle.1);

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
