//! These tests mirror how Cloud Wallet (`getVaultInternals` and `getVaultMipsMemoListBytes` in
//! ent-wallet) constructs vaults and their MIPS memos, to ensure that the memos it writes on chain
//! can be parsed back into the exact custody structure.

use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::Memos;
use chia_sdk_test::{BlsPair, K1Pair, R1Pair, Simulator};
use chia_sdk_types::{
    Condition, Conditions, Mod,
    puzzles::{
        BlsMember, K1MemberPuzzleAssert, K1MemberPuzzleAssertSolution, PasskeyMemberPuzzleAssert,
        R1MemberPuzzleAssert, SingletonMember, Timelock,
    },
};
use chia_secp::{K1PublicKey, R1PublicKey};
use chia_sha2::Sha256;
use clvm_traits::{FromClvm, ToClvm, clvm_list};
use clvm_utils::{ToTreeHash, TreeHash, tree_hash};
use clvmr::{
    Allocator, NodePtr,
    serde::{node_from_bytes, node_to_bytes},
};
use rstest::rstest;

use crate::{
    InnerPuzzleSpend, Launcher, MipsSpend, MofN, Restriction, RestrictionKind, Spend, SpendContext,
    StandardLayer, mips_puzzle_hash,
};

use super::{tests::legacy_delegated_puzzle_wrappers, *};

#[derive(Debug, Clone, Copy)]
enum Key {
    K1(K1PublicKey),
    R1(R1PublicKey),
    Passkey(R1PublicKey),
    Bls(PublicKey),
}

#[derive(Debug, Clone)]
struct Signers {
    keys: Vec<Key>,
    vault_launcher_ids: Vec<Bytes32>,
    threshold: usize,
}

impl Signers {
    fn new(keys: Vec<Key>, vault_launcher_ids: Vec<Bytes32>, threshold: usize) -> Self {
        Self {
            keys,
            vault_launcher_ids,
            threshold,
        }
    }

    fn count(&self) -> usize {
        self.keys.len() + self.vault_launcher_ids.len()
    }
}

#[derive(Debug, Clone)]
struct VaultKeys {
    custody: Signers,
    recovery: Signers,
    clawback_timelock: u64,
    force_singleton_recreation: bool,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Custody,
    Recovery {
        post_recovery_inner_puzzle_hash: Bytes32,
        amount: u64,
    },
}

/// The member puzzle hash of a key, as in `getMemberHash`. BLS keys never use fast forward.
fn key_puzzle_hash(key: Key, fast_forward: bool) -> TreeHash {
    match key {
        Key::K1(pk) if fast_forward => K1MemberPuzzleAssert::new(pk).curry_tree_hash(),
        Key::K1(pk) => K1Member::new(pk).curry_tree_hash(),
        Key::R1(pk) if fast_forward => R1MemberPuzzleAssert::new(pk).curry_tree_hash(),
        Key::R1(pk) => R1Member::new(pk).curry_tree_hash(),
        Key::Passkey(pk) if fast_forward => PasskeyMemberPuzzleAssert::new(pk).curry_tree_hash(),
        Key::Passkey(pk) => PasskeyMember::new(pk).curry_tree_hash(),
        Key::Bls(pk) => BlsMember::new(pk).curry_tree_hash(),
    }
}

fn wrapper(puzzle_hash: TreeHash) -> Restriction {
    Restriction {
        kind: RestrictionKind::DelegatedPuzzleWrapper,
        puzzle_hash,
    }
}

fn member_validator_list_hash(timelock: u64) -> Bytes32 {
    vec![Timelock::new(timelock).curry_tree_hash()]
        .tree_hash()
        .into()
}

fn delegated_puzzle_validator_list_hash() -> Bytes32 {
    ().tree_hash().into()
}

fn recovery_finish_puzzle(
    ctx: &mut SpendContext,
    keys: &VaultKeys,
    post_recovery_inner_puzzle_hash: Bytes32,
    amount: u64,
    memos: NodePtr,
) -> anyhow::Result<NodePtr> {
    let conditions = if keys.force_singleton_recreation {
        Conditions::new()
            .create_coin(post_recovery_inner_puzzle_hash, amount, Memos::Some(memos))
            .assert_seconds_relative(keys.clawback_timelock)
            .assert_my_amount(amount)
    } else {
        Conditions::new()
            .create_coin(post_recovery_inner_puzzle_hash, 1, Memos::Some(memos))
            .assert_seconds_relative(keys.clawback_timelock)
    };

    Ok(ctx.delegated_spend(conditions)?.puzzle)
}

