use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_wallet::{nft::NftIntermediateLauncherArgs, singleton::SINGLETON_LAUNCHER_PUZZLE_HASH};
use clvm_utils::CurriedProgram;
use sha2::{Digest, Sha256};

use crate::{
    usize_to_bytes, AssertCoinAnnouncement, ChainedSpend, CreateCoinWithoutMemos, LaunchSingleton,
    SpendContext, SpendError,
};

pub fn intermediate_launcher(
    ctx: &mut SpendContext,
    parent_coin_id: Bytes32,
    index: usize,
    total: usize,
) -> Result<(ChainedSpend, LaunchSingleton), SpendError> {
    let mut coin_spends = Vec::new();
    let mut parent_conditions = Vec::new();

    let intermediate_puzzle = ctx.nft_intermediate_launcher();

    let puzzle = ctx.alloc(CurriedProgram {
        program: intermediate_puzzle,
        args: NftIntermediateLauncherArgs {
            launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            mint_number: index,
            mint_total: total,
        },
    })?;

    let puzzle_hash = ctx.tree_hash(puzzle);

    parent_conditions.push(ctx.alloc(CreateCoinWithoutMemos {
        puzzle_hash,
        amount: 0,
    })?);

    let puzzle_reveal = ctx.serialize(puzzle)?;
    let solution = ctx.serialize(())?;

    let intermediate_coin = Coin::new(parent_coin_id, puzzle_hash, 0);
    let intermediate_id = intermediate_coin.coin_id();

    coin_spends.push(CoinSpend::new(intermediate_coin, puzzle_reveal, solution));

    let mut index_message = Sha256::new();
    index_message.update(usize_to_bytes(index));
    index_message.update(usize_to_bytes(total));

    let mut announcement_id = Sha256::new();
    announcement_id.update(intermediate_id);
    announcement_id.update(index_message.finalize());

    parent_conditions.push(ctx.alloc(AssertCoinAnnouncement {
        announcement_id: Bytes32::new(announcement_id.finalize().into()),
    })?);

    let chained_spend = ChainedSpend {
        coin_spends,
        parent_conditions,
    };
    let launcher = LaunchSingleton::new(intermediate_id, 1);

    Ok((chained_spend, launcher))
}
