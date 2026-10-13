use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::Memos;
use chia_sdk_derive::conditions;
use nfts::TradePrice;

mod agg_sig;
mod announcements;
mod canonical;
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
        MagicCreateCoin<T> as Copy {
            opcode: i8 if 51,
            puzzle_hash: Bytes32,
            magic_amount: i8 if -113,
            ...rest: T,
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
            mode: u8 where mode <= 0b11_1111,
            message: Bytes,
            ...data: Vec<T>,
        },
        ReceiveMessage<T> {
            opcode: i8 if 67,
            mode: u8 where mode <= 0b11_1111,
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
        RunCatTail<P, S> as Copy {
            opcode: i8 if 51,
            puzzle_hash: () if (),
            magic_amount: i8 if -113,
            program: P,
            solution: S,
        },
        MeltSingleton as Default + Copy {
            opcode: i8 if 51,
            puzzle_hash: () if (),
            magic_amount: i8 if -113,
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
    /// Returns `true` if this is a `CREATE_COIN` with the magic amount of -113, regardless of
    /// its puzzle hash or any additional arguments.
    ///
    /// Consensus would reject this as a negative amount, but the singleton top layer treats it as
    /// a melt and the CAT layer treats it as a request to run the TAIL, both before it's output.
    pub fn has_magic_amount(&self) -> bool {
        matches!(
            self,
            Self::MagicCreateCoin(..) | Self::RunCatTail(..) | Self::MeltSingleton(..)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use clvm_traits::{FromClvm, ToClvm};
    use clvmr::{Allocator, NodePtr};

    const PUZZLE_HASH: [u8; 32] = [0x42; 32];

    /// Builds a list of atoms with the exact bytes given.
    fn list(allocator: &mut Allocator, atoms: &[&[u8]]) -> NodePtr {
        let mut node = NodePtr::NIL;
        for atom in atoms.iter().rev() {
            let atom = allocator.new_atom(atom).unwrap();
            node = allocator.new_pair(atom, node).unwrap();
        }
        node
    }

    fn parse(atoms: &[&[u8]]) -> Condition {
        let mut allocator = Allocator::new();
        let node = list(&mut allocator, atoms);
        Condition::from_clvm(&allocator, node).unwrap()
    }

    #[test]
    fn test_magic_amount_with_puzzle_hash() {
        // (51 <ph> -113)
        let condition = parse(&[&[51], &PUZZLE_HASH, &[0x8F]]);
        assert!(!condition.is_create_coin());
        assert!(condition.is_magic_create_coin());
        assert!(condition.has_magic_amount());
        assert_eq!(
            condition.into_magic_create_coin().unwrap().puzzle_hash,
            Bytes32::new(PUZZLE_HASH)
        );

        let mut allocator = Allocator::new();
        let node = list(&mut allocator, &[&[51], &PUZZLE_HASH, &[0x8F]]);
        assert!(conditions::CreateCoin::<NodePtr>::from_clvm(&allocator, node).is_err());
    }

    #[test]
    fn test_magic_amount_without_puzzle_hash() {
        // (51 () -113)
        let condition = parse(&[&[51], &[], &[0x8F]]);
        assert!(condition.is_melt_singleton());
        assert!(condition.has_magic_amount());

        // (51 () -113 tail solution)
        let condition = parse(&[&[51], &[], &[0x8F], &[1], &[2]]);
        assert!(condition.is_run_cat_tail());
        assert!(condition.has_magic_amount());
    }

    #[test]
    fn test_non_canonical_magic_amount() {
        // (51 <ph> 0xFF8F) isn't -113 to the singleton or CAT puzzles, which compare bytes.
        let condition = parse(&[&[51], &PUZZLE_HASH, &[0xFF, 0x8F]]);
        assert!(matches!(condition, Condition::Other(..)));
    }

    #[test]
    fn test_negative_amount() {
        // (51 <ph> -1)
        let condition = parse(&[&[51], &PUZZLE_HASH, &[0xFF]]);
        assert!(matches!(condition, Condition::Other(..)));

        // (52 -1)
        let condition = parse(&[&[52], &[0xFF]]);
        assert!(matches!(condition, Condition::Other(..)));
    }

    #[test]
    fn test_two_byte_opcode() {
        // (0x0033 <ph> 1) is an unknown condition to consensus.
        let condition = parse(&[&[0x00, 0x33], &PUZZLE_HASH, &[1]]);
        assert!(matches!(condition, Condition::Other(..)));
    }

    #[test]
    fn test_oversized_amount() {
        // (51 <ph> 2^64+5) would be truncated to 5 by clvm-traits.
        let amount = [0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05];
        let condition = parse(&[&[51], &PUZZLE_HASH, &amount]);
        assert!(matches!(condition, Condition::Other(..)));
    }

    #[test]
    fn test_redundant_leading_zero_amount() {
        // (51 <ph> 0x0001) is rejected by consensus.
        let condition = parse(&[&[51], &PUZZLE_HASH, &[0x00, 0x01]]);
        assert!(matches!(condition, Condition::Other(..)));
    }

    #[test]
    fn test_negative_height() {
        // (82 -1) would be parsed as a height of 255 by clvm-traits.
        let condition = parse(&[&[82], &[0xFF]]);
        assert!(matches!(condition, Condition::Other(..)));
    }

    #[test]
    fn test_message_mode() {
        // (66 63 "hello")
        let condition = parse(&[&[66], &[63], b"hello"]);
        assert_eq!(condition.into_send_message().unwrap().mode, 63);

        // (66 64 "hello") and (67 255 "hello") have invalid modes.
        let condition = parse(&[&[66], &[64], b"hello"]);
        assert!(matches!(condition, Condition::Other(..)));
        let condition = parse(&[&[67], &[0x00, 0xFF], b"hello"]);
        assert!(matches!(condition, Condition::Other(..)));
    }

    #[test]
    fn test_canonical_amounts() {
        let cases: [(&[u8], u64); 4] = [
            (&[], 0),
            (&[0x7F], 0x7F),
            (&[0x00, 0x80], 0x80),
            (
                &[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
                u64::MAX,
            ),
        ];

        for (atom, amount) in cases {
            let condition = parse(&[&[51], &PUZZLE_HASH, atom]);
            assert_eq!(condition.into_create_coin().unwrap().amount, amount);
        }
    }

    #[test]
    fn test_roundtrip() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let conditions: Vec<Condition> = vec![
            Condition::create_coin(Bytes32::new(PUZZLE_HASH), u64::MAX, Memos::None),
            Condition::magic_create_coin(Bytes32::new(PUZZLE_HASH), NodePtr::NIL),
            Condition::melt_singleton(),
            Condition::reserve_fee(0),
            Condition::assert_height_relative(u32::MAX),
            Condition::send_message(0b11_1111, Bytes::default(), vec![]),
            Condition::remark(NodePtr::NIL),
            Condition::assert_ephemeral(),
        ];

        for condition in conditions {
            let node = condition.to_clvm(&mut allocator)?;
            assert_eq!(Condition::from_clvm(&allocator, node)?, condition);
        }

        Ok(())
    }
}
