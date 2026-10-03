"""pyo3 raw-key SP send + scan-from-tweaks E2E, plus the raw-key contract.

Mirrors napi/__test__/silent_payments_e2e.spec.ts in snake_case.

Cross-language coverage is scoped to the unlabeled flow; labeled detection is
exercised by the Rust-side E2E tests in
crates/chia-sdk-driver/tests/silent_payments_e2e.rs (test_simulator_e2e_labeled),
where the labeled path is already byte-pinned against the CHIP-0057 test vectors.

`TweakData` is constructed on the Rust side (via
`Simulator.tweak_data_from_block`) and crossed the FFI boundary unchanged.
This is the first runtime test of `Vec<chia_bls::PublicKey>` marshaling on
`TweakData.tweak_points` across the pyo3 FFI.

`with_silent_payment_keys` accepts RAW
PublicKey/SecretKey and synthesizes the synthetic key internally via
`derive_synthetic` (default hidden puzzle). The happy paths build sender coins
at the SYNTHETIC puzzle hash so a raw-key registration round-trips;
`test_raw_key_not_synthetic_errors` proves a wrong key surfaces the typed
`SilentPaymentKeyNotSynthetic` error across the FFI boundary (runtime guard backstop).
"""

import pytest

from chia_wallet_sdk import (
    Action,
    Clvm,
    LabelRegistry,
    Mnemonic,
    PublicKey,
    ScalarField,
    SecretKey,
    SilentPaymentAddress,
    SilentPaymentKeys,
    SilentPaymentNetwork,
    SilentPaymentRegisteredKey,
    SilentPaymentRegisteredSecretKey,
    SilentPayments,
    Simulator,
    Spends,
    standard_puzzle_hash,
)

# The BIP-39 test mnemonic (CHIP-0057 Test Vector 8) — matches the AVA + Rust
# e2e fixtures so cross-language test outputs are byte-identical.
TEST_MNEMONIC = (
    "abandon abandon abandon abandon abandon abandon "
    "abandon abandon abandon abandon abandon about"
)
K_MAX_DEFAULT = SilentPayments.k_max()


def test_k_max_is_exported():
    """CHIP-0057 "Kmax": the output limit is available without duplicating it."""
    assert SilentPayments.k_max() == 2400

# CHIP-0057 test vectors 1-7 treat the recipient keys as given values.
TV1_SCAN_SK = "132567e4dec19a4f50d9e9a549f16283dfb5aa4ad1ffdb6a505fcfcc56a690f6"
TV1_SPEND_SK = "53d140b312a0e16316314274eb6398e15706d100fe8a754990540febd931b087"


def tv1_keys():
    return SilentPaymentKeys.from_secret_keys(
        SecretKey.from_bytes(bytes.fromhex(TV1_SCAN_SK)),
        SecretKey.from_bytes(bytes.fromhex(TV1_SPEND_SK)),
    )


def test_key_derivation_tv8():
    """Hardened derivation from the mnemonic, against CHIP-0057 Test Vector 8."""
    keys = SilentPaymentKeys.from_mnemonic(Mnemonic(TEST_MNEMONIC))
    assert (
        keys.scan_sk().to_bytes().hex()
        == "0c474f92e8945069c200bb09302d1e569a9b52f59cc04a27874b1bca2adeca9f"
    )
    assert (
        keys.spend_sk().to_bytes().hex()
        == "4f8acf271744cf7050197623569e1603b0193f84b958b5d5f1ce8fd18c908e7b"
    )
    assert (
        keys.unlabeled_address(SilentPaymentNetwork.Mainnet).encode()
        == "spxch1q30etue85q8xvzrf5j4gr4j09ke6u9c4s3vrnj9unt0hdj5dhpwxv6q0kp3qxcnh8u7fr0chtttlantgv0dj6xwftvfuzhrq8sjcsce9s7nwglsk5d5knclqwrwyehuvr7a5evgndm7g527yadv9lxjvjryvvu9l2"
    )
    assert (
        keys.unlabeled_address(SilentPaymentNetwork.Testnet).encode()
        == "tspxch1q30etue85q8xvzrf5j4gr4j09ke6u9c4s3vrnj9unt0hdj5dhpwxv6q0kp3qxcnh8u7fr0chtttlantgv0dj6xwftvfuzhrq8sjcsce9s7nwglsk5d5knclqwrwyehuvr7a5evgndm7g527yadv9lxjvjrycwanlf"
    )


