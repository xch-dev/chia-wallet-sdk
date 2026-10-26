from chia_wallet_sdk import (
    Clvm,
    InnerPuzzleMemo,
    K1Pair,
    MemberConfig,
    MemberMemo,
    MemoKind,
    MipsMemo,
    MipsMemoContext,
    MofNMemo,
    PublicKey,
    R1Pair,
    RestrictionMemo,
    Reveals,
    RunCatTail,
    VaultMessageProof,
    WrapperMemo,
    calculate_vault_coin_message,
    calculate_vault_puzzle_message,
    force_1_of_2_restriction,
    k1_member_hash,
    m_of_n_hash,
    parse_vault_transaction,
    prevent_vault_side_effects_restriction,
    r1_member_hash,
    sha256,
    timelock_restriction,
    to_hex,
    tree_hash_pair,
    vault_message_envelope,
    verify_vault_signed_message,
)


def test_alloc():
    clvm = Clvm()

    program = clvm.alloc(
        [
            clvm.nil(),
            PublicKey.infinity(),
            "Hello, world!",
            42,
            100,
            True,
            bytes([1, 2, 3]),
            bytes.fromhex("00" * 32),
            None,
            None,
            RunCatTail(clvm.nil(), clvm.nil()),
        ]
    )

    assert (
        to_hex(program.serialize())
        == "ff80ffb0c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ff8d48656c6c6f2c20776f726c6421ff2aff64ff01ff83010203ffa00000000000000000000000000000000000000000000000000000000000000000ff80ff80ffff33ff80ff818fff80ff808080"
    )


def member_node(clvm, key, restrictions=[]):
    return InnerPuzzleMemo(
        0, restrictions, MemoKind.member(MemberMemo.k1(clvm, key, True, True))
    )


def test_vault_custody_memo_round_trip():
    clvm = Clvm()
    custody_keys = [pair.pk for pair in K1Pair.many_from_seed(1, 2)]
    recovery_key = K1Pair.from_seed(3).pk
    clawback_timelock = 3600

    # Mirrors `getVaultInternals` in ent-wallet for a 2 of 2 custody and single recovery key.
    config = MemberConfig()
    custody_hashes = sorted(k1_member_hash(config, key, True) for key in custody_keys)
    custody_hash = m_of_n_hash(config, 2, custody_hashes)
    timelock = timelock_restriction(clawback_timelock)
    member_validator_list_hash = tree_hash_pair(
        timelock.puzzle_hash, clvm.nil().tree_hash()
    )
    recovery_config = config.with_restrictions(
        [
            force_1_of_2_restriction(
                custody_hash, 0, member_validator_list_hash, clvm.nil().tree_hash()
            ),
            *prevent_vault_side_effects_restriction(),
        ]
    )
    recovery_hash = k1_member_hash(recovery_config, recovery_key, True)
    expected = m_of_n_hash(config.with_top_level(True), 1, [custody_hash, recovery_hash])

    # Mirrors `getVaultMipsMemoListBytes` in ent-wallet.
    custody_nodes = sorted(
        (member_node(clvm, key) for key in custody_keys),
        key=lambda node: node.inner_puzzle_hash(False),
    )
    custody = InnerPuzzleMemo(0, [], MemoKind.m_of_n(MofNMemo(2, custody_nodes)))
    force_1_of_2 = WrapperMemo.force_1_of_2_restricted_variable(
        clvm,
        custody.inner_puzzle_hash(False),
        0,
        [RestrictionMemo.timelock(clvm, clawback_timelock, True)],
    )
    recovery = member_node(
        clvm,
        recovery_key,
        [
            RestrictionMemo.enforce_delegated_puzzle_wrappers(
                clvm,
                [
                    force_1_of_2,
                    *WrapperMemo.prevent_vault_side_effects(clvm, True),
                ],
            )
        ],
    )
    memo = MipsMemo(
        InnerPuzzleMemo(0, [], MemoKind.m_of_n(MofNMemo(1, [custody, recovery])))
    )
    memo_list = clvm.alloc([clvm.mips_memo(memo)]).serialize()

    parsed = clvm.deserialize(memo_list).first().parse_mips_memo()
    assert parsed is not None
    assert parsed.inner_puzzle_hash() == expected
    assert clvm.alloc([clvm.mips_memo(parsed)]).serialize() == memo_list

    ctx = MipsMemoContext()
    [parsed_custody, parsed_recovery] = parsed.inner_puzzle.kind.as_m_of_n().items

    for item in parsed_custody.kind.as_m_of_n().items:
        member = item.kind.as_member().parse(ctx)
        assert member is not None
        assert member.as_k1() is not None
        assert member.fast_forward()

    member = parsed_recovery.kind.as_member().parse(ctx)
    assert member is not None
    assert member.as_k1().to_bytes() == recovery_key.to_bytes()
    assert member.as_r1() is None

    [restriction] = parsed_recovery.restrictions
    parsed_restriction = restriction.parse(ctx)
    assert parsed_restriction is not None
    assert parsed_restriction.as_timelock() is None

    wrappers = [
        wrapper.parse(ctx)
        for wrapper in parsed_restriction.as_enforce_delegated_puzzle_wrappers()
    ]
    force_1_of_2 = wrappers[0].as_force_1_of_2_restricted_variable()
    assert force_1_of_2.left_side_subtree_hash == custody_hash
    assert [
        restriction.parse(ctx).as_timelock() for restriction in force_1_of_2.restrictions
    ] == [clawback_timelock]
    assert [wrapper.as_prevent_condition_opcode() for wrapper in wrappers[1:5]] == [
        60,
        62,
        66,
        67,
    ]
    assert wrappers[5].is_prevent_multiple_create_coins()


