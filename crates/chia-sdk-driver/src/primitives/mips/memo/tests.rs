use chia_consensus::opcodes::CREATE_COIN;
use chia_protocol::Bytes32;
use chia_sdk_test::{BlsPair, K1Pair, R1Pair};
use chia_sdk_types::puzzles::{EnforceDelegatedPuzzleWrappers, PreventConditionOpcode, Timelock};
use clvm_traits::{clvm_list, clvm_quote};
use clvm_utils::tree_hash;
use clvmr::serde::{
    node_from_bytes, node_from_bytes_backrefs, node_to_bytes, node_to_bytes_backrefs,
};
use rstest::rstest;
use std::time::{Duration, Instant};

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
    assert_eq!(MipsMemo::from_clvm_bounded(allocator, node)?, parsed);

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

/// Restrictions for the new right side of a 1 of 2, with more than one in each list so that the
/// order within each list is checked.
fn right_side_restrictions(
    allocator: &mut Allocator,
    reveal: bool,
) -> anyhow::Result<Vec<RestrictionMemo>> {
    let wrappers = [WrapperMemo::prevent_multiple_create_coins()];

    Ok(vec![
        RestrictionMemo::timelock(allocator, 100, reveal)?,
        RestrictionMemo::enforce_delegated_puzzle_wrappers(allocator, &wrappers)?,
        RestrictionMemo::timelock(allocator, 200, reveal)?,
        RestrictionMemo::enforce_delegated_puzzle_wrappers(allocator, &[])?,
    ])
}

fn force_1_of_2(
    allocator: &mut Allocator,
    left_side_subtree_hash: Bytes32,
    nonce: usize,
    reveal: bool,
) -> anyhow::Result<(
    WrapperMemo,
    Force1of2RestrictedVariable,
    Vec<RestrictionMemo>,
)> {
    let restrictions = right_side_restrictions(allocator, reveal)?;
    let restriction = WrapperMemo::force_1_of_2_restricted_variable(
        allocator,
        left_side_subtree_hash,
        nonce,
        restrictions.clone(),
    )?;

    let expected = Force1of2RestrictedVariable::new(
        left_side_subtree_hash,
        nonce,
        vec![
            Timelock::new(100).curry_tree_hash(),
            Timelock::new(200).curry_tree_hash(),
        ]
        .tree_hash()
        .into(),
        vec![
            EnforceDelegatedPuzzleWrappers::new(&[PREVENT_MULTIPLE_CREATE_COINS_HASH.into()])
                .curry_tree_hash(),
            EnforceDelegatedPuzzleWrappers::new(&[]).curry_tree_hash(),
        ]
        .tree_hash()
        .into(),
    );

    Ok((restriction, expected, restrictions))
}

#[rstest]
fn test_force_1_of_2_restriction(#[values(true, false)] reveal: bool) -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let left = Bytes32::new([1; 32]);
    let (wrapper, expected, restrictions) = force_1_of_2(&mut allocator, left, 5, reveal)?;
    assert_eq!(wrapper.puzzle_hash, expected.curry_tree_hash().into());

    let ctx = MipsMemoContext::default();

    assert_eq!(
        wrapper.parse(&allocator, &ctx),
        Some(ParsedWrapper::Force1of2RestrictedVariable(
            expected,
            restrictions.clone()
        ))
    );

    // It inspects conditions, so it also works as a member condition validator.
    let restriction = RestrictionMemo::new(true, wrapper.puzzle_hash, wrapper.memo);

    let Some(ParsedRestriction::Force1of2RestrictedVariable(parsed, parsed_restrictions)) =
        restriction.parse(&allocator, &ctx)
    else {
        panic!("expected force 1 of 2");
    };
    assert_eq!(parsed, expected);
    assert_eq!(parsed_restrictions, restrictions);

    // But it can't validate a delegated puzzle hash.
    let delegated_puzzle_validator = RestrictionMemo::new(false, wrapper.puzzle_hash, wrapper.memo);
    assert_eq!(delegated_puzzle_validator.parse(&allocator, &ctx), None);

    // The right side's restrictions are parsed the same way as any other restriction, so hidden
    // timelocks need to be in the context.
    let mut timelock_ctx = MipsMemoContext::default();
    timelock_ctx.timelocks.extend([100, 200]);

    for (i, restriction) in parsed_restrictions.iter().enumerate() {
        let parsed = restriction.parse(&allocator, &ctx);

        match i {
            0 | 2 => {
                let expected = Timelock::new(if i == 0 { 100 } else { 200 });
                if reveal {
                    assert_eq!(parsed, Some(ParsedRestriction::Timelock(expected)));
                } else {
                    assert_eq!(parsed, None);
                    assert_eq!(
                        restriction.parse(&allocator, &timelock_ctx),
                        Some(ParsedRestriction::Timelock(expected))
                    );
                }
            }
            _ => assert!(matches!(
                parsed,
                Some(ParsedRestriction::EnforceDelegatedPuzzleWrappers(..))
            )),
        }
    }

    // A restriction whose memo doesn't match its puzzle hash is left unknown.
    let mismatched = RestrictionMemo::new(true, Bytes32::new([2; 32]), restriction.memo);
    assert_eq!(mismatched.parse(&allocator, &ctx), None);

    Ok(())
}