def test_given_keys_tv5_addresses():
    """Addresses of the given TV1 keys, against CHIP-0057 Test Vector 5."""
    keys = tv1_keys()
    assert (
        keys.unlabeled_address(SilentPaymentNetwork.Mainnet).encode()
        == "spxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsmqrz7f"
    )
    assert (
        keys.labeled_address(SilentPaymentNetwork.Testnet, 1).encode()
        == "tspxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfd9jj2rac2q707npyfumq4wzqwkl79ksppyt5t58gecmqyj4396tzwlglqtamuqwwusfd6t8pkaq5cgn3xqq3"
    )


def test_identity_key_address_is_rejected():
    """An address assembled with an identity key cannot be encoded or sent to."""
    keys = tv1_keys()
    bad = SilentPaymentAddress(
        PublicKey.infinity(), keys.spend_pk(), SilentPaymentNetwork.Testnet
    )
    with pytest.raises(BaseException, match="identity element"):
        bad.encode()

    sim = Simulator()
    clvm = Clvm()
    sender = sim.bls(1_000)
    spends = Spends(clvm, sender.puzzle_hash)
    spends.add_xch(sender.coin)
    with pytest.raises(BaseException, match="identity element"):
        spends.apply([Action.silent_payment_send(bad, 100, None)])


def test_decode_returns_the_network():
    """A sender compares the decoded network with the one it transacts on."""
    keys = tv1_keys()
    for network in [SilentPaymentNetwork.Mainnet, SilentPaymentNetwork.Testnet]:
        encoded = keys.unlabeled_address(network).encode()
        assert SilentPaymentAddress.decode(encoded).network == network


def test_sender_primitives_tv4():
    """The sender-side primitives, against CHIP-0057 Test Vector 4 (two inputs)."""
    keys = tv1_keys()
    a_syn_0 = SecretKey.from_bytes(
        bytes.fromhex(
            "5002eaf015c1c3a9694cc054e96273279732f4f963616ff89b6d4addcd678c7a"
        )
    )
    a_syn_1 = SecretKey.from_bytes(
        bytes.fromhex(
            "05fded8808216b65d439fc41cb07c7270e37ed743e0745652afe055cfe91cf0f"
        )
    )
    coin_ids = [
        bytes.fromhex(
            "2b9857e0307ebfbe51829e3be8c992ae57f6a8debe06a5deab429ddae83a8c1a"
        ),
        bytes.fromhex(
            "209bb03a4cd165785e6149bc6dcb27e35829006f02ec927ab5a20521fd27d21a"
        ),
    ]

    # The key sum is a SecretKey.
    a_sum = SilentPayments.aggregate_sender_sks([a_syn_0, a_syn_1])
    assert (
        a_sum.to_bytes().hex()
        == "5600d8781de32f0f3d86bc96b46a3a4ea56ae26da168b55dc66b503acbf95b89"
    )
    input_hash = SilentPayments.compute_input_hash(coin_ids, a_sum.public_key())
    assert (
        input_hash.to_bytes().hex()
        == "3f1071552b7f2f5e49b68166cb204f0a1b6a23b0c30a28bcba59a9c3f766e166"
    )
    puzzle_hash = SilentPayments.derive_one_time_puzzle_hash(
        keys.scan_pk(), keys.spend_pk(), a_sum, input_hash, 0
    )
    assert (
        puzzle_hash.hex()
        == "5d7fc7d7447c746cfb400e801a169fc7bfd1c13e03bc7866e6b743860a53ac6b"
    )

    # The spend key holder completes a detection's tweak (here t_0 of TV4).
    t_0 = ScalarField.from_bytes(
        bytes.fromhex(
            "18fafd6001bef3fece078f469731b40a5f362994795f9ff6b9339aa235fee312"
        )
    )
    onetime_sk = SilentPayments.derive_onetime_sk(keys.spend_sk(), t_0)
    assert (
        onetime_sk.to_bytes().hex()
        == "6ccc3e13145fd561e438d1bb82954cebb63cfa9577ea15404987aa8e0f309399"
    )


def test_zero_key_sum_is_rejected():
    """Secret keys that sum to zero mod r make the sender fail (keys 1 and r - 1)."""
    one = SecretKey.from_bytes((1).to_bytes(32, "big"))
    r = 0x73EDA753299D7D483339D80809A1D80553BDA402FFFE5BFEFFFFFFFF00000001
    r_minus_one = SecretKey.from_bytes((r - 1).to_bytes(32, "big"))
    with pytest.raises(BaseException, match="key sum is zero"):
        SilentPayments.aggregate_sender_sks([one, r_minus_one])


