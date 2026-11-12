#!/usr/bin/env python3
"""Regenerate public legacy vectors using the independent internal-custody checkout.
Run in its Python environment, passing --reference and --output. No node, network,
private production keys, or Rust implementation is used. Fixed public seeds only.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--reference", required=True, type=Path)
parser.add_argument(
    "--output", type=Path, default=Path(__file__).with_name("lifecycle.json")
)
args = parser.parse_args()
revision = subprocess.check_output(
    ["git", "rev-parse", "HEAD"], cwd=args.reference, text=True
).strip()
if revision != "4150dc832a5e66b5ed839974f88d1feb573dcfe2":
    raise SystemExit("Reference checkout is not the pinned revision")
sys.path.insert(0, str(args.reference.resolve()))
from chia_rs import AugSchemeMPL, G1Element, SpendBundle
from chia_rs.sized_bytes import bytes32
from chia_rs.sized_ints import uint32, uint64
from chia.types.blockchain_format.coin import Coin
from chia.types.blockchain_format.program import Program
from chia.wallet.lineage_proof import LineageProof
from chia.wallet.puzzles.singleton_top_layer import SINGLETON_LAUNCHER_HASH
from cic.drivers.prefarm_info import PrefarmInfo
from cic.drivers.puzzle_root_construction import calculate_puzzle_root
from cic.drivers.prefarm import (
    construct_full_singleton,
    construct_singleton_inner_puzzle,
    get_withdrawal_spend_info,
    get_ach_clawback_spend_info,
    get_ach_clawforward_spend_bundle,
    get_rekey_spend_info,
    get_rekey_clawback_spend_info,
    get_rekey_completion_spend,
)
from cic.drivers.singleton import construct_p2_singleton

secrets = [AugSchemeMPL.key_gen(bytes([i]) * 32) for i in range(1, 7)]
keys = sorted([s.get_g1() for s in secrets[:3]], key=bytes)
new_keys = sorted([s.get_g1() for s in secrets[3:]], key=bytes)
launcher = Coin(bytes32(b"\x09" * 32), SINGLETON_LAUNCHER_HASH, uint64(1))
info = PrefarmInfo(
    launcher.name(),
    bytes32.zeros,
    uint64(1),
    uint64(3),
    uint64(4),
    uint64(2),
    uint64(7),
)
policy = calculate_puzzle_root(info, keys, uint32(2), uint32(3), uint32(1))
new_policy = calculate_puzzle_root(info, new_keys, uint32(2), uint32(3), uint32(1))
next_policy = calculate_puzzle_root(info, keys, uint32(3), uint32(3), uint32(1))
singleton = Coin(
    launcher.name(),
    construct_full_singleton(policy.prefarm_info).get_tree_hash(),
    uint64(101),
)
lineage = LineageProof(launcher.parent_coin_info, amount=launcher.amount)
child_lineage = LineageProof(
    singleton.parent_coin_info,
    construct_singleton_inner_puzzle(policy.prefarm_info).get_tree_hash(),
    singleton.amount,
)
destination = bytes32(hashlib.sha256(b"public-internal-custody-recipient").digest())
deposit_ph = construct_p2_singleton(launcher.name()).get_tree_hash()
deposits = [
    Coin(bytes32(bytes([i]) * 32), deposit_ph, uint64(amount))
    for i, amount in [(3, 101), (4, 103)]
]
deposits.sort(key=lambda c: bytes(c.name()), reverse=True)
fee = [Program.to([60, b""])]
# Simulator's explicit consensus domain; the Python driver's historical returned
# data_to_sign uses mainnet. Record both rather than porting that TODO to the SDK.
# Read the SDK simulator's Testnet11 domain independently from Chia's constants.
from chia.consensus.default_constants import DEFAULT_CONSTANTS
from chia.consensus.constants import replace_str_to_bytes
from chia.util.config import initial_config_file

# Chia's shipped testnet11 consensus override is authoritative, not a Rust helper.
import yaml

config = yaml.safe_load(initial_config_file("config.yaml"))
overrides = config["network_overrides"]["constants"]["testnet11"]
constants = replace_str_to_bytes(DEFAULT_CONSTANTS, **overrides)
domain = constants.AGG_SIG_ME_ADDITIONAL_DATA
operations = []


def outputs(spend):
    _, conditions = Program.from_bytes(bytes(spend.puzzle_reveal)).run_with_cost(
        500_000_000, Program.from_bytes(bytes(spend.solution))
    )
    return [
        Coin(
            spend.coin.name(),
            bytes32(c[1]),
            uint64(int.from_bytes(c[2], "big", signed=True)),
        )
        for c in conditions.as_python()
        if c[0] == b"3"
    ]


def add(name, kind, bundle, signers=(), python_message=None, multiple=None):
    records = []
    contexts = []
    fee_announcement = False
    for spend in bundle.coin_spends:
        cost, conditions = Program.from_bytes(bytes(spend.puzzle_reveal)).run_with_cost(
            500_000_000, Program.from_bytes(bytes(spend.solution))
        )
        records.append({"cost": cost, "conditions": bytes(conditions).hex()})
        for c in conditions.as_python():
            if c[0] == b"<" and c[1] == b"":
                fee_announcement = True
            if c[0] != b"2":
                continue
            pk = G1Element.from_bytes(c[1])
            message = c[2] + spend.coin.name() + domain
            shares = []
            for sk in secrets:
                if sk.get_g1() in signers:
                    shares.append(
                        {
                            "public_key": bytes(sk.get_g1()).hex(),
                            "signature": bytes(
                                AugSchemeMPL.sign(sk, message, pk)
                            ).hex(),
                        }
                    )
            contexts.append(
                {
                    "public_key": bytes(pk).hex(),
                    "raw_message": c[2].hex(),
                    "message": message.hex(),
                    "partials": shares,
                }
            )
    operations.append(
        {
            "name": name,
            "kind": kind,
            "spends": [s.to_json_dict() for s in bundle.coin_spends],
            "execution": records,
            "signers": [bytes(k).hex() for k in signers],
            "contexts": contexts,
            "fee_announcement": fee_announcement,
            "python_mainnet_message": (
                python_message.hex() if python_message is not None else None
            ),
            "multiple": multiple,
        }
    )


pending = None
for name, amount, claimed, context, fees in [
    ("payment", 40, [], destination, []),
    ("payment-fee", 40, [], destination, fee),
    ("absorb-pay", 200, deposits, destination, fee),
    ("claim-context", 0, deposits, destination, fee),
    ("no-op", 0, [], destination, fee),
]:
    bundle, msg = get_withdrawal_spend_info(
        singleton, keys[:2], policy, lineage, uint64(amount), context, claimed, fees
    )
    add(name, "payment", bundle, keys[:2], msg)
    if name == "payment":
        pending = next(c for c in outputs(bundle.coin_spends[0]) if c.amount == 40)
for with_fee in [False, True]:
    bundle, msg = get_ach_clawback_spend_info(
        pending, keys[:2], policy, destination, fee if with_fee else []
    )
    add("clawback-fee" if with_fee else "clawback", "clawback", bundle, keys[:2], msg)
add(
    "payment-complete",
    "payment-complete",
    get_ach_clawforward_spend_bundle(pending, policy, destination),
)
for label, selected, multiple in [("normal", keys[:2], 1), ("slow", keys[:1], 2)]:
    for with_fee in [False, True]:
        bundle, msg = get_rekey_spend_info(
            singleton, selected, policy, lineage, new_policy, fee if with_fee else []
        )
        add(
            f"{label}-rekey" + ("-fee" if with_fee else ""),
            "rekey",
            bundle,
            selected,
            msg,
            multiple,
        )
    created = outputs(bundle.coin_spends[0])
    drop = next(c for c in created if c.amount == 0)
    child = next(c for c in created if c.amount == 101)
    cancel, msg = get_rekey_clawback_spend_info(
        drop, selected, policy, multiple, new_policy, fee
    )
    add(f"{label}-cancel", "rekey-cancel", cancel, selected, msg, multiple)
    complete = get_rekey_completion_spend(
        child, drop, selected, policy, child_lineage, child_lineage, new_policy
    )
    add(f"{label}-complete", "rekey-complete", complete, multiple=multiple)
lock, msg = get_rekey_spend_info(singleton, keys, policy, lineage)
add("lock", "lock", lock, keys, msg, 0)
add(
    "lock-start",
    "lock",
    SpendBundle([lock.coin_spends[0]], lock.aggregated_signature),
    keys,
    msg,
    0,
)
add(
    "lock-complete",
    "lock-complete",
    SpendBundle([lock.coin_spends[2], lock.coin_spends[1]], lock.aggregated_signature),
    multiple=0,
)
# Carry over filter rejection cases from tests/test_filters.py independently of
# the higher-level drivers. These deliberately use an anyone-can-spend leaf.
from cic.drivers.filters import (
    construct_payment_and_rekey_filter,
    construct_rekey_filter,
    solve_filter_for_rekey,
)
from cic.drivers.drop_coins import curry_rekey_puzzle, curry_ach_puzzle

negative_programs = []
acs = Program.to(1)


def reject(name, puzzle, solution):
    try:
        puzzle.run_with_cost(500_000_000, solution)
    except ValueError:
        negative_programs.append(
            {
                "name": name,
                "puzzle": bytes(puzzle).hex(),
                "solution": bytes(solution).hex(),
            }
        )
    else:
        raise AssertionError(f"Python accepted negative vector: {name}")


for label, constructor, multiple in [
    ("payment-filter", construct_payment_and_rekey_filter, 1),
    ("recovery-filter", construct_rekey_filter, 2),
]:
    puzzle = constructor(policy.prefarm_info, acs.get_tree_hash(), multiple)
    for amount in [0, 2]:
        solution = solve_filter_for_rekey(
            acs,
            (0, []),
            Program.to([[51, destination, amount]]),
            policy.prefarm_info.puzzle_root,
            new_policy.prefarm_info.puzzle_root,
            multiple,
        )
        reject(f"{label}-arbitrary-output-{amount}", puzzle, solution)
    pending_hash = curry_rekey_puzzle(
        multiple, policy.prefarm_info, new_policy.prefarm_info
    ).get_tree_hash()
    for amount in [2, -2]:
        solution = solve_filter_for_rekey(
            acs,
            (0, []),
            Program.to([[51, pending_hash, amount]]),
            policy.prefarm_info.puzzle_root,
            new_policy.prefarm_info.puzzle_root,
            multiple,
        )
        reject(f"{label}-wrong-rekey-amount-{amount}", puzzle, solution)
    wrong_multiple = curry_rekey_puzzle(
        0, policy.prefarm_info, new_policy.prefarm_info
    ).get_tree_hash()
    solution = solve_filter_for_rekey(
        acs,
        (0, []),
        Program.to([[51, pending_hash, 0], [51, wrong_multiple, 0]]),
        policy.prefarm_info.puzzle_root,
        new_policy.prefarm_info.puzzle_root,
        multiple,
    )
    reject(f"{label}-extra-rekey-output", puzzle, solution)
# ACH outputs must be even; a slow-rekey-only filter must reject payments.
for label, constructor, multiple, amount in [
    ("payment-filter", construct_payment_and_rekey_filter, 1, 3),
    ("recovery-filter", construct_rekey_filter, 2, 2),
]:
    puzzle = constructor(policy.prefarm_info, acs.get_tree_hash(), multiple)
    solution = Program.to(
        [
            acs,
            (0, []),
            [
                [
                    [
                        51,
                        curry_ach_puzzle(
                            policy.prefarm_info, destination
                        ).get_tree_hash(),
                        amount,
                    ]
                ],
                (policy.prefarm_info.puzzle_root, destination),
            ],
        ]
    )
    reject(f"{label}-wrong-payment", puzzle, solution)
# Higher-threshold recoveries exercise multiples beyond two and every supported
# recovery subset, independently of the primary 2-of-3 lifecycle vectors.
recovery_matrix = []
for required in [3, 4]:
    matrix_keys = sorted([sk.get_g1() for sk in secrets[:5]], key=bytes)
    old = calculate_puzzle_root(
        info, matrix_keys, uint32(required), uint32(5), uint32(1)
    )
    target = calculate_puzzle_root(
        info, matrix_keys, uint32(required - 1), uint32(5), uint32(1)
    )
    coin = Coin(
        launcher.name(),
        construct_full_singleton(old.prefarm_info).get_tree_hash(),
        uint64(101),
    )
    parent = LineageProof(
        coin.parent_coin_info,
        construct_singleton_inner_puzzle(old.prefarm_info).get_tree_hash(),
        coin.amount,
    )
    for count in range(1, required + 1):
        selected = matrix_keys[:count]
        multiple = 1 + required - count
        start, msg = get_rekey_spend_info(coin, selected, old, lineage, target, fee)
        add("start", "rekey", start, selected, msg, multiple)
        start_record = operations.pop()
        created = outputs(start.coin_spends[0])
        pending = next(c for c in created if c.amount == 0)
        continuation = next(c for c in created if c.amount == 101)
        cancel, msg = get_rekey_clawback_spend_info(
            pending, selected, old, multiple, target, fee
        )
        add("cancel", "rekey-cancel", cancel, selected, msg, multiple)
        cancel_record = operations.pop()
        finish = get_rekey_completion_spend(
            continuation, pending, selected, old, parent, parent, target
        )
        add("finish", "rekey-complete", finish, multiple=multiple)
        finish_record = operations.pop()
        recovery_matrix.append(
            {
                "policy": bytes(old).hex(),
                "new_policy": bytes(target).hex(),
                "operations": [start_record, cancel_record, finish_record],
            }
        )
policy_matrix = []
for n, required, maximum, minimum in [
    (1, 1, 1, 1),
    (2, 1, 2, 1),
    (2, 2, 2, 1),
    (3, 2, 3, 2),
    (3, 3, 3, 1),
    (5, 2, 5, 1),
    (5, 3, 5, 1),
    (5, 4, 5, 2),
    (5, 5, 5, 1),
    (6, 3, 6, 1),
]:
    matrix_keys = [s.get_g1() for s in secrets[:n]]
    derived = calculate_puzzle_root(
        info, matrix_keys, uint32(required), uint32(maximum), uint32(minimum)
    )
    policy_matrix.append(bytes(derived).hex())
result = {
    "reference_revision": revision,
    "domain": domain.hex(),
    "mainnet_domain": DEFAULT_CONSTANTS.AGG_SIG_ME_ADDITIONAL_DATA.hex(),
    "policy": bytes(policy).hex(),
    "new_policy": bytes(new_policy).hex(),
    "next_policy": bytes(next_policy).hex(),
    "negative_programs": negative_programs,
    "policy_matrix": policy_matrix,
    "recovery_matrix": recovery_matrix,
    "destination": destination.hex(),
    "singleton": singleton.to_json_dict(),
    "operations": operations,
}
args.output.write_text(json.dumps(result, indent=2) + "\n")
print(
    f"Wrote {len(operations)} independent operations to {args.output} ({args.output.stat().st_size} bytes)"
)
