use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::Memos;
use chia_sdk_derive::conditions;
use nfts::TradePrice;

mod agg_sig;
mod announcements;
mod list;
mod messages;
mod nfts;

pub use announcements::*;
pub use list::*;
pub use messages::*;

conditions! {
    pub enum Condition<T> {
        Remark<T> as Copy {
            opcode: i8 if 1,
            ...rest: T,
        },
        AggSigParent {
            opcode: i8 if 43,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigPuzzle {
            opcode: i8 if 44,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigAmount {
            opcode: i8 if 45,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigPuzzleAmount {
            opcode: i8 if 46,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigParentAmount {
            opcode: i8 if 47,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigParentPuzzle {
            opcode: i8 if 48,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigUnsafe {
            opcode: i8 if 49,
            public_key: PublicKey,
            message: Bytes,
        },
        AggSigMe {
            opcode: i8 if 50,
            public_key: PublicKey,
            message: Bytes,
        },
        CreateCoin<T> as Copy {
            opcode: i8 if 51,
            puzzle_hash: Bytes32,
            amount: u64,
            ...memos: Memos<T>,
        },
        ReserveFee as Copy {
            opcode: i8 if 52,
            amount: u64,
        },
        CreateCoinAnnouncement {
            opcode: i8 if 60,
            message: Bytes,
        },
        AssertCoinAnnouncement as Copy {
            opcode: i8 if 61,
            announcement_id: Bytes32,
        },
        CreatePuzzleAnnouncement {
            opcode: i8 if 62,
            message: Bytes,
        },
        AssertPuzzleAnnouncement as Copy {
            opcode: i8 if 63,
            announcement_id: Bytes32,
        },
        AssertConcurrentSpend as Copy {
            opcode: i8 if 64,
            coin_id: Bytes32,
        },
        AssertConcurrentPuzzle as Copy {
            opcode: i8 if 65,
            puzzle_hash: Bytes32,
        },
        SendMessage<T> {
            opcode: i8 if 66,
            mode: u8,
            message: Bytes,
            ...data: Vec<T>,
        },
        ReceiveMessage<T> {
            opcode: i8 if 67,
            mode: u8,
            message: Bytes,
            ...data: Vec<T>,
        },
        AssertMyCoinId as Copy {
            opcode: i8 if 70,
            coin_id: Bytes32,
        },
        AssertMyParentId as Copy {
            opcode: i8 if 71,
            parent_id: Bytes32,
        },
        AssertMyPuzzleHash as Copy {
            opcode: i8 if 72,
            puzzle_hash: Bytes32,
        },
        AssertMyAmount as Copy {
            opcode: i8 if 73,
            amount: u64,
        },
        AssertMyBirthSeconds as Copy {
            opcode: i8 if 74,
            seconds: u64,
        },
        AssertMyBirthHeight as Copy {
            opcode: i8 if 75,
            height: u32,
        },
        AssertEphemeral as Default + Copy {
            opcode: i8 if 76,
        },
        AssertSecondsRelative as Copy {
            opcode: i8 if 80,
            seconds: u64,
        },
        AssertSecondsAbsolute as Copy {
            opcode: i8 if 81,
            seconds: u64,
        },
        AssertHeightRelative as Copy {
            opcode: i8 if 82,
            height: u32,
        },
        AssertHeightAbsolute as Copy {
            opcode: i8 if 83,
            height: u32,
        },
        AssertBeforeSecondsRelative as Copy {
            opcode: i8 if 84,
            seconds: u64,
        },
        AssertBeforeSecondsAbsolute as Copy {
            opcode: i8 if 85,
            seconds: u64,
        },
        AssertBeforeHeightRelative as Copy {
            opcode: i8 if 86,
            height: u32,
        },
        AssertBeforeHeightAbsolute as Copy {
            opcode: i8 if 87,
            height: u32,
        },
        Softfork<T> as Copy {
            opcode: i8 if 90,
            cost: u64,
            ...rest: T,
        },
        TransferNft as Default {
            opcode: i8 if -10,
            launcher_id: Option<Bytes32>,
            trade_prices: Vec<TradePrice>,
            singleton_inner_puzzle_hash: Option<Bytes32>,
        },
        /// A `CREATE_COIN` with the magic -113 amount, followed by a TAIL reveal and solution.
        ///
        /// The meaning of this condition depends on the outer puzzle that consumes it. The CAT layer runs
        /// the revealed TAIL, but the singleton layer treats any -113 output as a melt, regardless of what
        /// follows the amount. Use [`Condition::is_magic_create_coin`] to detect either interpretation.
        RunCatTail<P, S> as Copy {
            opcode: i8 if 51,
            puzzle_hash: Option<Bytes32>,
            magic_amount: i8 if -113,
            program: P,
            solution: S,
        },
        /// A `CREATE_COIN` with the magic -113 amount, and nothing following it.
        ///
        /// The meaning of this condition depends on the outer puzzle that consumes it. The singleton layer
        /// melts the singleton, but the CAT layer treats any -113 output as a TAIL reveal (which fails here,
        /// since there's no TAIL). A singleton will also melt if it outputs a [`Condition::RunCatTail`].
        /// Use [`Condition::is_magic_create_coin`] to detect either interpretation.
        MeltSingleton as Default + Copy {
            opcode: i8 if 51,
            puzzle_hash: Option<Bytes32>,
            magic_amount: i8 if -113,
            // Nothing can follow the magic amount, so that this can't be confused with `RunCatTail`.
            ...rest: () if (),
        },
        UpdateNftMetadata<P, S> as Copy {
            opcode: i8 if -24,
            updater_puzzle_reveal: P,
            updater_solution: S,
        },
        UpdateDatastoreMerkleRoot {
            opcode: i8 if -13,
            new_merkle_root: Bytes32,
            ...memos: Vec<Bytes>,
        },
    }
}

impl<T> Condition<T> {
    /// Whether this is a `CREATE_COIN` with the magic -113 amount, which outer puzzles intercept rather
    /// than passing to consensus. This is either a [`Condition::RunCatTail`] or a [`Condition::MeltSingleton`],
    /// but the outer puzzle determines what it means, so code that handles one should usually handle both.
    pub fn is_magic_create_coin(&self) -> bool {
        self.is_run_cat_tail() || self.is_melt_singleton()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use clvm_traits::{FromClvm, ToClvm};
    use clvmr::{Allocator, NodePtr};

    use super::*;

    fn parse(allocator: &mut Allocator, value: impl ToClvm<Allocator>) -> Result<Condition> {
        let ptr = value.to_clvm(allocator)?;
        Ok(Condition::from_clvm(allocator, ptr)?)
    }

    #[test]
    fn test_shared_opcode_variants() -> Result<()> {
        let mut allocator = Allocator::new();
        let puzzle_hash = Bytes32::new([1; 32]);

        let condition = parse(&mut allocator, (51, (puzzle_hash, (1, ()))))?;
        assert!(condition.is_create_coin());
        assert!(!condition.is_magic_create_coin());

        for puzzle_hash in [None, Some(puzzle_hash)] {
            let condition = parse(&mut allocator, (51, (puzzle_hash, (-113, ((), ((), ()))))))?;
            assert_eq!(
                condition,
                Condition::run_cat_tail(puzzle_hash, NodePtr::NIL, NodePtr::NIL)
            );
            assert_eq!(
                condition.into_run_cat_tail().map(|tail| tail.puzzle_hash),
                Some(puzzle_hash)
            );

            // Arguments after the TAIL solution are ignored by the CAT layer.
            let condition = parse(
                &mut allocator,
                (51, (puzzle_hash, (-113, (1, (2, (3, ())))))),
            )?;
            assert!(condition.is_run_cat_tail());
            assert!(condition.is_magic_create_coin());

            let condition = parse(&mut allocator, (51, (puzzle_hash, (-113, ()))))?;
            assert_eq!(condition, Condition::melt_singleton(puzzle_hash));
            assert!(condition.is_magic_create_coin());
        }

        Ok(())
    }

    #[test]
    fn test_shared_opcode_roundtrip() -> Result<()> {
        let mut allocator = Allocator::new();

        for condition in [
            Condition::create_coin(Bytes32::new([1; 32]), 1, Memos::None),
            Condition::run_cat_tail(None, NodePtr::NIL, NodePtr::NIL),
            Condition::melt_singleton(None),
            Condition::melt_singleton(Some(Bytes32::new([1; 32]))),
        ] {
            assert_eq!(parse(&mut allocator, condition.clone())?, condition);
        }

        Ok(())
    }

    #[test]
    fn test_malformed_known_opcode() {
        let mut allocator = Allocator::new();

        // The mode must be an atom.
        assert!(parse(&mut allocator, (66, ((1, 2), (Bytes::default(), ())))).is_err());

        // A negative amount is only valid if it's the magic -113 amount.
        assert!(parse(&mut allocator, (51, (Bytes32::default(), (-1, ())))).is_err());

        // The puzzle hash must be nil or 32 bytes.
        assert!(parse(&mut allocator, (51, ((), (1, ())))).is_err());
        assert!(parse(&mut allocator, (51, (Bytes::new(vec![1; 31]), (-113, ())))).is_err());

        // A -113 output with a TAIL reveal but no solution is neither a melt nor a TAIL reveal.
        assert!(parse(&mut allocator, (51, ((), (-113, (1, ()))))).is_err());
    }

    #[test]
    fn test_unknown_opcode() -> Result<()> {
        let mut allocator = Allocator::new();

        let condition = parse(&mut allocator, (100, (1, ())))?;
        assert!(matches!(condition, Condition::Other(_)));

        // Opcodes are matched by their exact atom, so redundant leading zeros aren't a known opcode.
        let opcode = allocator.new_atom(&[0x00, 0x33])?;
        let condition = parse(&mut allocator, (opcode, (Bytes32::default(), (1, ()))))?;
        assert!(matches!(condition, Condition::Other(_)));

        let condition = parse(&mut allocator, NodePtr::NIL)?;
        assert!(matches!(condition, Condition::Other(_)));

        let condition = parse(&mut allocator, ((51, ()), ()))?;
        assert!(matches!(condition, Condition::Other(_)));

        Ok(())
    }
}