def test_labels_and_change_address():
    """Label generation and the change address, against CHIP-0057 TV3 and TV7."""
    keys = tv1_keys()

    # TV3: label m = 1.
    label = SilentPayments.generate_label(keys.scan_sk(), 1)
    assert (
        label.scalar.to_bytes().hex()
        == "48fa440acca87f501b9984b5d23327d0b7766a4baa913dfb3001d412c48ce465"
    )
    assert (
        label.public_key.to_bytes().hex()
        == "a6dcff3646739745ef7f3ba8e51808dac13765fa9d5e73386d3fbd7841e0773e"
        "02a0f8d91baf57d337954322bd06d80c"
    )
    labeled = keys.labeled_address(SilentPaymentNetwork.Mainnet, 1)
    assert (
        labeled.spend_pk.to_bytes().hex()
        == "965250fb8503cff4c244f360ab84075bfe2da01091745d0e8ce36024ab12e962"
        "77d1f02fbbe01cee412dd2ce1b7414c2"
    )

    # TV7: the change label m = 0.
    change_label = SilentPayments.generate_label(keys.scan_sk(), 0)
    assert (
        change_label.scalar.to_bytes().hex()
        == "3106829938a8b73a652a9a31c6c76a37e32f67f924a50e3649291d6904f22082"
    )
    change = keys.change_address(SilentPaymentNetwork.Mainnet)
    assert change.scan_pk.to_bytes() == keys.scan_pk().to_bytes()
    assert (
        change.spend_pk.to_bytes().hex()
        == "a2c089434a6abae657b8e3a868f3d1b94299b141f3da6a6788f966b0856d6802"
        "2bd58459d964b7514111529647ebd7e8"
    )

    # The change address is only available through change_address.
    with pytest.raises(BaseException, match="reserved for change"):
        keys.labeled_address(SilentPaymentNetwork.Mainnet, 0)


def test_unlabeled_e2e():
    sim = Simulator()
    clvm = Clvm()

    # Recipient: deterministic mnemonic.
    recipient = SilentPaymentKeys.from_mnemonic(Mnemonic(TEST_MNEMONIC))
    recipient_address = recipient.unlabeled_address(SilentPaymentNetwork.Testnet)

    # Sender: fresh BLS pair from simulator (used only for its key pair).
    sender = sim.bls(1_000)

    # with_silent_payment_keys synthesizes the registered RAW key via
    # derive_synthetic internally, so the sender coin must live at the SYNTHETIC
    # puzzle hash for the raw-key registration to round-trip through the runtime guard.
    sender_synthetic_pk = sender.pk.derive_synthetic()
    sender_synthetic_sk = sender.sk.derive_synthetic()
    sender_ph = standard_puzzle_hash(sender_synthetic_pk)
    sender_coin = sim.new_coin(sender_ph, 1_000)
    height_before = sim.height()

    # Build the SP send via the dedicated Action.silent_payment_send +
    # with_silent_payment_keys path.
    spends = Spends(clvm, sender_ph)
    spends.add_xch(sender_coin)

    actions = [
        Action.silent_payment_send(recipient_address, 100, None)
    ]

    # Register RAW SP keys (wrapper-class form — bindy doesn't
    # marshal Vec<(K,V)> directly). The facade synthesizes derive_synthetic(...)
    # internally.
    spends.with_silent_payment_keys(
        [SilentPaymentRegisteredKey(sender_ph, sender.pk)],
        [SilentPaymentRegisteredSecretKey(sender_ph, sender.sk)],
    )

    deltas = spends.apply(actions)
    finished = spends.prepare(deltas, None)

    # Standard-puzzle-spend the sender's XCH input. The coin is curried over the
    # SYNTHETIC key, so spend + sign with the synthetic key pair.
    for pending in finished.pending_spends():
        finished.insert(
            pending.coin().coin_id(),
            clvm.standard_spend(
                sender_synthetic_pk, clvm.delegated_spend(pending.conditions())
            ),
        )
    finished.spend()

    # Farm the block.
    sim.spend_coins(clvm.coin_spends(), [sender_synthetic_sk])

    # Extract TweakData via the bindings helper.
    tweak_data = sim.tweak_data_from_block(height_before)
    assert len(tweak_data.tweak_points) == 1, "one SP transaction -> one tweak_point"
    assert len(tweak_data.outputs) >= 1, "at least the recipient's output is present"

    # Vec<PublicKey> runtime marshaling proof — the first time this Vec
    # crosses the FFI boundary in any test. Each tweak point must be a
    # valid PublicKey we can serialize back to bytes.
    for tp in tweak_data.tweak_points:
        assert len(tp.to_bytes()) == 48, "each tweak_point round-trips as 48 bytes"

    # Scan.
    labels = LabelRegistry()
    detections = SilentPayments.scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        tweak_data,
        labels,
        K_MAX_DEFAULT,
    )

    assert len(detections) == 1, "scanner finds exactly one SP output"
    assert detections[0].k == 0, "first output at this scan_pk -> k=0"
    assert detections[0].label is None, "unlabeled detection -> label is None"
    assert detections[0].amount == 100, "amount round-trips"

    # derive_synthetic + standard-puzzle-spend the
    # detected coin from Python. Strongest reading of "Vec<PublicKey>
    # marshals correctly" — the cross-language client not only reads
    # tweak_points but can complete the full send -> farm -> extract ->
    # scan -> SPEND round-trip.
    # The detection carries only the combined tweak; the spend secret key is
    # first needed here, to derive the one-time key.
    onetime_sk = detections[0].onetime_sk(recipient.spend_sk())
    synthetic_sk = onetime_sk.derive_synthetic()
    synthetic_pk = synthetic_sk.public_key()

    detected_coin_id = detections[0].coin_id
    detected_amount = detections[0].amount
    detected_coin_state = sim.coin_state(detected_coin_id)
    assert detected_coin_state is not None, "detected coin in simulator state"
    assert (
        detected_coin_state.spent_height is None
    ), "detected coin is unspent before follow-on spend"

    follow_clvm = Clvm()
    conditions = [
        follow_clvm.create_coin(sender.puzzle_hash, detected_amount - 1, None),
        follow_clvm.reserve_fee(1),
    ]
    delegated_spend = follow_clvm.delegated_spend(conditions)
    standard_spend = follow_clvm.standard_spend(synthetic_pk, delegated_spend)
    follow_clvm.spend_coin(detected_coin_state.coin, standard_spend)

    sim.spend_coins(follow_clvm.coin_spends(), [synthetic_sk])

    after_spend = sim.coin_state(detected_coin_id)
    assert after_spend is not None, "coin state present after follow-on spend"
    assert (
        after_spend.spent_height is not None
    ), "detected SP coin successfully spent"


