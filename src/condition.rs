use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use clvm_traits::{
    clvm_list, clvm_tuple, destructure_list, destructure_tuple, match_list, match_tuple, FromClvm,
    ToClvm,
};
use clvmr::{allocator::NodePtr, Allocator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    Remark,
    AggSigParent {
        public_key: PublicKey,
        message: Bytes,
    },
    AggSigPuzzle {
        public_key: PublicKey,
        message: Bytes,
    },
    AggSigAmount {
        public_key: PublicKey,
        message: Bytes,
    },
    AggSigPuzzleAmount {
        public_key: PublicKey,
        message: Bytes,
    },
    AggSigParentAmount {
        public_key: PublicKey,
        message: Bytes,
    },
    AggSigParentPuzzle {
        public_key: PublicKey,
        message: Bytes,
    },
    AggSigUnsafe {
        public_key: PublicKey,
        message: Bytes,
    },
    AggSigMe {
        public_key: PublicKey,
        message: Bytes,
    },
    CreateCoin {
        puzzle_hash: Bytes32,
        amount: u64,
        memos: Vec<Bytes32>,
    },
    ReserveFee {
        amount: u64,
    },
    CreateCoinAnnouncement {
        message: Bytes,
    },
    AssertCoinAnnouncement {
        announcement_id: Bytes,
    },
    CreatePuzzleAnnouncement {
        message: Bytes,
    },
    AssertPuzzleAnnouncement {
        announcement_id: Bytes,
    },
    AssertConcurrentSpend {
        coin_id: Bytes32,
    },
    AssertConcurrentPuzzle {
        puzzle_hash: Bytes32,
    },
    AssertMyCoinId {
        coin_id: Bytes32,
    },
    AssertMyParentId {
        parent_id: Bytes32,
    },
    AssertMyPuzzleHash {
        puzzle_hash: Bytes32,
    },
    AssertMyAmount {
        amount: u64,
    },
    AssertMyBirthSeconds {
        seconds: u64,
    },
    AssertMyBirthHeight {
        block_height: u32,
    },
    AssertEphemeral,
    AssertSecondsRelative {
        seconds: u64,
    },
    AssertSecondsAbsolute {
        seconds: u64,
    },
    AssertHeightRelative {
        block_height: u32,
    },
    AssertHeightAbsolute {
        block_height: u32,
    },
    AssertBeforeSecondsRelative {
        seconds: u64,
    },
    AssertBeforeSecondsAbsolute {
        seconds: u64,
    },
    AssertBeforeHeightRelative {
        block_height: u32,
    },
    AssertBeforeHeightAbsolute {
        block_height: u32,
    },
    Softfork {
        cost: u64,
        rest: NodePtr,
    },
}

