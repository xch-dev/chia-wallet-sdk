use std::fmt::Debug;

use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_sdk_types::puzzles::{
    CatNftMetadata, VerificationLayer1stCurryArgs, VerificationLayer2ndCurryArgs,
    VerificationLayerSolution, VERIFICATION_LAYER_PUZZLE_HASH,
};
use clvm_traits::{clvm_list, FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, SpendContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationLayer {
    pub revocation_singleton_launcher_id: Bytes32,
    pub verified_data: VerifiedData,
}

impl VerificationLayer {
    pub fn new(revocation_singleton_launcher_id: Bytes32, verified_data: VerifiedData) -> Self {
        Self {
            revocation_singleton_launcher_id,
            verified_data,
        }
    }
}

impl Layer for VerificationLayer {
    type Solution = VerificationLayerSolution;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle_2nd_curry) = puzzle.as_curried() else {
            return Ok(None);
        };

        let puzzle_2nd_curry =
            CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, puzzle_2nd_curry.curried_ptr)?;
        let puzzle_1st_curry = Puzzle::parse(allocator, puzzle_2nd_curry.program);
        let Some(puzzle_1st_curry) = puzzle_1st_curry.as_curried() else {
            return Ok(None);
        };

        if puzzle_1st_curry.mod_hash != VERIFICATION_LAYER_PUZZLE_HASH {
            return Ok(None);
        }

        let args_2nd_curry = VerificationLayer2ndCurryArgs::<VerifiedData>::from_clvm(
            allocator,
            puzzle_2nd_curry.args,
        )?;
        let args_1st_curry =
            VerificationLayer1stCurryArgs::from_clvm(allocator, puzzle_1st_curry.args)?;

        if args_1st_curry
            .revocation_singleton_struct
            .launcher_puzzle_hash
            != SINGLETON_LAUNCHER_HASH.into()
            || args_1st_curry.revocation_singleton_struct.mod_hash
                != SINGLETON_TOP_LAYER_V1_1_HASH.into()
        {
            return Err(DriverError::NonStandardLayer);
        }

        if args_2nd_curry.self_hash
            != VerificationLayer1stCurryArgs::curry_tree_hash(
                args_1st_curry.revocation_singleton_struct.launcher_id,
            )
            .into()
        {
            return Err(DriverError::NonStandardLayer);
        }

        Ok(Some(Self {
            revocation_singleton_launcher_id: args_1st_curry
                .revocation_singleton_struct
                .launcher_id,
            verified_data: args_2nd_curry.verified_data,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        VerificationLayerSolution::from_clvm(allocator, solution).map_err(DriverError::FromClvm)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let puzzle_1st_curry = ctx.curry(VerificationLayer1stCurryArgs {
            revocation_singleton_struct: SingletonStruct::new(
                self.revocation_singleton_launcher_id,
            ),
        })?;
        let self_hash =
            VerificationLayer1stCurryArgs::curry_tree_hash(self.revocation_singleton_launcher_id)
                .into();

        CurriedProgram {
            program: puzzle_1st_curry,
            args: VerificationLayer2ndCurryArgs {
                self_hash,
                verified_data: self.verified_data.clone(),
            },
        }
        .to_clvm(ctx)
        .map_err(DriverError::ToClvm)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct VerifiedData {
    pub version: u32,
    pub asset_id: Bytes32,
    pub data_hash: Bytes32,
    #[clvm(rest)]
    pub comment: String,
}

impl VerifiedData {
    pub fn data_hash_from_cat_nft_metadata(metadata: &CatNftMetadata) -> Bytes32 {
        clvm_list!(
            metadata.ticker.clone(),
            metadata.name.clone(),
            metadata.description.clone(),
            metadata.image_hash,
            metadata.metadata_hash,
            metadata.license_hash,
        )
        .tree_hash()
        .into()
    }

    pub fn from_cat_nft_metadata(
        asset_id: Bytes32,
        metadata: &CatNftMetadata,
        comment: String,
    ) -> Self {
        Self {
            version: 1,
            asset_id,
            data_hash: Self::data_hash_from_cat_nft_metadata(metadata),
            comment,
        }
    }

    pub fn get_hint(&self) -> Bytes32 {
        self.data_hash
    }
}