def test_multi_input_e2e():
    """pyo3 multi-input SP send + scan-from-tweaks E2E.

    Mirrors `test_unlabeled_e2e` but with 2 sender coins,
    `Relation.AssertConcurrent` on `prepare`, and TweakData built via the
    new `SilentPayments.tweak_data_from_block_spends` helper over
    `sim.block_spends(h) + sim.block_outputs(h)`.

    Exercises the full binding surface end-to-end: the `Relation`
    enum, the `Spends.prepare(deltas, relation)`
    signature, the `SilentPayments.tweak_data_from_block_spends`
    static method, and the `Simulator.block_spends` /
    `Simulator.block_outputs` facade additions.
    """
    from chia_wallet_sdk import Relation

    sim = Simulator()
    clvm = Clvm()

    recipient = SilentPaymentKeys.from_mnemonic(Mnemonic(TEST_MNEMONIC))
    recipient_address = recipient.unlabeled_address(SilentPaymentNetwork.Testnet)

    # Two XCH coins with different BLS pairs. The Relation
    # cycle binding ties them together so the receiver scanner can re-group
    # them as a strongly connected component of ASSERT_CONCURRENT_SPEND edges.
    #
    # with_silent_payment_keys synthesizes the registered RAW key via
    # derive_synthetic internally, so each coin must live at its SYNTHETIC
    # puzzle hash for the raw-key registration to round-trip through the runtime guard.
    sender1 = sim.bls(500)
    sender2 = sim.bls(500)
    sender1_synthetic_pk = sender1.pk.derive_synthetic()
    sender2_synthetic_pk = sender2.pk.derive_synthetic()
    sender1_synthetic_sk = sender1.sk.derive_synthetic()
    sender2_synthetic_sk = sender2.sk.derive_synthetic()
    sender1_ph = standard_puzzle_hash(sender1_synthetic_pk)
    sender2_ph = standard_puzzle_hash(sender2_synthetic_pk)
    sender1_coin = sim.new_coin(sender1_ph, 500)
    sender2_coin = sim.new_coin(sender2_ph, 500)
    height_before = sim.height()

    spends = Spends(clvm, sender1_ph)
    spends.add_xch(sender1_coin)
    spends.add_xch(sender2_coin)

    actions = [
        Action.silent_payment_send(recipient_address, 700, None)
    ]

    # Register RAW keys — the facade synthesizes derive_synthetic(...) internally.
    spends.with_silent_payment_keys(
        [
            SilentPaymentRegisteredKey(sender1_ph, sender1.pk),
            SilentPaymentRegisteredKey(sender2_ph, sender2.pk),
        ],
        [
            SilentPaymentRegisteredSecretKey(sender1_ph, sender1.sk),
            SilentPaymentRegisteredSecretKey(sender2_ph, sender2.sk),
        ],
    )

    deltas = spends.apply(actions)

    # Pass Relation.AssertConcurrent so the driver-side gate
    # (two or more spent XCH coins) is satisfied. Without it,
    # DriverError::SilentPaymentRequiresInputBinding fires inside prepare().
    finished = spends.prepare(deltas, Relation.AssertConcurrent)

    # Each coin is curried over its SYNTHETIC key; spend + sign with the
    # synthetic key pair.
    for pending in finished.pending_spends():
        is_s1 = pending.coin().puzzle_hash == sender1_ph
        synthetic_pk = sender1_synthetic_pk if is_s1 else sender2_synthetic_pk
        finished.insert(
            pending.coin().coin_id(),
            clvm.standard_spend(
                synthetic_pk, clvm.delegated_spend(pending.conditions())
            ),
        )
    finished.spend()

    sim.spend_coins(clvm.coin_spends(), [sender1_synthetic_sk, sender2_synthetic_sk])

    # Build the TweakData with SilentPayments.tweak_data_from_block_spends
    # over the block's spends and outputs, as a wallet reading real blocks
    # would, rather than with the Simulator.tweak_data_from_block shortcut.
    block_spends = sim.block_spends(height_before)
    block_outputs = sim.block_outputs(height_before)
    tweak_data = SilentPayments.tweak_data_from_block_spends(
        block_spends, block_outputs
    )
    # Additive ScanBlock model (tweak_data_from_block_spends): a 2-input
    # concurrent SP send emits 2 Pass-1 singletons + 1 Pass-2 SCC aggregate
    # = 3 candidate tweak_points. Only the SCC-aggregate point matches the
    # sender-derived input_hash, so the scanner still detects exactly one
    # output (asserted below). Mirrors the Rust inline test
    # block_tweak_data.rs::same_ph_multi_input_round_trip_via_concurrent_spend.
    assert (
        len(tweak_data.tweak_points) == 3
    ), "additive model: 2 Pass-1 singletons + 1 Pass-2 SCC aggregate"

    labels = LabelRegistry()
    detections = SilentPayments.scan_from_tweaks(
        recipient.scan_sk(),
        recipient.spend_pk(),
        tweak_data,
        labels,
        K_MAX_DEFAULT,
    )

    assert len(detections) == 1, "scanner finds exactly one SP output"
    assert detections[0].k == 0, "first output at this scan_pk -> k=0"
    assert detections[0].label is None, "unlabeled detection -> label is None"
    assert detections[0].amount == 700, "multi-input SP output amount round-trips"


