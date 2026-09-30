use chia_consensus::opcodes::CREATE_COIN;
use chia_protocol::Bytes32;
use chia_sdk_test::{BlsPair, K1Pair, R1Pair};
use chia_sdk_types::puzzles::{EnforceDelegatedPuzzleWrappers, PreventConditionOpcode, Timelock};
use clvm_traits::{clvm_list, clvm_quote};
use clvmr::serde::{node_from_bytes, node_to_bytes};
use rstest::rstest;

use crate::{MofN, Restriction, RestrictionKind, mips_puzzle_hash};

use super::*;

const LAUNCHER_ID: Bytes32 = Bytes32::new([7; 32]);
const FIXED_PUZZLE_HASH: Bytes32 = Bytes32::new([9; 32]);

#[derive(Debug, Clone, Copy)]
enum MemberCase {
    K1,
    K1FastForward,
    R1,
    R1FastForward,
    Bls,
    BlsFastForward,
    BlsTaproot,
    BlsTaprootFastForward,
    Passkey,
    PasskeyFastForward,
    Singleton,
    SingletonFastForward,
    FixedPuzzle,
}

/// Returns the member memo, the member it's expected to parse as, and a context which contains
/// the information needed to resolve the member if it isn't revealed.
fn member_case(
    allocator: &mut Allocator,
    case: MemberCase,
    reveal: bool,
) -> anyhow::Result<(MemberMemo, ParsedMember, MipsMemoContext)> {
    let k1 = K1Pair::default().pk;
    let r1 = R1Pair::default().pk;
    let bls = BlsPair::default().pk;

    let mut ctx = MipsMemoContext::default();

    let (memo, expected) = match case {
        MemberCase::K1 | MemberCase::K1FastForward => {
            let fast_forward = matches!(case, MemberCase::K1FastForward);
            ctx.k1.push(k1);
            (
                MemberMemo::k1(allocator, k1, fast_forward, reveal)?,
                if fast_forward {
                    ParsedMember::K1PuzzleAssert(K1MemberPuzzleAssert::new(k1))
                } else {
                    ParsedMember::K1(K1Member::new(k1))
                },
            )
        }
        MemberCase::R1 | MemberCase::R1FastForward => {
            let fast_forward = matches!(case, MemberCase::R1FastForward);
            ctx.r1.push(r1);
            (
                MemberMemo::r1(allocator, r1, fast_forward, reveal)?,
                if fast_forward {
                    ParsedMember::R1PuzzleAssert(R1MemberPuzzleAssert::new(r1))
                } else {
                    ParsedMember::R1(R1Member::new(r1))
                },
            )
        }
        MemberCase::Bls | MemberCase::BlsFastForward => {
            let fast_forward = matches!(case, MemberCase::BlsFastForward);
            ctx.bls.push(bls);
            (
                MemberMemo::bls(allocator, bls, fast_forward, false, reveal)?,
                if fast_forward {
                    ParsedMember::BlsPuzzleAssert(BlsMemberPuzzleAssert::new(bls))
                } else {
                    ParsedMember::Bls(BlsMember::new(bls))
                },
            )
        }
        MemberCase::BlsTaproot | MemberCase::BlsTaprootFastForward => {
            let fast_forward = matches!(case, MemberCase::BlsTaprootFastForward);
            ctx.bls.push(bls);
            (
                MemberMemo::bls(allocator, bls, fast_forward, true, reveal)?,
                if fast_forward {
                    ParsedMember::BlsTaprootPuzzleAssert(BlsTaprootMemberPuzzleAssert::new(bls))
                } else {
                    ParsedMember::BlsTaproot(BlsTaprootMember::new(bls))
                },
            )
        }
        MemberCase::Passkey | MemberCase::PasskeyFastForward => {
            let fast_forward = matches!(case, MemberCase::PasskeyFastForward);
            ctx.r1.push(r1);
            (
                MemberMemo::passkey(allocator, r1, fast_forward, reveal)?,
                if fast_forward {
                    ParsedMember::PasskeyPuzzleAssert(PasskeyMemberPuzzleAssert::new(r1))
                } else {
                    ParsedMember::Passkey(PasskeyMember::new(r1))
                },
            )
        }
        MemberCase::Singleton | MemberCase::SingletonFastForward => {
            let fast_forward = matches!(case, MemberCase::SingletonFastForward);
            ctx.hashes.push(LAUNCHER_ID);
            (
                MemberMemo::singleton(allocator, LAUNCHER_ID, fast_forward, reveal)?,
                if fast_forward {
                    ParsedMember::SingletonWithMode(SingletonMemberWithMode::new(
                        LAUNCHER_ID,
                        0b010_010,
                    ))
                } else {
                    ParsedMember::Singleton(SingletonMember::new(LAUNCHER_ID))
                },
            )
        }
        MemberCase::FixedPuzzle => {
            ctx.hashes.push(FIXED_PUZZLE_HASH);
            (
                MemberMemo::fixed_puzzle(allocator, FIXED_PUZZLE_HASH, reveal)?,
                ParsedMember::FixedPuzzle(FixedPuzzleMember::new(FIXED_PUZZLE_HASH)),
            )
        }
    };

    Ok((memo, expected, ctx))
}

