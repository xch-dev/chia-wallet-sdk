#![allow(clippy::missing_const_for_fn)]

use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::{NftIntermediateLauncherArgs, NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH},
    singleton::SINGLETON_LAUNCHER_PUZZLE_HASH,
};
use clvm_utils::{CurriedProgram, ToTreeHash};
use clvmr::{
    sha2::{Digest, Sha256},
    Allocator,
};

use crate::{
    spend_builder::{P2Spend, ParentConditions},
    SpendContext, SpendError,
};

use super::SpendableLauncher;

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct IntermediateLauncher {
    mint_number: usize,
    mint_total: usize,
    intermediate_coin: Coin,
    launcher_coin: Coin,
}

impl IntermediateLauncher {
    pub fn new(parent_coin_id: Bytes32, mint_number: usize, mint_total: usize) -> Self {
        let intermediate_puzzle_hash = CurriedProgram {
            program: NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH,
            args: NftIntermediateLauncherArgs {
                launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                mint_number,
                mint_total,
            },
        }
        .tree_hash()
        .into();

        let intermediate_coin = Coin::new(parent_coin_id, intermediate_puzzle_hash, 0);

        let launcher_coin = Coin::new(
            intermediate_coin.coin_id(),
            SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            1,
        );

        Self {
            mint_number,
            mint_total,
            intermediate_coin,
            launcher_coin,
        }
    }

    pub fn intermediate_coin(&self) -> Coin {
        self.intermediate_coin
    }

    pub fn launcher_coin(&self) -> Coin {
        self.launcher_coin
    }

    pub fn create(self, ctx: &mut SpendContext<'_>) -> Result<SpendableLauncher, SpendError> {
        let mut parent = ParentConditions::new();

        let intermediate_puzzle = ctx.nft_intermediate_launcher()?;

        let puzzle = ctx.alloc(&CurriedProgram {
            program: intermediate_puzzle,
            args: NftIntermediateLauncherArgs {
                launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                mint_number: self.mint_number,
                mint_total: self.mint_total,
            },
        })?;

        parent = parent.create_coin(ctx, self.intermediate_coin.puzzle_hash, 0)?;

        let puzzle_reveal = ctx.serialize(&puzzle)?;
        let solution = ctx.serialize(&())?;

        ctx.spend(CoinSpend::new(
            self.intermediate_coin,
            puzzle_reveal,
            solution,
        ));

        let mut index_message = Sha256::new();
        index_message.update(usize_to_bytes(self.mint_number));
        index_message.update(usize_to_bytes(self.mint_total));

        parent = parent.assert_coin_announcement(
            ctx,
            self.intermediate_coin.coin_id(),
            index_message.finalize(),
        )?;

        Ok(SpendableLauncher::with_parent(self.launcher_coin, parent))
    }
}

fn usize_to_bytes(value: usize) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let atom = allocator.new_number(value.into()).unwrap();
    allocator.atom(atom).as_ref().to_vec()
}
