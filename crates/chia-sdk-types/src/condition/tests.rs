use anyhow::Result;
use chia_bls::SecretKey;
use chia_consensus::{
    conditions::{Condition as ConsensusCondition, parse_args},
    flags::ConsensusFlags,
    opcodes::{
        AGG_SIG_AMOUNT, AGG_SIG_ME, AGG_SIG_PARENT, AGG_SIG_PARENT_AMOUNT, AGG_SIG_PARENT_PUZZLE,
        AGG_SIG_PUZZLE, AGG_SIG_PUZZLE_AMOUNT, AGG_SIG_UNSAFE, parse_opcode,
    },
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash;
use clvmr::{Allocator, NodePtr};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

use super::*;

fn parse(allocator: &mut Allocator, value: impl ToClvm<Allocator>) -> Result<Condition> {
    let ptr = value.to_clvm(allocator)?;
    Ok(Condition::from_clvm(allocator, ptr)?)
}

/// Parses a condition, and checks that it serializes back to exactly the same value.
fn roundtrip(allocator: &mut Allocator, value: impl ToClvm<Allocator>) -> Result<Condition> {
    let ptr = value.to_clvm(allocator)?;
    let condition = Condition::from_clvm(allocator, ptr)?;
    let serialized = condition.to_clvm(allocator)?;
    assert_eq!(tree_hash(allocator, serialized), tree_hash(allocator, ptr));
    Ok(condition)
}

#[test]
fn test_shared_opcode_variants() -> Result<()> {
    let mut allocator = Allocator::new();
    let puzzle_hash = Bytes32::new([1; 32]);

    let condition = roundtrip(&mut allocator, (51, (puzzle_hash, (1, ()))))?;
    assert!(condition.is_create_coin());
    assert!(!condition.is_magic_create_coin());

    for puzzle_hash in [None, Some(puzzle_hash)] {
        let condition = roundtrip(&mut allocator, (51, (puzzle_hash, (-113, ((), ((), ()))))))?;
        assert_eq!(
            condition,
            Condition::run_cat_tail(puzzle_hash, NodePtr::NIL, NodePtr::NIL)
        );
        assert!(condition.is_magic_create_coin());

        // Arguments after the TAIL solution are ignored by the CAT layer, but they're kept.
        let condition = roundtrip(
            &mut allocator,
            (51, (puzzle_hash, (-113, (1, (2, (3, ())))))),
        )?;
        assert!(condition.is_run_cat_tail());

        let condition = roundtrip(&mut allocator, (51, (puzzle_hash, (-113, ()))))?;
        assert_eq!(condition, Condition::melt_singleton(puzzle_hash));
        assert!(condition.is_magic_create_coin());
    }

    Ok(())
}

#[test]
fn test_constructed_roundtrip() -> Result<()> {
    let mut allocator = Allocator::new();

    for condition in [
        Condition::create_coin(Bytes32::new([1; 32]), 1, Memos::None),
        Condition::create_coin(Bytes32::new([1; 32]), 1, Memos::Some(NodePtr::NIL)),
        Condition::run_cat_tail(None, NodePtr::NIL, NodePtr::NIL),
        Condition::melt_singleton(None),
        Condition::melt_singleton(Some(Bytes32::new([1; 32]))),
        Condition::reserve_fee(0),
        Condition::assert_seconds_relative(u64::MAX),
        Condition::assert_height_absolute(u32::MAX),
        Condition::assert_ephemeral(),
        Condition::send_message(0, Bytes::new(vec![1; 1024]), Vec::new()),
    ] {
        assert_eq!(roundtrip(&mut allocator, condition.clone())?, condition);
    }

    Ok(())
}

#[test]
fn test_trailing_arguments() -> Result<()> {
    let mut allocator = Allocator::new();
    let puzzle_hash = Bytes32::new([1; 32]);

    // Memos that aren't a list are kept as is, as are any arguments after them.
    let condition = roundtrip(&mut allocator, (51, (puzzle_hash, (100, 5))))?;
    let create_coin = condition.into_create_coin().unwrap();
    assert_eq!(create_coin.memos, Memos::None);
    assert!(allocator.small_number(create_coin.rest) == Some(5));

    let condition = roundtrip(&mut allocator, (51, (puzzle_hash, (100, ((), (7, 8))))))?;
    let create_coin = condition.into_create_coin().unwrap();
    assert_eq!(create_coin.memos, Memos::Some(NodePtr::NIL));

    // Consensus ignores the arguments after the ones it reads.
    let condition = roundtrip(&mut allocator, (80, (5, (6, ()))))?;
    assert_eq!(
        condition.into_assert_seconds_relative().map(|c| c.seconds),
        Some(5)
    );

    let condition = roundtrip(&mut allocator, (76, 5))?;
    assert!(condition.is_assert_ephemeral());

    // The message data doesn't need to be a proper list.
    let condition = roundtrip(
        &mut allocator,
        (66, (0b010, (Bytes::default(), (puzzle_hash, 5)))),
    )?;
    let send_message = condition.into_send_message().unwrap();
    assert_eq!(send_message.data.len(), 1);

    // Conditions that are parsed on their own don't allow trailing arguments by default.
    let ptr = (52, (1, (2, ()))).to_clvm(&mut allocator)?;
    assert!(conditions::ReserveFee::<()>::from_clvm(&allocator, ptr).is_err());
    assert!(conditions::ReserveFee::<NodePtr>::from_clvm(&allocator, ptr).is_ok());

    Ok(())
}

#[test]
fn test_transfer_nft() -> Result<()> {
    let mut allocator = Allocator::new();
    let launcher_id = Bytes32::new([1; 32]);
    let puzzle_hash = Bytes32::new([2; 32]);

    let condition = roundtrip(
        &mut allocator,
        (
            -10,
            (
                launcher_id,
                ([(100, (puzzle_hash, ()))], (puzzle_hash, (7, ()))),
            ),
        ),
    )?;
    let transfer_nft = condition.into_transfer_nft().unwrap();
    assert_eq!(
        transfer_nft.trade_prices,
        vec![TradePrice::new(100, puzzle_hash)]
    );

    // A trade price with anything after the puzzle hash couldn't be serialized back to the same value.
    assert!(
        parse(
            &mut allocator,
            (
                -10,
                (
                    launcher_id,
                    ([(100, (puzzle_hash, (5, ())))], (puzzle_hash, ()))
                )
            )
        )
        .is_err()
    );

    Ok(())
}

#[test]
fn test_trivially_true() -> Result<()> {
    let mut allocator = Allocator::new();

    let negative = allocator.new_atom(&[0xff, 0xff])?;
    let overflow = allocator.new_atom(&[0x01; 9])?;

    for value in [
        (80, (-1, ())).to_clvm(&mut allocator)?,
        (81, (negative, ())).to_clvm(&mut allocator)?,
        (82, (-1, ())).to_clvm(&mut allocator)?,
        (85, (overflow, ())).to_clvm(&mut allocator)?,
        (87, (u64::MAX, ())).to_clvm(&mut allocator)?,
    ] {
        let condition = roundtrip(&mut allocator, value)?;
        assert!(matches!(condition, Condition::TriviallyTrue(_)));
    }

    // These can never be satisfied, so consensus rejects them.
    for value in [
        (80, (overflow, ())).to_clvm(&mut allocator)?,
        (84, (-1, ())).to_clvm(&mut allocator)?,
        (74, (-1, ())).to_clvm(&mut allocator)?,
    ] {
        assert!(Condition::<NodePtr>::from_clvm(&allocator, value).is_err());
    }

    Ok(())
}

#[test]
fn test_malformed_known_opcode() -> Result<()> {
    let mut allocator = Allocator::new();

    // The mode must be an atom.
    assert!(parse(&mut allocator, (66, ((1, 2), (Bytes::default(), ())))).is_err());

    // Only 6 bits of the mode can be set.
    assert!(parse(&mut allocator, (66, (64, (Bytes::default(), ())))).is_err());

    // A negative amount is only valid if it's the magic -113 amount.
    assert!(parse(&mut allocator, (51, (Bytes32::default(), (-1, ())))).is_err());

    // The puzzle hash must be nil or 32 bytes.
    assert!(parse(&mut allocator, (51, ((), (1, ())))).is_err());
    assert!(parse(&mut allocator, (51, (Bytes::new(vec![1; 31]), (-113, ())))).is_err());

    // A -113 output with a TAIL reveal but no solution is neither a melt nor a TAIL reveal.
    assert!(parse(&mut allocator, (51, ((), (-113, (1, ()))))).is_err());

    // Integers must be canonically encoded.
    let amount = allocator.new_atom(&[0x00, 0x01])?;
    assert!(parse(&mut allocator, (51, (Bytes32::default(), (amount, ())))).is_err());

    // Messages can't be longer than 1024 bytes.
    assert!(parse(&mut allocator, (60, (Bytes::new(vec![1; 1025]), ()))).is_err());

    // A condition must be a pair.
    assert!(parse(&mut allocator, NodePtr::NIL).is_err());
    assert!(parse(&mut allocator, 5).is_err());

    Ok(())
}

#[test]
fn test_unknown_opcode() -> Result<()> {
    let mut allocator = Allocator::new();

    let condition = roundtrip(&mut allocator, (100, (1, ())))?;
    assert!(matches!(condition, Condition::Other(_)));

    // Opcodes are matched by their exact atom, so redundant leading zeros aren't a known opcode.
    let opcode = allocator.new_atom(&[0x00, 0x33])?;
    let condition = roundtrip(&mut allocator, (opcode, (Bytes32::default(), (1, ()))))?;
    assert!(matches!(condition, Condition::Other(_)));

    // Consensus ignores conditions whose opcode isn't an atom.
    let condition = roundtrip(&mut allocator, ((51, ()), ()))?;
    assert!(matches!(condition, Condition::Other(_)));

    Ok(())
}

/// Generates CLVM values from building blocks that exercise the edge cases of the consensus rules.
struct Generator {
    rng: ChaCha8Rng,
    public_key: [u8; 48],
}

impl Generator {
    fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            public_key: SecretKey::from_seed(&[1; 32]).public_key().to_bytes(),
        }
    }

    fn atom(&mut self, allocator: &mut Allocator) -> NodePtr {
        let bytes: Vec<u8> = match self.rng.random_range(0..20) {
            0 => vec![],
            1 => vec![0],
            2 => vec![0, 0x33],
            3 => vec![self.rng.random_range(1..0x80)],
            4 => vec![0xff],
            5 => vec![0x8f],
            6 => vec![0x00, 0x80],
            7 => vec![0x80],
            8 => (0..32).map(|_| self.rng.random()).collect(),
            9 => self.public_key.to_vec(),
            10 => (0..48).map(|_| self.rng.random()).collect(),
            11 => vec![1; 31],
            12 => vec![1; self.rng.random_range(1024..=1025)],
            13 => vec![0x01; 9],
            14 => vec![0x01; 5],
            15 => vec![0x7f; 8],
            16 => {
                let mut bytes = vec![0xff; 9];
                bytes[0] = 0;
                bytes
            }
            17 => vec![0b11_1111],
            18 => vec![0x40],
            _ => vec![self.rng.random_range(0..0x40)],
        };

        allocator.new_atom(&bytes).unwrap()
    }

    fn value(&mut self, allocator: &mut Allocator, depth: usize) -> NodePtr {
        if depth == 0 || self.rng.random_bool(0.8) {
            return self.atom(allocator);
        }

        let first = self.value(allocator, depth - 1);
        let rest = self.value(allocator, depth - 1);
        allocator.new_pair(first, rest).unwrap()
    }

    fn opcode(&mut self, allocator: &mut Allocator) -> NodePtr {
        let bytes: Vec<u8> = match self.rng.random_range(0..10) {
            // Layer specific opcodes, which consensus doesn't know about.
            0 => vec![[0xf6, 0xe8, 0xf3][self.rng.random_range(0..3)]],
            1 => vec![100],
            2 => vec![0x01, 0x00],
            3 => vec![0, 0x33],
            4 => return self.value(allocator, 2),
            _ => {
                let opcodes = [
                    1, 43, 44, 45, 46, 47, 48, 49, 50, 51, 51, 51, 52, 60, 61, 62, 63, 64, 65, 66,
                    66, 67, 67, 70, 71, 72, 73, 74, 75, 76, 80, 81, 82, 83, 84, 85, 86, 87, 90,
                ];
                vec![opcodes[self.rng.random_range(0..opcodes.len())]]
            }
        };

        allocator.new_atom(&bytes).unwrap()
    }

    fn condition(&mut self, allocator: &mut Allocator) -> NodePtr {
        if self.rng.random_bool(0.05) {
            return self.magic_create_coin(allocator);
        }

        let mut node = if self.rng.random_bool(0.8) {
            NodePtr::NIL
        } else {
            self.value(allocator, 2)
        };

        for _ in 0..self.rng.random_range(0..6) {
            let arg = self.value(allocator, 2);
            node = allocator.new_pair(arg, node).unwrap();
        }

        if self.rng.random_bool(0.02) {
            return node;
        }

        let opcode = self.opcode(allocator);
        allocator.new_pair(opcode, node).unwrap()
    }
}