impl ToClvm for Condition {
    fn to_clvm(&self, a: &mut Allocator) -> clvm_traits::Result<NodePtr> {
        match self {
            Condition::Remark => clvm_list!(1).to_clvm(a),
            Condition::AggSigParent {
                public_key,
                message,
            } => clvm_list!(43, public_key, message).to_clvm(a),
            Condition::AggSigPuzzle {
                public_key,
                message,
            } => clvm_list!(44, public_key, message).to_clvm(a),
            Condition::AggSigAmount {
                public_key,
                message,
            } => clvm_list!(45, public_key, message).to_clvm(a),
            Condition::AggSigPuzzleAmount {
                public_key,
                message,
            } => clvm_list!(46, public_key, message).to_clvm(a),
            Condition::AggSigParentAmount {
                public_key,
                message,
            } => clvm_list!(47, public_key, message).to_clvm(a),
            Condition::AggSigParentPuzzle {
                public_key,
                message,
            } => clvm_list!(48, public_key, message).to_clvm(a),
            Condition::AggSigUnsafe {
                public_key,
                message,
            } => clvm_list!(49, public_key, message).to_clvm(a),
            Condition::AggSigMe {
                public_key,
                message,
            } => clvm_list!(50, public_key, message).to_clvm(a),
            Condition::CreateCoin {
                puzzle_hash,
                amount,
                memos,
            } => {
                if memos.is_empty() {
                    clvm_list!(51, puzzle_hash, amount).to_clvm(a)
                } else {
                    clvm_list!(51, puzzle_hash, amount, memos).to_clvm(a)
                }
            }
            Condition::ReserveFee { amount } => clvm_list!(52, amount).to_clvm(a),
            Condition::CreateCoinAnnouncement { message } => clvm_list!(60, message).to_clvm(a),
            Condition::AssertCoinAnnouncement { announcement_id } => {
                clvm_list!(61, announcement_id).to_clvm(a)
            }
            Condition::CreatePuzzleAnnouncement { message } => clvm_list!(62, message).to_clvm(a),
            Condition::AssertPuzzleAnnouncement { announcement_id } => {
                clvm_list!(63, announcement_id).to_clvm(a)
            }
            Condition::AssertConcurrentSpend { coin_id } => clvm_list!(64, coin_id).to_clvm(a),
            Condition::AssertConcurrentPuzzle { puzzle_hash } => {
                clvm_list!(65, puzzle_hash).to_clvm(a)
            }
            Condition::AssertMyCoinId { coin_id } => clvm_list!(70, coin_id).to_clvm(a),
            Condition::AssertMyParentId { parent_id } => clvm_list!(71, parent_id).to_clvm(a),
            Condition::AssertMyPuzzleHash { puzzle_hash } => clvm_list!(72, puzzle_hash).to_clvm(a),
            Condition::AssertMyAmount { amount } => clvm_list!(73, amount).to_clvm(a),
            Condition::AssertMyBirthSeconds { seconds } => clvm_list!(74, seconds).to_clvm(a),
            Condition::AssertMyBirthHeight { block_height } => {
                clvm_list!(75, block_height).to_clvm(a)
            }
            Condition::AssertEphemeral => clvm_list!(76).to_clvm(a),
            Condition::AssertSecondsRelative { seconds } => clvm_list!(80, seconds).to_clvm(a),
            Condition::AssertSecondsAbsolute { seconds } => clvm_list!(81, seconds).to_clvm(a),
            Condition::AssertHeightRelative { block_height } => {
                clvm_list!(82, block_height).to_clvm(a)
            }
            Condition::AssertHeightAbsolute { block_height } => {
                clvm_list!(83, block_height).to_clvm(a)
            }
            Condition::AssertBeforeSecondsRelative { seconds } => {
                clvm_list!(84, seconds).to_clvm(a)
            }
            Condition::AssertBeforeSecondsAbsolute { seconds } => {
                clvm_list!(85, seconds).to_clvm(a)
            }
            Condition::AssertBeforeHeightRelative { block_height } => {
                clvm_list!(86, block_height).to_clvm(a)
            }
            Condition::AssertBeforeHeightAbsolute { block_height } => {
                clvm_list!(87, block_height).to_clvm(a)
            }
            Condition::Softfork { cost, rest } => clvm_tuple!(90, cost, rest).to_clvm(a),
        }
    }
}

