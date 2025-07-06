use std::fmt::Debug;

use chia::{
    clvm_traits::{FromClvm, ToClvm},
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::singleton::SingletonStruct,
};
use chia_puzzles::{SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH};
use chia_wallet_sdk::driver::{DriverError, Layer, Puzzle, SpendContext};

use clvm_traits::clvm_list;
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::{CatNftMetadata, SpendContextExt};

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
        let puzzle_1st_curry = CurriedProgram {
            program: ctx.verification_puzzle()?,
            args: VerificationLayer1stCurryArgs {
                revocation_singleton_struct: SingletonStruct::new(
                    self.revocation_singleton_launcher_id,
                ),
            },
        }
        .to_clvm(ctx)?;
        let self_hash: Bytes32 =
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

pub const VERIFICATION_LAYER_PUZZLE: [u8; 576] = hex!("ff02ffff01ff02ffff03ffff09ff2fff8080ffff01ff04ffff04ff14ffff01ff808080ffff04ffff04ff08ffff04ffff0bff56ffff0bff0affff0bff0aff66ff0b80ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ffff0bffff0101ff0b8080ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ffff02ff1effff04ff02ffff04ff17ff8080808080ffff0bff0aff66ff46808080ff46808080ff46808080ffff01ff01808080ff808080ffff01ff04ffff04ff08ffff01ff80ff818f8080ffff04ffff04ff1cffff04ffff0112ffff04ff80ffff04ffff0bff56ffff0bff0affff0bff0aff66ff0980ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ffff02ff1effff04ff02ffff04ff05ff8080808080ffff0bff0affff0bff76ffff0bff0affff0bff0aff66ff2f80ffff0bff0aff66ff46808080ff46808080ff46808080ff8080808080ff80808080ff0180ffff04ffff01ffff33ff3e43ff02ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ffff02ff1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");

pub const VERIFICATION_LAYER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    72600e1408134c0def58ce09d1b9edce15ffcfd5f5a2ebcd421d4a47ec4518c2
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct VerificationLayer1stCurryArgs {
    pub revocation_singleton_struct: SingletonStruct,
}

impl VerificationLayer1stCurryArgs {
    pub fn curry_tree_hash(revocation_singleton_launcher_id: Bytes32) -> TreeHash {
        CurriedProgram {
            program: VERIFICATION_LAYER_PUZZLE_HASH,
            args: VerificationLayer1stCurryArgs {
                revocation_singleton_struct: SingletonStruct::new(revocation_singleton_launcher_id),
            },
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct VerificationLayer2ndCurryArgs<T> {
    pub self_hash: Bytes32,
    pub verified_data: T,
}

impl<T> VerificationLayer2ndCurryArgs<T>
where
    T: ToTreeHash,
{
    pub fn curry_tree_hash(
        revocation_singleton_launcher_id: Bytes32,
        verified_data: T,
    ) -> TreeHash {
        let self_hash =
            VerificationLayer1stCurryArgs::curry_tree_hash(revocation_singleton_launcher_id);

        CurriedProgram {
            program: self_hash,
            args: VerificationLayer2ndCurryArgs {
                self_hash: self_hash.into(),
                verified_data: verified_data.tree_hash(),
            },
        }
        .tree_hash()
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct VerificationLayerSolution {
    pub revocation_singleton_inner_puzzle_hash: Option<Bytes32>,
}

impl VerificationLayerSolution {
    pub fn oracle() -> Self {
        Self {
            revocation_singleton_inner_puzzle_hash: None,
        }
    }

    pub fn revocation(revocation_singleton_inner_puzzle_hash: Bytes32) -> Self {
        Self {
            revocation_singleton_inner_puzzle_hash: Some(revocation_singleton_inner_puzzle_hash),
        }
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