impl Generator {
    /// A `CREATE_COIN` with the magic -113 amount, which is rarely generated otherwise.
    fn magic_create_coin(&mut self, allocator: &mut Allocator) -> NodePtr {
        let mut node = if self.rng.random_bool(0.8) {
            NodePtr::NIL
        } else {
            self.atom(allocator)
        };

        for _ in 0..self.rng.random_range(0..4) {
            let arg = self.value(allocator, 2);
            node = allocator.new_pair(arg, node).unwrap();
        }

        let amount = allocator.new_atom(&[0x8f]).unwrap();
        node = allocator.new_pair(amount, node).unwrap();

        let puzzle_hash = match self.rng.random_range(0..3) {
            0 => NodePtr::NIL,
            1 => allocator.new_atom(&[1; 32]).unwrap(),
            _ => self.value(allocator, 1),
        };
        node = allocator.new_pair(puzzle_hash, node).unwrap();

        let opcode = allocator.new_atom(&[51]).unwrap();
        allocator.new_pair(opcode, node).unwrap()
    }
}

/// What consensus would do with a condition.
#[derive(Debug)]
enum Verdict {
    Rejected,
    Ignored,
    Accepted(ConsensusCondition),
}

fn consensus_verdict(allocator: &Allocator, node: NodePtr) -> Verdict {
    let Some((opcode, args)) = allocator.next(node) else {
        return Verdict::Rejected;
    };
    let Some(opcode) = parse_opcode(allocator, opcode, ConsensusFlags::empty()) else {
        return Verdict::Ignored;
    };
    match parse_args(allocator, args, opcode, ConsensusFlags::empty()) {
        Ok(condition) => Verdict::Accepted(condition),
        Err(_) => Verdict::Rejected,
    }
}