impl FromClvm for Condition {
    fn from_clvm(a: &Allocator, ptr: NodePtr) -> clvm_traits::Result<Self> {
        let destructure_tuple!(opcode, value) = <match_tuple!(u16, NodePtr)>::from_clvm(a, ptr)?;

        macro_rules! condition_list {
            ( $variant:ident $( , $name:ident: $ty:ty )* ) => {
                {
                    let destructure_list!( $( $name ),* ) = <match_list!( $( $ty ),* )>::from_clvm(a, value)?;
                    Self::$variant { $( $name ),* }
                }
            };
        }

        let condition = match opcode {
            1 => Self::Remark,
            43 => condition_list!(AggSigParent, public_key: PublicKey, message: Bytes),
            44 => condition_list!(AggSigPuzzle, public_key: PublicKey, message: Bytes),
            45 => condition_list!(AggSigAmount, public_key: PublicKey, message: Bytes),
            46 => condition_list!(AggSigPuzzleAmount, public_key: PublicKey, message: Bytes),
            47 => condition_list!(AggSigParentAmount, public_key: PublicKey, message: Bytes),
            48 => condition_list!(AggSigParentPuzzle, public_key: PublicKey, message: Bytes),
            49 => condition_list!(AggSigUnsafe, public_key: PublicKey, message: Bytes),
            50 => condition_list!(AggSigMe, public_key: PublicKey, message: Bytes),
            51 => {
                let destructure_tuple!(puzzle_hash, amount, memos) =
                    <match_tuple!(Bytes32, u64, Option<(Vec<Bytes32>, ())>)>::from_clvm(a, value)?;
                Self::CreateCoin {
                    puzzle_hash,
                    amount,
                    memos: memos.map(|memos| memos.0).unwrap_or_default(),
                }
            }
            52 => condition_list!(ReserveFee, amount: u64),
            60 => condition_list!(CreateCoinAnnouncement, message: Bytes),
            61 => condition_list!(AssertCoinAnnouncement, announcement_id: Bytes),
            62 => condition_list!(CreatePuzzleAnnouncement, message: Bytes),
            63 => condition_list!(AssertPuzzleAnnouncement, announcement_id: Bytes),
            64 => condition_list!(AssertConcurrentSpend, coin_id: Bytes32),
            65 => condition_list!(AssertConcurrentPuzzle, puzzle_hash: Bytes32),
            70 => condition_list!(AssertMyCoinId, coin_id: Bytes32),
            71 => condition_list!(AssertMyParentId, parent_id: Bytes32),
            72 => condition_list!(AssertMyPuzzleHash, puzzle_hash: Bytes32),
            73 => condition_list!(AssertMyAmount, amount: u64),
            74 => condition_list!(AssertMyBirthSeconds, seconds: u64),
            75 => condition_list!(AssertMyBirthHeight, block_height: u32),
            76 => condition_list!(AssertEphemeral),

            80 => condition_list!(AssertSecondsRelative, seconds: u64),
            81 => condition_list!(AssertSecondsAbsolute, seconds: u64),
            82 => condition_list!(AssertHeightRelative, block_height: u32),
            83 => condition_list!(AssertHeightAbsolute, block_height: u32),
            84 => {
                condition_list!(AssertBeforeSecondsRelative, seconds: u64)
            }
            85 => {
                condition_list!(AssertBeforeSecondsAbsolute, seconds: u64)
            }
            86 => {
                condition_list!(AssertBeforeHeightRelative, block_height: u32)
            }
            87 => {
                condition_list!(AssertBeforeHeightAbsolute, block_height: u32)
            }

            90 => {
                let destructure_tuple!(cost, rest) =
                    <match_tuple!(u64, NodePtr)>::from_clvm(a, value)?;
                Self::Softfork { cost, rest }
            }
            _ => {
                return Err(clvm_traits::Error::Custom(format!(
                    "unknown opcode {opcode}"
                )))
            }
        };
        Ok(condition)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatCondition {
    Normal(Condition),
    Melt {
        puzzle_hash: Bytes32,
        memos: Vec<Bytes32>,
    },
}

impl ToClvm for CatCondition {
    fn to_clvm(&self, a: &mut Allocator) -> clvm_traits::Result<NodePtr> {
        match self {
            Self::Normal(condition) => condition.to_clvm(a),
            Self::Melt { puzzle_hash, memos } => {
                if memos.is_empty() {
                    clvm_list!(51, puzzle_hash, -113).to_clvm(a)
                } else {
                    clvm_list!(51, puzzle_hash, -113, memos).to_clvm(a)
                }
            }
        }
    }
}

impl FromClvm for CatCondition {
    fn from_clvm(a: &Allocator, ptr: NodePtr) -> clvm_traits::Result<Self> {
        match Condition::from_clvm(a, ptr) {
            Ok(condition) => Ok(Self::Normal(condition)),
            Err(error) => {
                let destructure_tuple!(opcode, puzzle_hash, amount, memos) =
                    <match_tuple!(u16, Bytes32, i64, Option<(Vec<Bytes32>, ())>)>::from_clvm(
                        a, ptr,
                    )?;

                if opcode != 51 || amount != -113 {
                    Err(error)
                } else {
                    Ok(Self::Melt {
                        puzzle_hash,
                        memos: memos.map(|memos| memos.0).unwrap_or_default(),
                    })
                }
            }
        }
    }
}
