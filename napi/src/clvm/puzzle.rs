use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::{IntoJs, IntoJsWithClvm, IntoRust, NftInfo};

use super::Program;

#[napi]
pub struct Puzzle {
    pub(crate) puzzle_hash: Uint8Array,
    pub(crate) program: Reference<Program>,
    pub(crate) mod_hash: Uint8Array,
    pub(crate) args: Option<Reference<Program>>,
}

#[napi]
impl Puzzle {
    #[napi(getter)]
    pub fn puzzle_hash(&self) -> Uint8Array {
        self.puzzle_hash.to_vec().into()
    }

    #[napi(getter)]
    pub fn program(&self, env: Env) -> Result<Reference<Program>> {
        self.program.clone(env)
    }

    #[napi(getter)]
    pub fn mod_hash(&self) -> Uint8Array {
        self.mod_hash.to_vec().into()
    }

    #[napi(getter)]
    pub fn args(&self, env: Env) -> Result<Option<Reference<Program>>> {
        self.args
            .as_ref()
            .map(|program| program.clone(env))
            .transpose()
    }

    #[napi]
    pub fn parse_nft(&self, env: Env) -> Result<Option<NftInfo>> {
        let puzzle = self.to_sdk_puzzle(env)?;

        let Some((nft_info, p2_puzzle)) = self.program.clvm.0.parse_nft(puzzle)? else {
            return Ok(None);
        };

        Ok(Some(NftInfo {
            launcher_id: nft_info.launcher_id.js()?,
            metadata: nft_info.metadata.js_with_clvm(env, &self.program.clvm)?,
            metadata_updater_puzzle_hash: nft_info.metadata_updater_puzzle_hash.js()?,
            current_owner: nft_info.current_owner.js()?,
            royalty_puzzle_hash: nft_info.royalty_puzzle_hash.js()?,
            royalty_ten_thousandths: nft_info.royalty_ten_thousandths,
            p2_puzzle_hash: nft_info.p2_puzzle_hash.js()?,
            p2_puzzle: Some(
                Program::new(self.program.clvm.clone(env)?, p2_puzzle.ptr).into_reference(env)?,
            ),
        }))
    }
}

impl Puzzle {
    fn to_sdk_puzzle(&self, env: Env) -> Result<chia_sdk_bindings::Puzzle> {
        Ok(Self {
            puzzle_hash: self.puzzle_hash.to_vec().into(),
            program: self.program.clone(env)?,
            mod_hash: self.mod_hash.to_vec().into(),
            args: self
                .args
                .as_ref()
                .map(|program| program.clone(env))
                .transpose()?,
        }
        .rust()?)
    }
}
