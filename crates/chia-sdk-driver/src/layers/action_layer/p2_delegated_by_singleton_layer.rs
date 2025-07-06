use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::Bytes32,
    puzzles::singleton::SingletonStruct,
};
use chia_puzzles::SINGLETON_TOP_LAYER_V1_1_HASH;
use chia_wallet_sdk::driver::{DriverError, Layer, Puzzle, SpendContext};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

use crate::SpendContextExt;

#[derive(Debug, Clone)]
#[must_use]
pub struct P2DelegatedBySingletonLayer {
    pub singleton_struct_hash: Bytes32,
    pub nonce: u64,
}

impl P2DelegatedBySingletonLayer {
    pub fn new(singleton_struct_hash: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_struct_hash,
            nonce,
        }
    }
}

impl Layer for P2DelegatedBySingletonLayer {
    type Solution = P2DelegatedBySingletonLayerSolution<NodePtr, NodePtr>;

    fn parse_puzzle(allocator: &Allocator, puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH {
            return Ok(None);
        }

        let args = P2DelegatedBySingletonLayerArgs::from_clvm(allocator, puzzle.args)?;

        if args.singleton_mod_hash != SINGLETON_TOP_LAYER_V1_1_HASH.into() {
            return Ok(None);
        }

        Ok(Some(Self {
            singleton_struct_hash: args.singleton_struct_hash,
            nonce: args.nonce,
        }))
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        P2DelegatedBySingletonLayerSolution::from_clvm(allocator, solution)
            .map_err(DriverError::FromClvm)
    }

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        Ok(CurriedProgram {
            program: ctx.p2_delegated_by_singleton_puzzle()?,
            args: P2DelegatedBySingletonLayerArgs::new(self.singleton_struct_hash, self.nonce),
        }
        .to_clvm(ctx)?)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        solution.to_clvm(ctx).map_err(DriverError::ToClvm)
    }
}

pub const P2_DELEGATED_BY_SINGLETON_PUZZLE: [u8; 382] = hex!("ff02ffff01ff04ffff04ff08ffff04ffff0117ffff04ffff02ff0effff04ff02ffff04ff5fff80808080ffff04ffff0bff2affff0bff0cffff0bff0cff32ff0580ffff0bff0cffff0bff3affff0bff0cffff0bff0cff32ff0b80ffff0bff0cffff0bff3affff0bff0cffff0bff0cff32ff2f80ffff0bff0cff32ff22808080ff22808080ff22808080ff8080808080ffff02ff5fff81bf8080ffff04ffff01ffff4302ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff0effff04ff02ffff04ff09ff80808080ffff02ff0effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");

pub const P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    25fbd0d4586ff8266eb8b0fc4768b7714394d87f87824b0124fc10806ba87bb5
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2DelegatedBySingletonLayerArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_struct_hash: Bytes32,
    pub nonce: u64,
}

impl P2DelegatedBySingletonLayerArgs {
    pub fn new(singleton_struct_hash: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash,
            nonce,
        }
    }

    pub fn from_launcher_id(launcher_id: Bytes32, nonce: u64) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_struct_hash: SingletonStruct::new(launcher_id).tree_hash().into(),
            nonce,
        }
    }

    pub fn curry_tree_hash(singleton_struct_hash: Bytes32, nonce: u64) -> TreeHash {
        CurriedProgram {
            program: P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH,
            args: Self::new(singleton_struct_hash, nonce),
        }
        .tree_hash()
    }

    pub fn curry_tree_hash_with_launcher_id(launcher_id: Bytes32, nonce: u64) -> TreeHash {
        CurriedProgram {
            program: P2_DELEGATED_BY_SINGLETON_PUZZLE_HASH,
            args: Self::from_launcher_id(launcher_id, nonce),
        }
        .tree_hash()
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct P2DelegatedBySingletonLayerSolution<P, S> {
    pub singleton_inner_puzzle_hash: Bytes32,
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}