fn member_memo(member: MemberMemo) -> MipsMemo {
    MipsMemo::new(InnerPuzzleMemo::new(0, vec![], MemoKind::Member(member)))
}

/// Serializes and deserializes the memo, then decodes it.
fn serialized_roundtrip(allocator: &mut Allocator, memo: &MipsMemo) -> anyhow::Result<MipsMemo> {
    let node = memo.to_clvm(allocator)?;
    let bytes = node_to_bytes(allocator, node)?;
    let node = node_from_bytes(allocator, &bytes)?;
    let parsed = MipsMemo::<NodePtr>::from_clvm(&*allocator, node)?;

    let reencoded = parsed.to_clvm(allocator)?;
    assert_eq!(node_to_bytes(allocator, reencoded)?, bytes);

    Ok(parsed)
}

fn to_restriction(memo: &RestrictionMemo) -> Restriction {
    Restriction {
        kind: if memo.member_condition_validator {
            RestrictionKind::MemberCondition
        } else {
            RestrictionKind::DelegatedPuzzleHash
        },
        puzzle_hash: memo.puzzle_hash.into(),
    }
}

#[rstest]
fn test_member_roundtrip(
    #[values(
        MemberCase::K1,
        MemberCase::K1FastForward,
        MemberCase::R1,
        MemberCase::R1FastForward,
        MemberCase::Bls,
        MemberCase::BlsFastForward,
        MemberCase::BlsTaproot,
        MemberCase::BlsTaprootFastForward,
        MemberCase::Passkey,
        MemberCase::PasskeyFastForward,
        MemberCase::Singleton,
        MemberCase::SingletonFastForward,
        MemberCase::FixedPuzzle
    )]
    case: MemberCase,
    #[values(true, false)] reveal: bool,
) -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let (member, expected, ctx) = member_case(&mut allocator, case, reveal)?;
    let memo = member_memo(member.clone());

    let node = memo.to_clvm(&mut allocator)?;
    let parsed = MipsMemo::<NodePtr>::from_clvm(&allocator, node)?;
    assert_eq!(parsed, memo);

    let serialized = serialized_roundtrip(&mut allocator, &memo)?;
    assert_eq!(serialized.inner_puzzle_hash(), memo.inner_puzzle_hash());

    assert_eq!(
        parsed.inner_puzzle_hash(),
        mips_puzzle_hash(0, vec![], member.puzzle_hash.into(), true)
    );

    let MemoKind::Member(parsed_member) = parsed.inner_puzzle.kind else {
        panic!("expected member");
    };

    let default_ctx = MipsMemoContext::default();

    if reveal {
        assert_eq!(
            parsed_member.parse(&allocator, &default_ctx),
            Some(expected)
        );
    } else {
        assert_eq!(parsed_member.memo, NodePtr::NIL);
        assert_eq!(parsed_member.parse(&allocator, &default_ctx), None);
        assert_eq!(parsed_member.parse(&allocator, &ctx), Some(expected));
    }

    Ok(())
}

