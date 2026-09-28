use chia_protocol::{Bytes32, Coin};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;

use crate::{Cat, Did, Nft, OptionContract, OutputConstraints};

pub trait Asset {
    fn coin(&self) -> Coin;
    fn p2_puzzle_hash(&self) -> Bytes32;
    fn constraints(&self) -> OutputConstraints;

    fn coin_id(&self) -> Bytes32 {
        self.coin().coin_id()
    }

    fn full_puzzle_hash(&self) -> Bytes32 {
        self.coin().puzzle_hash
    }

    fn amount(&self) -> u64 {
        self.coin().amount
    }
}

impl Asset for Coin {
    fn coin(&self) -> Coin {
        *self
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.puzzle_hash
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: false,
            settlement: self.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}

impl Asset for Cat {
    fn coin(&self) -> Coin {
        self.coin
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: false,
            settlement: self.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}

impl Asset for Did {
    fn coin(&self) -> Coin {
        self.coin
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: true,
            settlement: self.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}

impl Asset for Nft {
    fn coin(&self) -> Coin {
        self.coin
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: true,
            settlement: self.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}

impl Asset for OptionContract {
    fn coin(&self) -> Coin {
        self.coin
    }

    fn p2_puzzle_hash(&self) -> Bytes32 {
        self.info.p2_puzzle_hash
    }

    fn constraints(&self) -> OutputConstraints {
        OutputConstraints {
            singleton: true,
            settlement: self.info.p2_puzzle_hash == SETTLEMENT_PAYMENT_HASH.into(),
        }
    }
}
