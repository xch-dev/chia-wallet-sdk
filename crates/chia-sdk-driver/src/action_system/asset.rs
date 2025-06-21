use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;

use crate::{Cat, Did, HashedPtr, Nft, OptionContract, OutputConstraints};

pub trait Asset {
    fn coin_id(&self) -> Bytes32;
    fn full_puzzle_hash(&self) -> Bytes32;
    fn p2_puzzle_hash(&self) -> Bytes32;
    fn amount(&self) -> u64;
    fn constraints(&self) -> OutputConstraints;
}

impl Asset for Coin {
    fn coin_id(&self) -> Bytes32 {
        self.coin_id()
    }

    fn full_puzzle_hash(&self) -> Bytes32 {
        self.puzzle_hash
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.puzzle_hash
    }

    fn amount(&self) -> u64 {
        self.amount
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: false,
            settlement: self.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}

impl Asset for Cat {
    fn coin_id(&self) -> Bytes32 {
        self.coin.coin_id()
    }

    fn full_puzzle_hash(&self) -> Bytes32 {
        self.coin.puzzle_hash
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn amount(&self) -> u64 {
        self.coin.amount
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: false,
            settlement: self.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}

impl Asset for Did<HashedPtr> {
    fn coin_id(&self) -> Bytes32 {
        self.coin.coin_id()
    }

    fn full_puzzle_hash(&self) -> Bytes32 {
        self.coin.puzzle_hash
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn amount(&self) -> u64 {
        self.coin.amount
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: true,
            settlement: self.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}

impl Asset for Nft<HashedPtr> {
    fn coin_id(&self) -> Bytes32 {
        self.coin.coin_id()
    }

    fn full_puzzle_hash(&self) -> Bytes32 {
        self.coin.puzzle_hash
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn amount(&self) -> u64 {
        self.coin.amount
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: true,
            settlement: self.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}

impl Asset for OptionContract {
    fn coin_id(&self) -> Bytes32 {
        self.coin.coin_id()
    }

    fn full_puzzle_hash(&self) -> Bytes32 {
        self.coin.puzzle_hash
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn amount(&self) -> u64 {
        self.coin.amount
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: true,
            settlement: self.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}