#[test]
fn test_custom_member() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let puzzle = clvm_quote!(clvm_list!(clvm_list!(CREATE_COIN, Bytes32::default(), 1)))
        .to_clvm(&mut allocator)?;
    let puzzle_hash = tree_hash(&allocator, puzzle);

    let memo = member_memo(MemberMemo::new(puzzle_hash.into(), puzzle));
    let parsed = serialized_roundtrip(&mut allocator, &memo)?;
    assert_eq!(
        parsed.inner_puzzle_hash(),
        mips_puzzle_hash(0, vec![], puzzle_hash, true)
    );

    let MemoKind::Member(member) = parsed.inner_puzzle.kind else {
        panic!("expected member");
    };

    let Some(ParsedMember::Custom(parsed_puzzle)) =
        member.parse(&allocator, &MipsMemoContext::default())
    else {
        panic!("expected custom member");
    };
    assert_eq!(tree_hash(&allocator, parsed_puzzle), puzzle_hash);

    // A puzzle reveal that doesn't match the puzzle hash is left unknown, but preserved.
    let unknown = MemberMemo::new(Bytes32::new([1; 32]), puzzle);
    assert_eq!(unknown.parse(&allocator, &MipsMemoContext::default()), None);

    let memo = member_memo(unknown);
    let parsed = serialized_roundtrip(&mut allocator, &memo)?;
    let MemoKind::Member(member) = parsed.inner_puzzle.kind else {
        panic!("expected member");
    };
    assert_eq!(member.puzzle_hash, Bytes32::new([1; 32]));
    assert_eq!(tree_hash(&allocator, member.memo), puzzle_hash);

    Ok(())
}

#[rstest]
fn test_timelock_restriction(#[values(true, false)] reveal: bool) -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let restriction = RestrictionMemo::timelock(&mut allocator, 100, reveal)?;
    assert!(restriction.member_condition_validator);

    let expected = Some(ParsedRestriction::Timelock(Timelock::new(100)));

    if reveal {
        assert_eq!(
            restriction.parse(&allocator, &MipsMemoContext::default()),
            expected
        );
    } else {
        assert_eq!(
            restriction.parse(&allocator, &MipsMemoContext::default()),
            None
        );

        let mut ctx = MipsMemoContext::default();
        ctx.timelocks.push(100);
        assert_eq!(restriction.parse(&allocator, &ctx), expected);
    }

    Ok(())
}

#[test]
fn test_force_1_of_2_restriction() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let left = Bytes32::new([1; 32]);
    let member_validators = Bytes32::new([2; 32]);
    let delegated_puzzle_validators = Bytes32::new([3; 32]);

    let restriction = RestrictionMemo::force_1_of_2_restricted_variable(
        &mut allocator,
        left,
        5,
        member_validators,
        delegated_puzzle_validators,
    )?;

    let expected =
        Force1of2RestrictedVariable::new(left, 5, member_validators, delegated_puzzle_validators);

    assert_eq!(
        restriction.parse(&allocator, &MipsMemoContext::default()),
        Some(ParsedRestriction::Force1of2RestrictedVariable(expected))
    );

    let wrapper = WrapperMemo::new(restriction.puzzle_hash, restriction.memo);
    assert_eq!(
        wrapper.parse(&allocator, &MipsMemoContext::default()),
        Some(ParsedWrapper::Force1of2RestrictedVariable(expected))
    );

    Ok(())
}

