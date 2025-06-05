use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::{nft::NftIntermediateLauncherArgs, Memos};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_types::{announcement_id, Conditions};
use chia_sha2::Sha256;
use clvmr::Allocator;

use crate::{DriverError, SpendContext};

use super::Launcher;

/// An intermediate launcher is a coin that is created prior to the actual launcher coin.
/// In this case, it automatically creates the launcher coin upon being spent.
///
/// The purpose of this is to allow multiple launcher coins to be created from a single parent.
/// Without an intermediate launcher, they would all have the same coin id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct IntermediateLauncher {
    mint_number: usize,
    mint_total: usize,
    intermediate_coin: Coin,
    launcher_coin: Coin,
}

impl IntermediateLauncher {
    /// Create a new intermediate launcher with the given index. This makes the puzzle hash, and therefore coin id, unique.
    pub fn new(parent_coin_id: Bytes32, mint_number: usize, mint_total: usize) -> Self {
        let intermediate_puzzle_hash =
            NftIntermediateLauncherArgs::curry_tree_hash(mint_number, mint_total).into();

        let intermediate_coin = Coin::new(parent_coin_id, intermediate_puzzle_hash, 0);

        let launcher_coin = Coin::new(
            intermediate_coin.coin_id(),
            SINGLETON_LAUNCHER_HASH.into(),
            1,
        );

        Self {
            mint_number,
            mint_total,
            intermediate_coin,
            launcher_coin,
        }
    }

    /// The intermediate coin that will be created when the parent is spent.
    pub fn intermediate_coin(&self) -> Coin {
        self.intermediate_coin
    }

    /// The singleton launcher coin that will be created when the intermediate coin is spent.
    pub fn launcher_coin(&self) -> Coin {
        self.launcher_coin
    }

    /// Spends the intermediate coin to create the launcher coin.
    pub fn create(self, ctx: &mut SpendContext) -> Result<Launcher, DriverError> {
        let mut parent = Conditions::new();

        let puzzle = ctx.curry(NftIntermediateLauncherArgs::new(
            self.mint_number,
            self.mint_total,
        ))?;

        parent = parent.create_coin(self.intermediate_coin.puzzle_hash, 0, Memos::None);

        let puzzle_reveal = ctx.serialize(&puzzle)?;
        let solution = ctx.serialize(&())?;

        ctx.insert(CoinSpend::new(
            self.intermediate_coin,
            puzzle_reveal,
            solution,
        ));

        let mut index_message = Sha256::new();
        index_message.update(usize_to_bytes(self.mint_number));
        index_message.update(usize_to_bytes(self.mint_total));

        Ok(Launcher::from_coin(
            self.launcher_coin,
            parent.assert_coin_announcement(announcement_id(
                self.intermediate_coin.coin_id(),
                index_message.finalize(),
            )),
        ))
    }
}

fn usize_to_bytes(value: usize) -> Vec<u8> {
    let mut allocator = Allocator::new();
    let atom = allocator.new_number(value.into()).unwrap();
    allocator.atom(atom).as_ref().to_vec()
}