#[allow(clippy::struct_field_names)]
struct VaultInternals {
    inner_puzzle_hash: TreeHash,
    custody_hash: TreeHash,
    recovery_hash: TreeHash,
}

/// Port of `getVaultInternals`, which computes the vault's inner puzzle hash directly from the
/// same MIPS construction functions used to spend the vault.
fn vault_internals(
    ctx: &mut SpendContext,
    keys: &VaultKeys,
    state: State,
    recovery_finish_memos: NodePtr,
) -> anyhow::Result<VaultInternals> {
    let signer_hashes = |signers: &Signers| {
        let mut hashes: Vec<TreeHash> = signers
            .keys
            .iter()
            .map(|&key| mips_puzzle_hash(0, vec![], key_puzzle_hash(key, true), false))
            .chain(signers.vault_launcher_ids.iter().map(|&launcher_id| {
                mips_puzzle_hash(
                    0,
                    vec![],
                    SingletonMember::new(launcher_id).curry_tree_hash(),
                    false,
                )
            }))
            .collect();
        hashes.sort();
        hashes
    };

    let custody_hashes = signer_hashes(&keys.custody);

    let custody_hash = if keys.custody.count() > 1 {
        mips_puzzle_hash(
            0,
            vec![],
            MofN::new(keys.custody.threshold, custody_hashes).inner_puzzle_hash(),
            false,
        )
    } else {
        custody_hashes[0]
    };

    let recovery_hash = match state {
        State::Custody => {
            let mut restrictions = vec![wrapper(
                Force1of2RestrictedVariable::new(
                    custody_hash.into(),
                    0,
                    member_validator_list_hash(keys.clawback_timelock),
                    delegated_puzzle_validator_list_hash(),
                )
                .curry_tree_hash(),
            )];

            restrictions.extend(
                [
                    CREATE_COIN_ANNOUNCEMENT,
                    CREATE_PUZZLE_ANNOUNCEMENT,
                    SEND_MESSAGE,
                    RECEIVE_MESSAGE,
                ]
                .into_iter()
                .map(|opcode| wrapper(PreventConditionOpcode::new(opcode).curry_tree_hash())),
            );
            restrictions.push(wrapper(PREVENT_MULTIPLE_CREATE_COINS_HASH.into()));

            if keys.force_singleton_recreation {
                restrictions.push(wrapper(FORCE_SINGLETON_RECREATION_HASH.into()));
            }

            if keys.recovery.count() > 1 {
                mips_puzzle_hash(
                    0,
                    restrictions,
                    MofN::new(keys.recovery.threshold, signer_hashes(&keys.recovery))
                        .inner_puzzle_hash(),
                    false,
                )
            } else if let Some(&key) = keys.recovery.keys.first() {
                mips_puzzle_hash(0, restrictions, key_puzzle_hash(key, true), false)
            } else {
                mips_puzzle_hash(
                    0,
                    restrictions,
                    SingletonMember::new(keys.recovery.vault_launcher_ids[0]).curry_tree_hash(),
                    false,
                )
            }
        }
        State::Recovery {
            post_recovery_inner_puzzle_hash,
            amount,
        } => {
            let puzzle = recovery_finish_puzzle(
                ctx,
                keys,
                post_recovery_inner_puzzle_hash,
                amount,
                recovery_finish_memos,
            )?;

            mips_puzzle_hash(
                0,
                vec![Restriction {
                    kind: RestrictionKind::MemberCondition,
                    puzzle_hash: Timelock::new(keys.clawback_timelock).curry_tree_hash(),
                }],
                ctx.tree_hash(puzzle),
                false,
            )
        }
    };

    Ok(VaultInternals {
        inner_puzzle_hash: mips_puzzle_hash(
            0,
            vec![],
            MofN::new(1, vec![custody_hash, recovery_hash]).inner_puzzle_hash(),
            true,
        ),
        custody_hash,
        recovery_hash,
    })
}

struct Node {
    hash: TreeHash,
    memo: InnerPuzzleMemo,
}

