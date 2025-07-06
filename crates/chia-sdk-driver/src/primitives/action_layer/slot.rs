use chia::{
    clvm_utils::{CurriedProgram, ToTreeHash, TreeHash},
    protocol::{Bytes32, Coin, CoinSpend},
    puzzles::singleton::{SingletonArgs, SingletonStruct},
};
use chia_wallet_sdk::driver::{DriverError, SpendContext};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::SpendContextExt;

use super::SlotInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotProof {
    pub parent_parent_info: Bytes32,
    pub parent_inner_puzzle_hash: Bytes32,
}

impl SlotProof {
    pub fn slot_parent_id(&self, launcher_id: Bytes32) -> Bytes32 {
        Coin::new(
            self.parent_parent_info,
            SingletonArgs::curry_tree_hash(launcher_id, self.parent_inner_puzzle_hash.into())
                .into(),
            1,
        )
        .coin_id()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Slot<V> {
    pub coin: Coin,
    pub proof: SlotProof,

    pub info: SlotInfo<V>,
}

impl<V> Slot<V> {
    pub fn new(proof: SlotProof, info: SlotInfo<V>) -> Self {
        let parent_coin_id = proof.slot_parent_id(info.launcher_id);

        Self {
            coin: Coin::new(parent_coin_id, Slot::<V>::puzzle_hash(&info).into(), 0),
            proof,
            info,
        }
    }

    pub fn first_curry_hash(launcher_id: Bytes32, nonce: u64) -> TreeHash {
        CurriedProgram {
            program: SLOT_PUZZLE_HASH,
            args: Slot1stCurryArgs {
                singleton_struct: SingletonStruct::new(launcher_id),
                nonce,
            },
        }
        .tree_hash()
    }

    pub fn puzzle_hash(info: &SlotInfo<V>) -> TreeHash {
        CurriedProgram {
            program: Self::first_curry_hash(info.launcher_id, info.nonce),
            args: Slot2ndCurryArgs {
                value_hash: info.value_hash,
            },
        }
        .tree_hash()
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let program = ctx.slot_puzzle()?;
        let self_program = ctx.alloc(&CurriedProgram {
            program,
            args: Slot1stCurryArgs {
                singleton_struct: SingletonStruct::new(self.info.launcher_id),
                nonce: self.info.nonce,
            },
        })?;

        ctx.alloc(&CurriedProgram {
            program: self_program,
            args: Slot2ndCurryArgs {
                value_hash: self.info.value_hash,
            },
        })
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        spender_inner_puzzle_hash: Bytes32,
    ) -> Result<(), DriverError> {
        let puzzle_reveal = self.construct_puzzle(ctx)?;
        let puzzle_reveal = ctx.serialize(&puzzle_reveal)?;

        let solution = ctx.serialize(&SlotSolution {
            parent_parent_info: self.proof.parent_parent_info,
            parent_inner_puzzle_hash: self.proof.parent_inner_puzzle_hash,
            spender_inner_puzzle_hash,
        })?;

        ctx.insert(CoinSpend::new(self.coin, puzzle_reveal, solution));

        Ok(())
    }
}

pub const SLOT_PUZZLE: [u8; 456] = hex!("ff02ffff01ff04ffff04ff08ffff04ffff30ff2fffff02ff1effff04ff02ffff04ff05ffff04ff5fff8080808080ffff010180ff808080ffff04ffff04ff14ffff04ffff0112ffff04ff80ffff04ffff02ff1effff04ff02ffff04ff05ffff04ff81bfff8080808080ff8080808080ff808080ffff04ffff01ffff47ff4302ffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff16ffff04ff02ffff04ff09ff80808080ffff02ff16ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff0bff2affff0bff1cffff0bff1cff32ff0980ffff0bff1cffff0bff3affff0bff1cffff0bff1cff32ffff02ff16ffff04ff02ffff04ff05ff8080808080ffff0bff1cffff0bff3affff0bff1cffff0bff1cff32ff0b80ffff0bff1cff32ff22808080ff22808080ff22808080ff018080");

pub const SLOT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    66460af4bd504bc5e26f05698530a46fceb764b354727faf620e3a49065fa513
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct Slot1stCurryArgs {
    pub singleton_struct: SingletonStruct,
    pub nonce: u64,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct Slot2ndCurryArgs {
    pub value_hash: Bytes32,
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SlotSolution {
    pub parent_parent_info: Bytes32,
    pub parent_inner_puzzle_hash: Bytes32,
    pub spender_inner_puzzle_hash: Bytes32,
}