fn is_agg_sig_with_invalid_key(allocator: &Allocator, node: NodePtr) -> bool {
    let Some((opcode, args)) = allocator.next(node) else {
        return false;
    };
    let Some(opcode) = parse_opcode(allocator, opcode, ConsensusFlags::empty()) else {
        return false;
    };
    let Some((public_key, _)) = allocator.next(args) else {
        return false;
    };

    matches!(
        opcode,
        AGG_SIG_UNSAFE
            | AGG_SIG_ME
            | AGG_SIG_PARENT
            | AGG_SIG_PUZZLE
            | AGG_SIG_AMOUNT
            | AGG_SIG_PUZZLE_AMOUNT
            | AGG_SIG_PARENT_AMOUNT
            | AGG_SIG_PARENT_PUZZLE
    ) && chia_bls::PublicKey::from_clvm(allocator, public_key).is_err()
}

fn is_two_byte_opcode(allocator: &Allocator, node: NodePtr) -> bool {
    allocator.next(node).is_some_and(|(opcode, _)| {
        matches!(allocator.sexp(opcode), clvmr::SExp::Atom) && allocator.atom_len(opcode) == 2
    })
}

fn is_layer_opcode(allocator: &Allocator, node: NodePtr) -> bool {
    allocator.next(node).is_some_and(|(opcode, _)| {
        matches!(allocator.sexp(opcode), clvmr::SExp::Atom)
            && matches!(allocator.atom(opcode).as_ref(), [0xf6 | 0xe8 | 0xf3])
    })
}