impl Node {
    fn new(memo: InnerPuzzleMemo) -> Self {
        Self {
            hash: memo.inner_puzzle_hash(false),
            memo,
        }
    }
}

/// Port of `createMipsSignerTree`.
fn signer_tree(
    allocator: &mut Allocator,
    signers: &Signers,
    restrictions: Vec<RestrictionMemo>,
) -> anyhow::Result<Node> {
    let mut nodes = Vec::new();

    for &key in &signers.keys {
        let member = match key {
            Key::K1(pk) => MemberMemo::k1(allocator, pk, true, true)?,
            Key::R1(pk) => MemberMemo::r1(allocator, pk, true, true)?,
            Key::Passkey(pk) => MemberMemo::passkey(allocator, pk, true, true)?,
            Key::Bls(pk) => MemberMemo::bls(allocator, pk, false, false, true)?,
        };
        nodes.push(Node::new(InnerPuzzleMemo::new(
            0,
            vec![],
            MemoKind::Member(member),
        )));
    }

    for &launcher_id in &signers.vault_launcher_ids {
        let member = MemberMemo::singleton(allocator, launcher_id, false, true)?;
        nodes.push(Node::new(InnerPuzzleMemo::new(
            0,
            vec![],
            MemoKind::Member(member),
        )));
    }

    if nodes.len() > 1 {
        nodes.sort_by_key(|node| node.hash);

        return Ok(Node::new(InnerPuzzleMemo::new(
            0,
            restrictions,
            MemoKind::MofN(MofNMemo::new(
                signers.threshold,
                nodes.into_iter().map(|node| node.memo).collect(),
            )),
        )));
    }

    let node = nodes.remove(0);

    if restrictions.is_empty() {
        return Ok(node);
    }

    Ok(Node::new(InnerPuzzleMemo::new(
        0,
        restrictions,
        node.memo.kind,
    )))
}

/// Port of `getVaultMipsMemoListBytes`, which returns the memo list and the top level memo.
fn vault_memo_list(
    ctx: &mut SpendContext,
    keys: &VaultKeys,
    state: State,
    recovery_finish_memos: NodePtr,
) -> anyhow::Result<(NodePtr, MipsMemo)> {
    let custody = signer_tree(ctx, &keys.custody, vec![])?;

    let recovery = match state {
        State::Custody => {
            let force_1_of_2 = RestrictionMemo::force_1_of_2_restricted_variable(
                ctx,
                custody.hash.into(),
                0,
                member_validator_list_hash(keys.clawback_timelock),
                delegated_puzzle_validator_list_hash(),
            )?;

            let mut wrappers = vec![WrapperMemo::new(
                force_1_of_2.puzzle_hash,
                force_1_of_2.memo,
            )];

            for opcode in [
                CREATE_COIN_ANNOUNCEMENT,
                CREATE_PUZZLE_ANNOUNCEMENT,
                SEND_MESSAGE,
                RECEIVE_MESSAGE,
            ] {
                wrappers.push(WrapperMemo::prevent_condition_opcode(ctx, opcode, true)?);
            }
            wrappers.push(WrapperMemo::prevent_multiple_create_coins());

            if keys.force_singleton_recreation {
                wrappers.push(WrapperMemo::force_singleton_recreation());
            }

            let restrictions = vec![RestrictionMemo::enforce_delegated_puzzle_wrappers(
                ctx, &wrappers,
            )?];

            signer_tree(ctx, &keys.recovery, restrictions)?
        }
        State::Recovery {
            post_recovery_inner_puzzle_hash,
            amount,
        } => {
            let puzzle = recovery_finish_puzzle(
                ctx,
                keys,
                post_recovery_inner_puzzle_hash,
                amount,
                recovery_finish_memos,
            )?;
            let puzzle_hash = ctx.tree_hash(puzzle);

            Node::new(InnerPuzzleMemo::new(
                0,
                vec![RestrictionMemo::timelock(
                    ctx,
                    keys.clawback_timelock,
                    true,
                )?],
                MemoKind::Member(MemberMemo::new(puzzle_hash.into(), puzzle)),
            ))
        }
    };

    let memo = MipsMemo::new(InnerPuzzleMemo::new(
        0,
        vec![],
        MemoKind::MofN(MofNMemo::new(1, vec![custody.memo, recovery.memo])),
    ));

    let memo_node = memo.to_clvm(&mut **ctx)?;
    let memo_list = clvm_list!(memo_node).to_clvm(&mut **ctx)?;

    Ok((memo_list, memo))
}