#[test]
fn test_enforce_delegated_puzzle_wrappers() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let (force_1_of_2, force_1_of_2_expected, force_1_of_2_restrictions) =
        force_1_of_2(&mut allocator, Bytes32::new([1; 32]), 0, true)?;

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
                ParsedWrapper::Force1of2RestrictedVariable(
                    force_1_of_2_expected,
                    force_1_of_2_restrictions.clone(),
                ),
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
        // The stack Cloud Wallet uses for recovery, plus a timelock.
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
                ParsedWrapper::Force1of2RestrictedVariable(
                    force_1_of_2_expected,
                    force_1_of_2_restrictions,
                ),
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

    let Some(ParsedRestriction::EnforceDelegatedPuzzleWrappers(parsed, parsed_wrappers)) =
        restriction.parse(allocator, &MipsMemoContext::default())
    else {
        panic!("expected enforce delegated puzzle wrappers");
    };

    assert_eq!(parsed, EnforceDelegatedPuzzleWrappers::new(&wrapper_hashes));
    assert_eq!(parsed_wrappers, wrappers);

    for (wrapper, expected) in parsed_wrappers.iter().zip(expected) {
        assert_eq!(
            wrapper
                .parse(allocator, &MipsMemoContext::default())
                .as_ref(),
            Some(expected)
        );
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

#[test]
fn test_restrictions_hash() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let r1 = R1Pair::default().pk;
    let member = MemberMemo::r1(&mut allocator, r1, true, true)?;

    let (force_1_of_2, _, _) = force_1_of_2(&mut allocator, Bytes32::new([4; 32]), 1, true)?;
    let restrictions = vec![
        RestrictionMemo::timelock(&mut allocator, 1000, true)?,
        RestrictionMemo::new(true, force_1_of_2.puzzle_hash, force_1_of_2.memo),
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

/// Builds an M of N memo whose items all point to the same node, without expanding it.
fn shared_m_of_n(
    allocator: &mut Allocator,
    required: usize,
    items: Vec<NodePtr>,
) -> anyhow::Result<NodePtr> {
    Ok(ShallowInnerPuzzleMemo {
        _nonce: 0,
        restrictions: vec![],
        kind: ShallowMemoKind::MofN(ShallowMofNMemo {
            _required: required,
            items,
        }),
    }
    .to_clvm(allocator)?)
}

fn leaf_memo(allocator: &mut Allocator) -> anyhow::Result<InnerPuzzleMemo> {
    Ok(InnerPuzzleMemo::new(
        0,
        vec![],
        MemoKind::Member(MemberMemo::k1(
            allocator,
            K1Pair::default().pk,
            false,
            true,
        )?),
    ))
}

#[test]
fn test_deeply_nested_memo_hashes_quickly() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    // Each level is a 2 of 2 between the next level and a leaf. Hashing each child more than once
    // would double the cost at every level, which wouldn't finish at this depth.
    let leaf = leaf_memo(&mut allocator)?;
    let mut inner = InnerPuzzleMemo::new(1, vec![], leaf.kind.clone());

    for nonce in 2..=64 {
        inner = InnerPuzzleMemo::new(
            nonce,
            vec![],
            MemoKind::MofN(MofNMemo::new(2, vec![inner, leaf.clone()])),
        );
    }

    let memo = MipsMemo::new(inner);

    let start = Instant::now();
    let hash = memo.inner_puzzle_hash();
    assert_eq!(memo.try_inner_puzzle_hash()?, hash);
    assert!(memo.verify(hash));
    assert!(!memo.verify(TreeHash::new([0; 32])));
    assert!(start.elapsed() < Duration::from_secs(5));

    Ok(())
}

#[test]
fn test_bounded_parse_rejects_deep_memo() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let leaf = leaf_memo(&mut allocator)?.to_clvm(&mut allocator)?;
    let mut node = leaf;

    // Deep enough that recursive decoding could overflow the stack.
    for _ in 0..100_000 {
        node = shared_m_of_n(&mut allocator, 1, vec![node])?;
    }

    let memo = clvm_list!("CHIP-0043", node).to_clvm(&mut allocator)?;

    let start = Instant::now();
    assert!(matches!(
        MipsMemo::from_clvm_bounded(&allocator, memo),
        Err(DriverError::MipsMemoTooDeep)
    ));
    assert!(start.elapsed() < Duration::from_secs(5));

    // Right at the limit is fine.
    let mut node = leaf;
    for _ in 0..MAX_MIPS_MEMO_DEPTH {
        node = shared_m_of_n(&mut allocator, 1, vec![node])?;
    }
    let memo = clvm_list!("CHIP-0043", node).to_clvm(&mut allocator)?;
    MipsMemo::from_clvm_bounded(&allocator, memo)?;

    Ok(())
}

