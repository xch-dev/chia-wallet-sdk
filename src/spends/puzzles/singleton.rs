use chia_protocol::{Bytes32, Coin};
use chia_wallet::singleton::SINGLETON_LAUNCHER_PUZZLE_HASH;
use clvmr::NodePtr;

use crate::{CreateCoinWithoutMemos, CreatePuzzleAnnouncement, SpendContext, SpendError};

/// The information required to create a new singleton launcher.
pub struct Launcher {
    /// The conditions that must be output from the parent to make this singleton launcher valid.
    pub parent_conditions: Vec<NodePtr>,
    /// The singleton launcher coin.
    pub coin: Coin,
}

/// Creates a new singleton launcher coin.
pub fn create_launcher(
    ctx: &mut SpendContext,
    parent_coin_id: Bytes32,
) -> Result<Launcher, SpendError> {
    let mut parent_conditions = vec![ctx.alloc(CreateCoinWithoutMemos {
        puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
        amount: 1,
    })?];

    let launcher_coin = Coin::new(parent_coin_id, SINGLETON_LAUNCHER_PUZZLE_HASH.into(), 1);
    let launcher_id = launcher_coin.coin_id();

    parent_conditions.push(ctx.alloc(CreatePuzzleAnnouncement {
        message: launcher_id.to_vec().into(),
    })?);

    Ok(Launcher {
        parent_conditions,
        coin: launcher_coin,
    })
}