/// Cloud Wallet writes the memo list as `[mips_memo]`.
fn parse_memo_list(allocator: &mut Allocator, memos: NodePtr) -> anyhow::Result<MipsMemo> {
    let items = Vec::<NodePtr>::from_clvm(allocator, memos)?;
    let [memo] = items.as_slice() else {
        anyhow::bail!("expected a single memo, found {}", items.len());
    };
    Ok(MipsMemo::from_clvm(&*allocator, *memo)?)
}

#[derive(Default)]
struct Found {
    members: usize,
    custom_members: usize,
    force_1_of_2_wrappers: usize,
    wrappers: usize,
    timelock_restrictions: usize,
}

/// Walks the memo tree, asserting that every node resolves to a known type without any context.
fn assert_fully_known(
    allocator: &mut Allocator,
    memo: &InnerPuzzleMemo,
    found: &mut Found,
) -> anyhow::Result<()> {
    let ctx = MipsMemoContext::default();

    for restriction in &memo.restrictions {
        match restriction.parse(allocator, &ctx) {
            Some(ParsedRestriction::EnforceDelegatedPuzzleWrappers(parsed, wrappers)) => {
                // Cloud Wallet memos written before wrapper puzzle hashes were included.
                let legacy = legacy_delegated_puzzle_wrappers(allocator, &wrappers)?;
                assert_eq!(
                    legacy.parse(allocator, &ctx),
                    Some(ParsedRestriction::EnforceDelegatedPuzzleWrappers(
                        parsed,
                        wrappers.clone()
                    ))
                );

                for wrapper in wrappers {
                    let parsed = wrapper.parse(allocator, &ctx).expect("unknown wrapper");

                    found.wrappers += 1;

                    if matches!(parsed, ParsedWrapper::Force1of2RestrictedVariable(_)) {
                        found.force_1_of_2_wrappers += 1;
                    }
                }
            }
            Some(ParsedRestriction::Timelock(_)) => found.timelock_restrictions += 1,
            Some(ParsedRestriction::Force1of2RestrictedVariable(_)) => {}
            None => panic!("unknown restriction"),
        }
    }

    match &memo.kind {
        MemoKind::Member(member) => {
            let parsed = member.parse(allocator, &ctx).expect("unknown member");

            found.members += 1;

            if matches!(parsed, ParsedMember::Custom(_)) {
                found.custom_members += 1;
            }
        }
        MemoKind::MofN(m_of_n) => {
            for item in &m_of_n.items {
                assert_fully_known(allocator, item, found)?;
            }
        }
    }

    Ok(())
}

fn k1(seed: u64) -> Key {
    Key::K1(K1Pair::new(seed).pk)
}

fn r1(seed: u64) -> Key {
    Key::R1(R1Pair::new(seed).pk)
}

fn passkey(seed: u64) -> Key {
    Key::Passkey(R1Pair::new(seed).pk)
}

fn bls(seed: u64) -> Key {
    Key::Bls(BlsPair::new(seed).pk)
}

fn custody_signers(case: &str) -> Signers {
    match case {
        "single_k1" => Signers::new(vec![k1(1)], vec![], 1),
        "single_bls" => Signers::new(vec![bls(1)], vec![], 1),
        "two_of_four_mixed" => Signers::new(vec![k1(1), r1(2), passkey(3), bls(4)], vec![], 2),
        "nested_vault" => Signers::new(vec![passkey(1)], vec![Bytes32::new([42; 32])], 1),
        _ => unreachable!(),
    }
}

fn recovery_signers(case: &str) -> Signers {
    match case {
        "single_k1" => Signers::new(vec![k1(100)], vec![], 1),
        "single_vault" => Signers::new(vec![], vec![Bytes32::new([43; 32])], 1),
        "two_of_two" => Signers::new(vec![r1(101)], vec![Bytes32::new([44; 32])], 2),
        _ => unreachable!(),
    }
}