#[test]
fn test_enforce_delegated_puzzle_wrappers() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let force_1_of_2 = RestrictionMemo::force_1_of_2_restricted_variable(
        &mut allocator,
        Bytes32::new([1; 32]),
        0,
        Bytes32::new([2; 32]),
        Bytes32::new([3; 32]),
    )?;

    let groups = vec![
        (
            vec![
                WrapperMemo::new(force_1_of_2.puzzle_hash, force_1_of_2.memo),
                WrapperMemo::force_assert_coin_announcement(),
                WrapperMemo::force_coin_message(),
                WrapperMemo::force_singleton_recreation(),
                WrapperMemo::prevent_multiple_create_coins(),
            ],
            vec![
                ParsedWrapper::Force1of2RestrictedVariable(Force1of2RestrictedVariable::new(
                    Bytes32::new([1; 32]),
                    0,
                    Bytes32::new([2; 32]),
                    Bytes32::new([3; 32]),
                )),
                ParsedWrapper::ForceAssertCoinAnnouncement,
                ParsedWrapper::ForceCoinMessage,
                ParsedWrapper::ForceSingletonRecreation,
                ParsedWrapper::PreventMultipleCreateCoins,
            ],
        ),
        (
            vec![
                WrapperMemo::timelock(&mut allocator, 50, true)?,
                WrapperMemo::prevent_condition_opcode(&mut allocator, SEND_MESSAGE, true)?,
                WrapperMemo::prevent_condition_opcode(&mut allocator, RECEIVE_MESSAGE, false)?,
            ],
            vec![
                ParsedWrapper::Timelock(Timelock::new(50)),
                ParsedWrapper::PreventConditionOpcode(PreventConditionOpcode::new(SEND_MESSAGE)),
                // Hidden, but resolved because it's one of the default opcodes.
                ParsedWrapper::PreventConditionOpcode(PreventConditionOpcode::new(RECEIVE_MESSAGE)),
            ],
        ),
        // The stack Cloud Wallet uses for recovery, plus a timelock. In the legacy format,
        // revealed values only have a couple of candidates each, so this stays within the
        // search limit.
        (
            vec![
                WrapperMemo::new(force_1_of_2.puzzle_hash, force_1_of_2.memo),
                WrapperMemo::prevent_condition_opcode(
                    &mut allocator,
                    CREATE_COIN_ANNOUNCEMENT,
                    true,
                )?,
                WrapperMemo::prevent_condition_opcode(
                    &mut allocator,
                    CREATE_PUZZLE_ANNOUNCEMENT,
                    true,
                )?,
                WrapperMemo::prevent_condition_opcode(&mut allocator, SEND_MESSAGE, true)?,
                WrapperMemo::prevent_condition_opcode(&mut allocator, RECEIVE_MESSAGE, true)?,
                WrapperMemo::prevent_multiple_create_coins(),
                WrapperMemo::force_singleton_recreation(),
                WrapperMemo::timelock(&mut allocator, 50, true)?,
            ],
            vec![
                ParsedWrapper::Force1of2RestrictedVariable(Force1of2RestrictedVariable::new(
                    Bytes32::new([1; 32]),
                    0,
                    Bytes32::new([2; 32]),
                    Bytes32::new([3; 32]),
                )),
                ParsedWrapper::PreventConditionOpcode(PreventConditionOpcode::new(
                    CREATE_COIN_ANNOUNCEMENT,
                )),
                ParsedWrapper::PreventConditionOpcode(PreventConditionOpcode::new(
                    CREATE_PUZZLE_ANNOUNCEMENT,
                )),
                ParsedWrapper::PreventConditionOpcode(PreventConditionOpcode::new(SEND_MESSAGE)),
                ParsedWrapper::PreventConditionOpcode(PreventConditionOpcode::new(RECEIVE_MESSAGE)),
                ParsedWrapper::PreventMultipleCreateCoins,
                ParsedWrapper::ForceSingletonRecreation,
                ParsedWrapper::Timelock(Timelock::new(50)),
            ],
        ),
    ];

    for (wrappers, expected) in groups {
        check_delegated_puzzle_wrappers(&mut allocator, &wrappers, &expected)?;
    }

    // An unrecognized opcode can't be resolved without adding it to the context.
    let hidden = WrapperMemo::prevent_condition_opcode(&mut allocator, CREATE_COIN, false)?;
    assert_eq!(hidden.parse(&allocator, &MipsMemoContext::default()), None);

    let mut ctx = MipsMemoContext::default();
    ctx.opcodes.push(CREATE_COIN);
    assert_eq!(
        hidden.parse(&allocator, &ctx),
        Some(ParsedWrapper::PreventConditionOpcode(
            PreventConditionOpcode::new(CREATE_COIN)
        ))
    );

    Ok(())
}