#[test]
fn test_bounded_parse_rejects_shared_subtrees() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    // A few hundred bytes of CLVM which expands to 2^30 inner puzzles when decoded naively.
    let mut node = leaf_memo(&mut allocator)?.to_clvm(&mut allocator)?;
    for _ in 0..30 {
        node = shared_m_of_n(&mut allocator, 1, vec![node, node])?;
    }

    let memo = clvm_list!("CHIP-0043", node).to_clvm(&mut allocator)?;
    let bytes = node_to_bytes_backrefs(&allocator, memo)?;
    assert!(bytes.len() < 1000);
    let memo = node_from_bytes_backrefs(&mut allocator, &bytes)?;

    let start = Instant::now();
    assert!(matches!(
        MipsMemo::from_clvm_bounded(&allocator, memo),
        Err(DriverError::MipsMemoTooLarge)
    ));
    assert!(start.elapsed() < Duration::from_secs(5));

    Ok(())
}

#[test]
fn test_custom_member_with_shared_subtrees() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let mut puzzle = NodePtr::NIL;
    for _ in 0..64 {
        puzzle = allocator.new_pair(puzzle, puzzle)?;
    }

    let start = Instant::now();
    let puzzle_hash = tree_hash_cached(&allocator, puzzle, &mut TreeCache::default());
    let member = MemberMemo::new(puzzle_hash.into(), puzzle);
    assert_eq!(
        member.parse(&allocator, &MipsMemoContext::default()),
        Some(ParsedMember::Custom(puzzle))
    );
    assert!(start.elapsed() < Duration::from_secs(5));

    Ok(())
}

#[rstest]
#[case::zero_of_two(0, 2)]
#[case::three_of_two(3, 2)]
#[case::one_of_zero(1, 0)]
#[case::zero_of_zero(0, 0)]
fn test_invalid_m_of_n_required(
    #[case] required: usize,
    #[case] count: usize,
) -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    let items = k1_items(&mut allocator, count)?;
    let memo = MipsMemo::new(InnerPuzzleMemo::new(
        0,
        vec![],
        MemoKind::MofN(MofNMemo::new(required, items)),
    ));

    assert!(matches!(
        memo.try_inner_puzzle_hash(),
        Err(DriverError::InvalidMofNRequired { .. })
    ));
    assert!(!memo.verify(memo.inner_puzzle_hash()));

    let node = memo.to_clvm(&mut allocator)?;
    assert!(MipsMemo::<NodePtr>::from_clvm(&allocator, node).is_ok());
    assert!(MipsMemo::from_clvm_bounded(&allocator, node).is_err());

    Ok(())
}

#[test]
fn test_duplicate_m_of_n_items() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();

    // A 2 of 3 where the same key appears twice can be spent by that key alone.
    let mut items = k1_items(&mut allocator, 2)?;
    items.push(items[0].clone());

    let memo = MipsMemo::new(InnerPuzzleMemo::new(
        0,
        vec![],
        MemoKind::MofN(MofNMemo::new(2, items)),
    ));

    assert!(matches!(
        memo.try_inner_puzzle_hash(),
        Err(DriverError::DuplicateMofNMembers)
    ));

    let node = memo.to_clvm(&mut allocator)?;
    assert!(MipsMemo::from_clvm_bounded(&allocator, node).is_err());

    Ok(())
}

#[test]
fn test_revealed_zero_timelock() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();
    let ctx = MipsMemoContext::default();

    let restriction = RestrictionMemo::timelock(&mut allocator, 0, true)?;
    assert_eq!(
        restriction.parse(&allocator, &ctx),
        Some(ParsedRestriction::Timelock(Timelock::new(0)))
    );

    let wrapper = WrapperMemo::timelock(&mut allocator, 0, true)?;
    assert_eq!(
        wrapper.parse(&allocator, &ctx),
        Some(ParsedWrapper::Timelock(Timelock::new(0)))
    );

    Ok(())
}

#[test]
fn test_restriction_kind_must_match() -> anyhow::Result<()> {
    let mut allocator = Allocator::new();
    let ctx = MipsMemoContext::default();

    // A timelock inspects conditions, so it can't validate the delegated puzzle hash.
    let timelock = RestrictionMemo::timelock(&mut allocator, 100, true)?;
    let flipped = RestrictionMemo::new(false, timelock.puzzle_hash, timelock.memo);
    assert_eq!(flipped.parse(&allocator, &ctx), None);

    // The wrapper enforcer validates the delegated puzzle hash, so it can't inspect conditions.
    let enforce = RestrictionMemo::enforce_delegated_puzzle_wrappers(
        &mut allocator,
        &[WrapperMemo::prevent_multiple_create_coins()],
    )?;
    assert!(enforce.parse(&allocator, &ctx).is_some());
    let flipped = RestrictionMemo::new(true, enforce.puzzle_hash, enforce.memo);
    assert_eq!(flipped.parse(&allocator, &ctx), None);

    Ok(())
}