#[rstest]
fn test_vault_memo_roundtrip(
    #[values("single_k1", "single_bls", "two_of_four_mixed", "nested_vault")] custody: &str,
    #[values("single_k1", "single_vault", "two_of_two")] recovery: &str,
    #[values(true, false)] force_singleton_recreation: bool,
    #[values(true, false)] recovery_state: bool,
) -> anyhow::Result<()> {
    let mut ctx = SpendContext::new();

    let keys = VaultKeys {
        custody: custody_signers(custody),
        recovery: recovery_signers(recovery),
        clawback_timelock: 86_400,
        force_singleton_recreation,
    };

    let (state, recovery_finish_memos) = if recovery_state {
        let post_recovery_keys = VaultKeys {
            custody: recovery_signers(recovery),
            recovery: custody_signers(custody),
            ..keys.clone()
        };

        let post_recovery =
            vault_internals(&mut ctx, &post_recovery_keys, State::Custody, NodePtr::NIL)?;
        let (post_recovery_memos, _) =
            vault_memo_list(&mut ctx, &post_recovery_keys, State::Custody, NodePtr::NIL)?;

        (
            State::Recovery {
                post_recovery_inner_puzzle_hash: post_recovery.inner_puzzle_hash.into(),
                amount: 1,
            },
            post_recovery_memos,
        )
    } else {
        (State::Custody, NodePtr::NIL)
    };

    let internals = vault_internals(&mut ctx, &keys, state, recovery_finish_memos)?;
    let (memo_list, memo) = vault_memo_list(&mut ctx, &keys, state, recovery_finish_memos)?;

    // This is the same check that Cloud Wallet does before writing the memo.
    assert_eq!(memo.inner_puzzle_hash(), internals.inner_puzzle_hash);

    let bytes = node_to_bytes(&ctx, memo_list)?;
    let memo_list = node_from_bytes(&mut ctx, &bytes)?;

    let parsed = parse_memo_list(&mut ctx, memo_list)?;
    assert_eq!(parsed.inner_puzzle_hash(), internals.inner_puzzle_hash);

    let reencoded = clvm_list!(parsed.clone()).to_clvm(&mut *ctx)?;
    assert_eq!(node_to_bytes(&ctx, reencoded)?, bytes);

    let MemoKind::MofN(top_level) = &parsed.inner_puzzle.kind else {
        panic!("expected a 1 of 2 at the top level");
    };
    assert_eq!(top_level.required, 1);
    assert_eq!(top_level.items.len(), 2);
    assert_eq!(
        top_level.items[0].inner_puzzle_hash(false),
        internals.custody_hash
    );
    assert_eq!(
        top_level.items[1].inner_puzzle_hash(false),
        internals.recovery_hash
    );

    let mut found = Found::default();
    assert_fully_known(&mut ctx, &parsed.inner_puzzle, &mut found)?;

    let custody_members = keys.custody.count();

    if recovery_state {
        assert_eq!(found.members, custody_members + 1);
        assert_eq!(found.custom_members, 1);
        assert_eq!(found.timelock_restrictions, 1);
        assert_eq!(found.wrappers, 0);

        // The recovery finish member's puzzle reveal contains the post recovery memo, which
        // can itself be parsed.
        let MemoKind::Member(recovery_finish) = &top_level.items[1].kind else {
            panic!("expected recovery finish member");
        };
        let Some(ParsedMember::Custom(puzzle)) =
            recovery_finish.parse(&ctx, &MipsMemoContext::default())
        else {
            panic!("expected custom member");
        };
        assert_eq!(tree_hash(&ctx, puzzle), recovery_finish.puzzle_hash.into());

        let output = ctx.run(puzzle, NodePtr::NIL)?;
        let conditions = Vec::<Condition>::from_clvm(&*ctx, output)?;
        let create_coin = conditions
            .into_iter()
            .find_map(Condition::into_create_coin)
            .expect("missing create coin");
        let Memos::Some(memos) = create_coin.memos else {
            panic!("missing memos");
        };
        let post_recovery = parse_memo_list(&mut ctx, memos)?;
        assert_eq!(
            post_recovery.inner_puzzle_hash(),
            create_coin.puzzle_hash.into()
        );
    } else {
        let recovery_members = keys.recovery.count();
        assert_eq!(found.members, custody_members + recovery_members);
        assert_eq!(found.custom_members, 0);
        assert_eq!(found.timelock_restrictions, 0);
        assert_eq!(found.force_1_of_2_wrappers, 1);
        assert_eq!(
            found.wrappers,
            if force_singleton_recreation { 7 } else { 6 }
        );
    }

    Ok(())
}