fn check_delegated_puzzle_wrappers(
    allocator: &mut Allocator,
    wrappers: &[WrapperMemo],
    expected: &[ParsedWrapper],
) -> anyhow::Result<()> {
    let restriction = RestrictionMemo::enforce_delegated_puzzle_wrappers(allocator, wrappers)?;
    assert!(!restriction.member_condition_validator);

    let wrapper_hashes: Vec<TreeHash> = wrappers
        .iter()
        .map(|wrapper| wrapper.puzzle_hash.into())
        .collect();

    // Both the current format and the legacy format, which only has the memo of each wrapper.
    let legacy = legacy_delegated_puzzle_wrappers(allocator, wrappers)?;
    assert_eq!(legacy.puzzle_hash, restriction.puzzle_hash);

    for restriction in [&restriction, &legacy] {
        let Some(ParsedRestriction::EnforceDelegatedPuzzleWrappers(parsed, parsed_wrappers)) =
            restriction.parse(allocator, &MipsMemoContext::default())
        else {
            panic!("expected enforce delegated puzzle wrappers");
        };

        assert_eq!(parsed, EnforceDelegatedPuzzleWrappers::new(&wrapper_hashes));
        assert_eq!(parsed_wrappers, wrappers);

        for (wrapper, expected) in parsed_wrappers.iter().zip(expected) {
            assert_eq!(
                wrapper.parse(allocator, &MipsMemoContext::default()),
                Some(*expected)
            );
        }
    }

    // The memo hash matches the construction path, where each wrapper is a separate restriction.
    let k1 = K1Pair::default().pk;
    let member = MemberMemo::k1(allocator, k1, false, true)?;
    let memo = MipsMemo::new(InnerPuzzleMemo::new(
        3,
        vec![restriction],
        MemoKind::Member(member),
    ));
    let parsed = serialized_roundtrip(allocator, &memo)?;

    let restrictions = wrapper_hashes
        .iter()
        .map(|&puzzle_hash| Restriction {
            kind: RestrictionKind::DelegatedPuzzleWrapper,
            puzzle_hash,
        })
        .collect();

    assert_eq!(
        parsed.inner_puzzle_hash(),
        mips_puzzle_hash(3, restrictions, K1Member::new(k1).curry_tree_hash(), true)
    );

    Ok(())
}

/// Builds the restriction the way it was serialized before wrapper puzzle hashes were included.
pub(super) fn legacy_delegated_puzzle_wrappers(
    allocator: &mut Allocator,
    wrappers: &[WrapperMemo],
) -> anyhow::Result<RestrictionMemo> {
    let restriction = RestrictionMemo::enforce_delegated_puzzle_wrappers(allocator, wrappers)?;
    let memos: Vec<NodePtr> = wrappers.iter().map(|wrapper| wrapper.memo).collect();
    Ok(RestrictionMemo::new(
        false,
        restriction.puzzle_hash,
        memos.to_clvm(allocator)?,
    ))
}

#[test]
fn test_enforce_delegated_puzzle_wrappers_search_limit() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    // A wrapper with an empty memo has 8 candidates by default (4 wrappers without curried
    // arguments, and 4 hidden condition opcodes), so 4 of them fit within the search limit of
    // 2^12 combinations, but 5 of them don't. The current format doesn't need to search.
    for (count, resolves) in [(4, true), (5, false)] {
        let wrappers = vec![WrapperMemo::prevent_multiple_create_coins(); count];
        let legacy = legacy_delegated_puzzle_wrappers(&mut allocator, &wrappers)?;
        assert_eq!(
            legacy
                .parse(&allocator, &MipsMemoContext::default())
                .is_some(),
            resolves
        );

        let restriction =
            RestrictionMemo::enforce_delegated_puzzle_wrappers(&mut allocator, &wrappers)?;
        assert!(
            restriction
                .parse(&allocator, &MipsMemoContext::default())
                .is_some()
        );
    }

    Ok(())
}