def test_malformed_mips_memo():
    clvm = Clvm()
    memo = clvm.mips_memo(
        MipsMemo(member_node(clvm, K1Pair.from_seed(1).pk))
    )
    inner = memo.rest().first()

    assert memo.parse_mips_memo() is not None
    assert clvm.alloc(["CHIP-0042", inner]).parse_mips_memo() is None
    assert clvm.alloc(["CHIP-0043"]).parse_mips_memo() is None

def test_clear_signing_vault_transaction():
    clvm = Clvm()

    launcher_id = bytes([1] * 32)
    custody_hash = bytes([2] * 32)

    delegated_spend = clvm.delegated_spend([clvm.create_coin(custody_hash, 1, None)])
    delegated_puzzle_hash = delegated_spend.puzzle.tree_hash()

    tx = parse_vault_transaction(clvm, Reveals(clvm), launcher_id, delegated_spend)

    assert tx.vault_child is not None
    assert tx.vault_child.custody_hash == custody_hash
    assert tx.vault_child.amount == 1
    assert tx.spends == []
    assert tx.delegated_puzzle_hash == delegated_puzzle_hash
    assert len(tx.p2_puzzle_hashes) == 1

    vault_puzzle_hash = bytes([3] * 32)
    coin_id = bytes([4] * 32)
    genesis_challenge = bytes([5] * 32)

    assert (
        calculate_vault_puzzle_message(delegated_puzzle_hash, vault_puzzle_hash)
        == delegated_puzzle_hash + vault_puzzle_hash
    )
    assert (
        calculate_vault_coin_message(delegated_puzzle_hash, coin_id, genesis_challenge)
        == delegated_puzzle_hash + coin_id + genesis_challenge
    )


def test_vault_message_proof():
    clvm = Clvm()

    r1 = R1Pair.from_seed(1)
    config = MemberConfig().with_top_level(True)
    custody_hash = r1_member_hash(config, r1.pk, True)

    vault = clvm.mint_vault(bytes([1] * 32), custody_hash, clvm.nil()).vault

    messages = [b"hello", b"\xff\x00"]
    envelope = vault_message_envelope(clvm, messages)

    tx = parse_vault_transaction(clvm, Reveals(clvm), vault.info.launcher_id, envelope)
    assert tx.signed_messages == messages
    assert tx.vault_child is None

    message = sha256(
        calculate_vault_puzzle_message(envelope.puzzle.tree_hash(), vault.coin.puzzle_hash)
    )

    mips = clvm.mips_spend(vault.coin, envelope)
    mips.r1_member(config, r1.pk, r1.sk.sign_prehashed(message), True)

    proof = VaultMessageProof.from_custody_spend(vault.info.launcher_id, mips.spend(custody_hash))

    encoded = proof.encode_string()
    assert encoded.startswith("vaultsig1")

    for decoded in [VaultMessageProof.decode_string(encoded), VaultMessageProof.decode(proof.encode())]:
        assert verify_vault_signed_message(decoded, vault.info.launcher_id, custody_hash) == messages

    try:
        verify_vault_signed_message(proof, bytes(32), custody_hash)
        assert False, "expected the wrong launcher id to be rejected"
    except Exception:
        pass