/// Mints a vault from the memo's inner puzzle hash, spends it through the custody path while
/// writing the memo on chain, and then parses the memo back from the spend.
#[test]
fn test_vault_memo_on_chain() -> anyhow::Result<()> {
    let mut sim = Simulator::new();
    let mut ctx = SpendContext::new();

    let custody_key = K1Pair::new(1);

    let keys = VaultKeys {
        custody: Signers::new(vec![Key::K1(custody_key.pk)], vec![], 1),
        recovery: Signers::new(vec![k1(2)], vec![], 1),
        clawback_timelock: 60,
        force_singleton_recreation: true,
    };

    let internals = vault_internals(&mut ctx, &keys, State::Custody, NodePtr::NIL)?;
    let (memo_list, memo) = vault_memo_list(&mut ctx, &keys, State::Custody, NodePtr::NIL)?;
    let custody_hash = memo.inner_puzzle_hash();

    let alice = sim.bls(1);
    let alice_p2 = StandardLayer::new(alice.pk);
    let (mint_vault, vault) =
        Launcher::new(alice.coin.coin_id(), 1).mint_vault(&mut ctx, custody_hash, ())?;
    alice_p2.spend(&mut ctx, alice.coin, mint_vault)?;
    sim.spend_coins(ctx.take(), &[alice.sk])?;

    let conditions = Conditions::new().create_coin(
        custody_hash.into(),
        vault.coin.amount,
        Memos::Some(memo_list),
    );
    let mut spend = MipsSpend::new(ctx.delegated_spend(conditions)?);

    let mut hasher = Sha256::new();
    hasher.update(ctx.tree_hash(spend.delegated.puzzle));
    hasher.update(vault.coin.puzzle_hash);
    let signature = custody_key.sk.sign_prehashed(&hasher.finalize())?;

    let member_puzzle = ctx.curry(K1MemberPuzzleAssert::new(custody_key.pk))?;
    let member_solution = ctx.alloc(&K1MemberPuzzleAssertSolution::new(
        vault.coin.puzzle_hash,
        signature,
    ))?;

    spend.members.insert(
        custody_hash,
        InnerPuzzleSpend::m_of_n(
            0,
            vec![],
            1,
            vec![internals.custody_hash, internals.recovery_hash],
        ),
    );
    spend.members.insert(
        internals.custody_hash,
        InnerPuzzleSpend::new(0, vec![], Spend::new(member_puzzle, member_solution)),
    );

    vault.spend(&mut ctx, &spend)?;
    sim.spend_coins(ctx.take(), &[])?;

    let child = vault.child(custody_hash, vault.coin.amount);
    assert!(sim.coin_state(child.coin.coin_id()).is_some());

    let (puzzle, solution) = sim
        .puzzle_and_solution(vault.coin.coin_id())
        .expect("vault should be spent");

    let mut allocator = Allocator::new();
    let puzzle = node_from_bytes(&mut allocator, puzzle.as_ref())?;
    let solution = node_from_bytes(&mut allocator, solution.as_ref())?;
    let output = clvmr::run_program(
        &mut allocator,
        &clvmr::ChiaDialect::new(0),
        puzzle,
        solution,
        u64::MAX,
    )?
    .1;

    let create_coin = Vec::<Condition>::from_clvm(&allocator, output)?
        .into_iter()
        .find_map(Condition::into_create_coin)
        .expect("missing create coin");

    let child_coin = Coin::new(
        vault.coin.coin_id(),
        create_coin.puzzle_hash,
        create_coin.amount,
    );
    assert_eq!(child_coin.coin_id(), child.coin.coin_id());

    let Memos::Some(memos) = create_coin.memos else {
        panic!("missing memos");
    };

    let parsed = parse_memo_list(&mut allocator, memos)?;
    assert_eq!(parsed.inner_puzzle_hash(), child.info.custody_hash);

    let mut found = Found::default();
    assert_fully_known(&mut allocator, &parsed.inner_puzzle, &mut found)?;
    assert_eq!(found.members, 2);
    assert_eq!(found.force_1_of_2_wrappers, 1);

    Ok(())
}