#[test]
fn test_restrictions_hash() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let r1 = R1Pair::default().pk;
    let member = MemberMemo::r1(&mut allocator, r1, true, true)?;

    let restrictions = vec![
        RestrictionMemo::timelock(&mut allocator, 1000, true)?,
        RestrictionMemo::force_1_of_2_restricted_variable(
            &mut allocator,
            Bytes32::new([4; 32]),
            1,
            Bytes32::new([5; 32]),
            Bytes32::new([6; 32]),
        )?,
    ];

    let memo = MipsMemo::new(InnerPuzzleMemo::new(
        7,
        restrictions.clone(),
        MemoKind::Member(member),
    ));
    let parsed = serialized_roundtrip(&mut allocator, &memo)?;

    assert_eq!(
        parsed.inner_puzzle_hash(),
        mips_puzzle_hash(
            7,
            restrictions.iter().map(to_restriction).collect(),
            R1MemberPuzzleAssert::new(r1).curry_tree_hash(),
            true,
        )
    );

    for restriction in &parsed.inner_puzzle.restrictions {
        assert!(
            restriction
                .parse(&allocator, &MipsMemoContext::default())
                .is_some()
        );
    }

    Ok(())
}

fn k1_items(allocator: &mut Allocator, count: usize) -> anyhow::Result<Vec<InnerPuzzleMemo>> {
    K1Pair::range_vec(count)
        .into_iter()
        .enumerate()
        .map(|(nonce, pair)| {
            Ok(InnerPuzzleMemo::new(
                nonce,
                vec![],
                MemoKind::Member(MemberMemo::k1(allocator, pair.pk, false, true)?),
            ))
        })
        .collect()
}

#[rstest]
#[case::one_of_one(1, 1)]
#[case::one_of_three(1, 3)]
#[case::two_of_two(2, 2)]
#[case::two_of_three(2, 3)]
#[case::two_of_four(2, 4)]
#[case::three_of_five(3, 5)]
#[case::four_of_four(4, 4)]
fn test_m_of_n_shapes(#[case] required: usize, #[case] count: usize) -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let items = k1_items(&mut allocator, count)?;
    let hashes = items
        .iter()
        .map(|item| item.inner_puzzle_hash(false))
        .collect::<Vec<_>>();

    let memo = MipsMemo::new(InnerPuzzleMemo::new(
        2,
        vec![],
        MemoKind::MofN(MofNMemo::new(required, items)),
    ));
    let parsed = serialized_roundtrip(&mut allocator, &memo)?;

    assert_eq!(
        parsed.inner_puzzle_hash(),
        mips_puzzle_hash(
            2,
            vec![],
            MofN::new(required, hashes).inner_puzzle_hash(),
            true
        )
    );

    let MemoKind::MofN(m_of_n) = &parsed.inner_puzzle.kind else {
        panic!("expected m of n");
    };
    assert_eq!(m_of_n.required, required);

    for item in &m_of_n.items {
        let MemoKind::Member(member) = &item.kind else {
            panic!("expected member");
        };
        assert!(matches!(
            member.parse(&allocator, &MipsMemoContext::default()),
            Some(ParsedMember::K1(_))
        ));
    }

    Ok(())
}

#[test]
fn test_nested_m_of_n() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let inner_items = k1_items(&mut allocator, 3)?;
    let inner_hashes = inner_items
        .iter()
        .map(|item| item.inner_puzzle_hash(false))
        .collect::<Vec<_>>();

    let timelock = RestrictionMemo::timelock(&mut allocator, 60, true)?;
    let nested = InnerPuzzleMemo::new(
        4,
        vec![timelock.clone()],
        MemoKind::MofN(MofNMemo::new(2, inner_items)),
    );
    let nested_hash = mips_puzzle_hash(
        4,
        vec![to_restriction(&timelock)],
        MofN::new(2, inner_hashes).inner_puzzle_hash(),
        false,
    );
    assert_eq!(nested.inner_puzzle_hash(false), nested_hash);

    let bls = BlsPair::default().pk;
    let member = InnerPuzzleMemo::new(
        0,
        vec![],
        MemoKind::Member(MemberMemo::bls(&mut allocator, bls, false, false, true)?),
    );
    let member_hash = mips_puzzle_hash(0, vec![], BlsMember::new(bls).curry_tree_hash(), false);

    let memo = MipsMemo::new(InnerPuzzleMemo::new(
        0,
        vec![],
        MemoKind::MofN(MofNMemo::new(1, vec![nested, member])),
    ));
    let parsed = serialized_roundtrip(&mut allocator, &memo)?;

    assert_eq!(
        parsed.inner_puzzle_hash(),
        mips_puzzle_hash(
            0,
            vec![],
            MofN::new(1, vec![nested_hash, member_hash]).inner_puzzle_hash(),
            true
        )
    );

    Ok(())
}