def test_raw_key_not_synthetic_errors():
    """pyo3: a raw key against a non-synthetic coin surfaces the typed error.

    The sim.bls() coin is curried over the RAW pk
    (StandardArgs::curry_tree_hash(pk)). Registering RAW sender.pk makes the
    facade synthesize derive_synthetic(sender.pk), whose curry_tree_hash !=
    sender.puzzle_hash — so the runtime guard must fire inside prepare() with the typed
    SilentPaymentKeyNotSynthetic error crossing the FFI boundary, and NO spend
    bundle is produced.
    """
    sim = Simulator()
    clvm = Clvm()

    recipient = SilentPaymentKeys.from_mnemonic(Mnemonic(TEST_MNEMONIC))
    recipient_address = recipient.unlabeled_address(SilentPaymentNetwork.Testnet)

    sender = sim.bls(1_000)

    spends = Spends(clvm, sender.puzzle_hash)
    spends.add_xch(sender.coin)

    actions = [
        Action.silent_payment_send(recipient_address, 100, None)
    ]

    spends.with_silent_payment_keys(
        [SilentPaymentRegisteredKey(sender.puzzle_hash, sender.pk)],
        [SilentPaymentRegisteredSecretKey(sender.puzzle_hash, sender.sk)],
    )

    deltas = spends.apply(actions)

    # the runtime guard fires inside prepare() — the typed SilentPaymentKeyNotSynthetic
    # error crosses the FFI boundary as a raised exception.
    with pytest.raises(BaseException, match="key not synthetic"):
        spends.prepare(deltas, None)

    # No spend bundle was produced on the failed path.
    assert len(clvm.coin_spends()) == 0, "no coin spends produced on the failed path"