#[test]
fn test_generated_conditions() {
    let mut generator = Generator::new(1337);
    let mut outcomes = std::collections::HashMap::<&str, usize>::new();

    for _ in 0..100_000 {
        let mut allocator = Allocator::new();
        let node = generator.condition(&mut allocator);
        let result = Condition::<NodePtr>::from_clvm(&allocator, node);

        // Every condition that parses must serialize back to exactly the same value.
        if let Ok(condition) = &result {
            let serialized = condition.to_clvm(&mut allocator).unwrap();
            assert_eq!(
                tree_hash(&allocator, serialized),
                tree_hash(&allocator, node),
                "{condition:?} doesn't round trip"
            );
        }

        // Layer specific conditions aren't seen by consensus, so they follow the layer's rules instead.
        if is_layer_opcode(&allocator, node) {
            *outcomes
                .entry(if result.is_ok() {
                    "layer"
                } else {
                    "invalid layer"
                })
                .or_default() += 1;
            continue;
        }

        let outcome = match (consensus_verdict(&allocator, node), &result) {
            // A trivially true timelock has no effect in consensus.
            (
                Verdict::Accepted(
                    ConsensusCondition::Skip | ConsensusCondition::SkipRelativeCondition,
                ),
                Ok(Condition::TriviallyTrue(_)),
            ) => "trivially true",
            // A remark is also skipped by consensus, but has its own variant.
            (Verdict::Accepted(ConsensusCondition::Skip), Ok(Condition::Remark(_))) => "remark",
            // Two byte opcodes are reserved for soft forks, and only have a cost.
            (Verdict::Accepted(ConsensusCondition::Softfork(_)), Ok(Condition::Other(_)))
                if is_two_byte_opcode(&allocator, node) =>
            {
                "soft fork"
            }
            (Verdict::Accepted(_), Ok(condition)) => {
                assert!(
                    !matches!(condition, Condition::TriviallyTrue(_) | Condition::Other(_)),
                    "{condition:?} should have been parsed as a known condition"
                );
                "valid"
            }
            // Unknown opcodes are ignored by consensus.
            (Verdict::Ignored, Ok(Condition::Other(_))) => "unknown",
            // Public keys are validated when they're parsed, rather than when the signature is checked.
            (Verdict::Accepted(_), Err(_)) if is_agg_sig_with_invalid_key(&allocator, node) => {
                "invalid key"
            }
            // A -113 output is intercepted by the CAT or singleton layer, so consensus never sees it.
            (Verdict::Rejected, Ok(condition)) if condition.is_magic_create_coin() => "magic",
            (Verdict::Rejected, Err(_)) => "invalid",
            (verdict, result) => {
                let opcode = allocator.next(node).map(|(opcode, _)| opcode);
                panic!(
                    "consensus result {verdict:?} doesn't match {result:?} for opcode {:?}",
                    opcode.map(|opcode| allocator.sexp(opcode))
                );
            }
        };

        *outcomes.entry(outcome).or_default() += 1;
    }

    // Make sure that the generator covers every kind of outcome.
    for outcome in [
        "trivially true",
        "remark",
        "soft fork",
        "valid",
        "unknown",
        "invalid key",
        "magic",
        "invalid",
        "layer",
        "invalid layer",
    ] {
        assert!(
            outcomes.get(outcome).copied().unwrap_or_default() >= 20,
            "not enough {outcome} conditions were generated: {outcomes:?}"
        );
    }
}