/// Builds a memo by hand, so that individual atoms can be made malformed. With the default values,
/// this is `(CHIP-0043 (() () 1 (1 ((() (restriction) () (puzzle_hash ()))))))`.
struct RawMemo {
    namespace: &'static str,
    member_condition_validator: Vec<u8>,
    puzzle_hash: Vec<u8>,
    missing_inner_puzzle: bool,
}

impl Default for RawMemo {
    fn default() -> Self {
        Self {
            namespace: "CHIP-0043",
            member_condition_validator: vec![1],
            puzzle_hash: vec![8; 32],
            missing_inner_puzzle: false,
        }
    }
}

impl RawMemo {
    fn build(&self, allocator: &mut Allocator) -> anyhow::Result<NodePtr> {
        let validator = allocator.new_atom(&self.member_condition_validator)?;
        let puzzle_hash = allocator.new_atom(&self.puzzle_hash)?;
        let timelock_hash = Bytes32::from(Timelock::new(1).curry_tree_hash());

        let restriction = clvm_list!(validator, timelock_hash, ()).to_clvm(allocator)?;
        let member_kind = clvm_list!((), clvm_list!(puzzle_hash, ())).to_clvm(allocator)?;
        let item = (0, (clvm_list!(restriction), member_kind)).to_clvm(allocator)?;

        let m_of_n_kind = clvm_list!(1, clvm_list!(1, clvm_list!(item))).to_clvm(allocator)?;
        let inner_puzzle = (0, ((), m_of_n_kind)).to_clvm(allocator)?;

        Ok(if self.missing_inner_puzzle {
            clvm_list!(self.namespace).to_clvm(allocator)?
        } else {
            clvm_list!(self.namespace, inner_puzzle).to_clvm(allocator)?
        })
    }
}

#[test]
fn test_raw_memo_is_valid() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let node = RawMemo::default().build(&mut allocator)?;
    let memo = MipsMemo::<NodePtr>::from_clvm(&allocator, node)?;

    let MemoKind::MofN(m_of_n) = &memo.inner_puzzle.kind else {
        panic!("expected m of n");
    };
    assert_eq!(m_of_n.required, 1);
    assert_eq!(m_of_n.items.len(), 1);
    assert_eq!(m_of_n.items[0].restrictions.len(), 1);

    Ok(())
}

#[rstest]
#[case::wrong_namespace(RawMemo { namespace: "CHIP-0044", ..Default::default() })]
#[case::missing_inner_puzzle(RawMemo { missing_inner_puzzle: true, ..Default::default() })]
#[case::invalid_bool(RawMemo { member_condition_validator: vec![2], ..Default::default() })]
#[case::short_puzzle_hash(RawMemo { puzzle_hash: vec![8; 31], ..Default::default() })]
#[case::long_puzzle_hash(RawMemo { puzzle_hash: vec![8; 33], ..Default::default() })]
fn test_malformed_memo(#[case] raw: RawMemo) -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let node = raw.build(&mut allocator)?;
    assert!(MipsMemo::<NodePtr>::from_clvm(&allocator, node).is_err());

    Ok(())
}

#[test]
fn test_malformed_shapes() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let atom = allocator.new_atom(b"CHIP-0043")?;
    assert!(MipsMemo::<NodePtr>::from_clvm(&allocator, atom).is_err());
    assert!(MipsMemo::<NodePtr>::from_clvm(&allocator, NodePtr::NIL).is_err());

    let pair_nonce = clvm_list!("CHIP-0043", ((1, 2), ((), ((), ())))).to_clvm(&mut allocator)?;
    assert!(MipsMemo::<NodePtr>::from_clvm(&allocator, pair_nonce).is_err());

    let unknown_kind =
        clvm_list!("CHIP-0043", (0, ((), clvm_list!(2, ())))).to_clvm(&mut allocator)?;
    assert!(MipsMemo::<NodePtr>::from_clvm(&allocator, unknown_kind).is_err());

    Ok(())
}
