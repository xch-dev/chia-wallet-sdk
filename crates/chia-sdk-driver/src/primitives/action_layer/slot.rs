use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::singleton::{SingletonArgs, SingletonStruct};
use chia_sdk_types::puzzles::{Slot1stCurryArgs, Slot2ndCurryArgs, SlotInfo, SlotSolution};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

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
        Slot1stCurryArgs {
            singleton_struct: SingletonStruct::new(launcher_id),
            nonce,
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
        let self_program = ctx.curry(Slot1stCurryArgs {
            singleton_struct: SingletonStruct::new(self.info.launcher_id),
            nonce: self.info.nonce,
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
