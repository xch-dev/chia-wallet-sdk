use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzles::{
    nft::{NftIntermediateLauncherArgs, NFT_INTERMEDIATE_LAUNCHER_PUZZLE_HASH},
    singleton::SINGLETON_LAUNCHER_PUZZLE_HASH,
};
use clvm_utils::{CurriedProgram, ToTreeHash};
use sha2::{Digest, Sha256};

use crate::{
    usize_to_bytes, AssertCoinAnnouncement, ChainedSpend, CreateCoinWithoutMemos, SpendContext,
    SpendError, SpendableLauncher,
};

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

    pub fn create(self, ctx: &mut SpendContext) -> Result<SpendableLauncher, SpendError> {
        let mut parent_conditions = Vec::new();

        let intermediate_puzzle = ctx.nft_intermediate_launcher();

        let puzzle = ctx.alloc(CurriedProgram {
            program: intermediate_puzzle,
            args: NftIntermediateLauncherArgs {
                launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                mint_number: self.mint_number,
                mint_total: self.mint_total,
            },
        })?;

        let puzzle_hash = ctx.tree_hash(puzzle);

        parent_conditions.push(ctx.alloc(CreateCoinWithoutMemos {
            puzzle_hash: puzzle_hash.into(),
            amount: 0,
        })?);

        let puzzle_reveal = ctx.serialize(puzzle)?;
        let solution = ctx.serialize(())?;

        let intermediate_id = self.intermediate_coin.coin_id();

        ctx.spend(CoinSpend::new(
            self.intermediate_coin,
            puzzle_reveal,
            solution,
        ));

        let mut index_message = Sha256::new();
        index_message.update(usize_to_bytes(self.mint_number));
        index_message.update(usize_to_bytes(self.mint_total));

        let mut announcement_id = Sha256::new();
        announcement_id.update(intermediate_id);
        announcement_id.update(index_message.finalize());

        parent_conditions.push(ctx.alloc(AssertCoinAnnouncement {
            announcement_id: Bytes32::new(announcement_id.finalize().into()),
        })?);

        let chained_spend = ChainedSpend { parent_conditions };

        Ok(SpendableLauncher::new(self.launcher_coin, chained_spend))
    }
}
