# Internal Custody

Internal Custody is the prefarm puzzle family implemented by
[Chia-Network/internal-custody](https://github.com/Chia-Network/internal-custody)
revision `4150dc832a5e66b5ed839974f88d1feb573dcfe2`. It is distinct from
MIPS/Cloud Wallet vaults and ClawbackV2. The SDK vendors the original compiled
modules without changing them; provenance, license, serialized SHA256 and tree
hashes are beside the reveals in `chia-sdk-types`.

ACH means Automated Clearing House, the reference protocol’s name for delayed,
cancellable payment coins.

## Interfaces

- `chia-sdk-types::puzzles` contains typed curry arguments and solutions matching
  the original CLSP layouts, including dotted pairs. Existing standard-puzzle and
  singleton types are reused.
- `InternalCustodyLayer` implements normal `Layer` construction/parsing and
  composes with `SingletonLayer`. Recognizing a puzzle does not authorize it.
- `InternalCustodyPolicy::derive` builds the Python-compatible threshold ladder,
  roots and ordered Merkle proofs. `from_reference_bytes` / `to_reference_bytes` round
  trip Python's public `RootDerivation` streamable format. `new` accepts bounded
  typed data. `verify_definition` independently derives the root and next-level
  commitment from the keys, thresholds and immutable parameters.
- `InternalCustody` is a `Singleton<InternalCustodyPolicy>` with coin-bound payment
  and rekey constructors and the existing `child_with` / `child_lineage_proof`
  helpers. Standalone policy builders also support offline reconstruction and
  pending drop coins independently of a live singleton.
- Policy methods construct payments (with optional deposit absorption),
  absorption alone, payment completion/clawback, ordinary rekeys, slower recovery,
  rekey cancellation/completion, and security-level increases. Construction is
  independent of transport, chain discovery, signing and broadcasting.
- `parse_internal_custody_operation` verifies every supported operation, including
  payments, and returns immutable `VerifiedInternalCustodyOperation` effects and
  an optional exact signature requirement. It requires an explicit network
  domain and a positive shared CLVM cost budget. Use the `Payment` request for
  withdrawals, absorption and no-op spends; `payment()` returns their facts.
  Effects expose the signed destination context even for absorb-only/no-op
  spends, the recreated singleton, and actual rekey initiation/completion delays.
  `offset_share()` computes the public synthetic-offset signature share locally;
  raw-key locks and unsigned completions have no offset share.
- `RequiredBlsSignature::sign_partial` / `verify_partial` use the condition's
  aggregate public key as augmentation. The higher-level verified objects also
  enforce membership in the selected subset. User approval and key access remain
  the application's responsibility.

No request JSON, mobile transport, trusted-vault registry, naming/address book,
private-key storage or network client is part of this protocol API. The SDK's
existing singleton primitives support launching; there is no separate Internal
Custody launcher puzzle or service.

## Python parity

| Operation | Python reference | SDK construction |
| --- | --- | --- |
| Policy/root/proofs | `calculate_puzzle_root` | `derive` |
| Payment / absorb + payment / absorb only | `get_withdrawal_spend_info` | `payment_spends` |
| Payment cancellation | `get_ach_clawback_spend_info` | `payment_clawback_spend` |
| Payment completion | `get_ach_clawforward_spend_bundle` | `completion_spend` |
| Ordinary / slow rekey | `get_rekey_spend_info` | `start_rekey_spend` |
| Rekey cancellation | `get_rekey_clawback_spend_info` | `rekey_clawback_spend` |
| Rekey completion | `get_rekey_completion_spend` | `complete_rekey_spends` |
| Security increase | `get_rekey_spend_info` | `increase_security_spends` |

The normal subset has `required` keys. A subset of `k` keys authorizes a minimum
multiple of `1 + required - k`, down to `minimum`. An attempt may commit to that
minimum or any larger positive `u8` multiple. Its initiation delay is
`rekey_increment * committed_multiple`, plus `slow_rekey` when that multiple
exceeds one. `start_rekey_spend` takes a replacement policy and explicit
`InternalCustodyRekeyOptions { timelock_multiple, fee_announcement }`. To match
Python's minimum-delay form, choose `1 + required - signers.len()`; callers can
also explicitly choose any longer supported delay. Construction checks that the
replacement policy preserves the immutable configuration; clear signing also
independently derives the target definition against its root.
Completion independently waits `rekey_clawback`. Cancellation binds the pending
attempt's exact state, while the selected subset's authorized minimum must not
exceed the committed multiple. A normal quorum can therefore veto a slower
recovery. Cancellation requires no target roster: unknown or malicious roots
remain cancellable. A cancelled zero-value rekey coin creates no outputs.

A lock uses `required + 1` keys, the precommitted next root, multiple zero,
a raw aggregate public key and an empty raw AGG_SIG_ME message; it cannot be
cancelled.

Deposit input order is preserved, including its delegated-announcement order.
Signer order does not change the aggregate key; duplicates are rejected. Payment
arithmetic uses a wider intermediate and checks the final coin against `u64`,
allowing absorption plus withdrawal even when their intermediate balance exceeds
one coin's range. Deposit totals and withdrawals must be even, preserving the
singleton's odd value. An absorb-only operation can retain a nonzero unused destination context, and a
zero-value no-op is supported, matching Python. `payment_spends` always takes
explicit `InternalCustodyPaymentOptions`: choose `Withdraw` or `Absorb` with its
signed destination context and specify the fee-announcement form.

The CLI's optional extra `[CREATE_COIN_ANNOUNCEMENT, ""]` is explicitly supported
by payment, payment-clawback, rekey and rekey-clawback builders/verifiers. It binds
an independent fee sponsor that asserts the announcing coin's announcement; it
does not authorize a deduction from vault funds. Arbitrary extra delegated
conditions remain available through low-level puzzle types, but the clear-signing
verifiers reject effects outside the typed supported forms.

Payment cancellation returns funds to the launcher's p2-singleton deposit
address. They require a subsequent absorb before becoming singleton funds.
Cancellation does not expire at the completion deadline: cancellation and
completion compete until the pending coin is spent. A pending payment retains its
original policy even if the live singleton later rekeys.

### Deliberate compatibility boundaries

- Unlike the reference CLI’s minimum-only rekeys, the SDK accepts any committed
  multiple at least the signing subset’s minimum, through 255. A stronger quorum
  may cancel a slower attempt without knowing its target roster. Both behaviors
  follow the unchanged puzzles’ authorization rules.
- The SDK rejects `minimum > required` and bounds public policies to 16 keys and
  1024 proofs per list, even where the reference CLI accepts larger configurations.
  These fail-closed limits bound offline work; they are not consensus limits.
- The compiled puzzles are unchanged. Current consensus rejects ephemeral inputs
  carrying relative-time assertions, even with zero delay. Python's three-spend
  lock bundle must therefore be submitted in two stages. `increase_security_spends`
  returns signed `authorization` and unsigned `completion` separately; confirm
  the first before submitting the second. `complete_rekey_spends` also supports
  reconstructing the second stage from persisted chain state. The verifier can
  describe the full historical three-spend bundle, but does not claim it is a
  single consensus-valid transaction.
- Python's historical `data_to_sign` helper hardcodes the mainnet AGG_SIG_ME
  domain. The SDK always uses the caller's explicit `AggSigConstants`. Tests
  compare the historical mainnet bytes separately from actual testnet11
  condition-derived messages; the historical domain bug is not propagated.
- Rekeying changes the root, not the immutable puzzle STATE. Replacement
  definitions must preserve all six immutable fields, including `rekey_increment`
  and `slow_rekey`; the historical Python metadata compatibility helper omits
  those last two checks. Allowing a claimed configuration that differs from the
  actual immutable STATE would misrepresent the resulting vault.

## Verification and bounds

Verifiers require byte-exact canonical reconstruction and execute the supplied
programs using the consensus dialect, without debug logging. They check exact
outputs, signature kind/key/context, parent/amount assertions, policy inclusion
proofs, deposit bindings and mutual rekey announcements. Replacement policy
metadata is independently derived against its committed root. Extra, missing,
reordered or substituted inputs/conditions fail closed, except that a different
valid independent pending coin naturally has a different coin ID and signing
context. Every payment, absorption and no-op recreates the singleton reported as
`vault_child`. Payment withdrawal and rekey initiation delays are read from the
executed singleton's relative-time assertions, measured from confirmation of
that singleton input. Completion delays are measured from confirmation of the
pending payment or rekey coin. Reported delays do not establish time readiness;
conditions are preserved, not claimed to have elapsed. Unrepresentable typed
conditions, including relative delays outside `u64`, are rejected.

Signed effects report the empty fee-sponsor announcement form for payments,
payment cancellations, rekeys and rekey cancellations. Payment cancellations
also report the original recipient committed by the pending coin.

The caller must supply a bounded allocator, expected network domain, and current
chain state. These verifiers do not establish inclusion, unspentness, freshness,
relative-time readiness, organizational identity or user approval. They do not
fetch, persist, sign with stored keys, broadcast or retry transactions.

Public configurations are limited to 128 KiB, 16 keys, 1024 proofs per list and
Merkle depth 32. Derivation bounds combination expansion before generating
leaves. Individual input programs are limited to 256 KiB. Payment verification
accepts a singleton followed by at most 63 explicit deposit inputs; lifecycle
verification accepts only the operation's exact input set (up to three for the
historical combined lock). These are SDK resource bounds, not puzzle consensus
limits.

## Independent tests

`internal_custody/tests/generate_reference.py` uses only the pinned
Python checkout and deterministic public test seeds. It generates:

- 31 primary operation variants and 21 additional higher-threshold rekey/recovery
  variants, with exact spend bytes, executed conditions,
  costs, aggregate keys, network-specific signature messages and Python-generated
  partial signatures;
- ten additional policy/threshold matrices with exact streamable bytes and proofs;
- twelve rejection vectors carried from the original filter tests.

Rust tests compare against those independent bytes, then submit signed spends to
`chia-sdk-test::Simulator` for consensus validation. Connected lifecycle tests
cover cancellation/completion races, refund absorption, normal and slow rekeys,
replacement-policy payments, staged locks and independent fee sponsors. Reference
regressions include intermediate-overflow payments, unknown-root cancellation,
stronger-quorum vetoes, and longer committed delays up to the `u8` maximum.
Adversarial tests cover wrong subsets/proofs/lineage, substituted inputs, immutable
STATE changes, missing announcement partners, hidden effects, noncanonical
programs, duplicate inputs, wrong amounts and shared cost-budget exhaustion.

The generator requires the published releases `chia-blockchain==2.7.5` and
`chia_rs==0.48.1` and records both versions in the fixture. Reveals are stored once
in a `programs` table keyed by serialized SHA256; operations reference that table.

From the SDK checkout, create an isolated Python 3.14 environment, install the
released packages, and check out the pinned reference revision:

```sh
python3.14 -m venv /tmp/custody-reference-env
/tmp/custody-reference-env/bin/python -m pip install \
  'chia-blockchain==2.7.5' 'chia_rs==0.48.1'
git clone https://github.com/Chia-Network/internal-custody \
  /tmp/internal-custody-reference
git -C /tmp/internal-custody-reference checkout --detach \
  4150dc832a5e66b5ed839974f88d1feb573dcfe2
/tmp/custody-reference-env/bin/python \
  crates/chia-sdk-driver/src/internal_custody/tests/generate_reference.py \
  --reference /tmp/internal-custody-reference
cargo test -p chia-sdk-types -p chia-sdk-signer -p chia-sdk-driver --locked
cargo clippy -p chia-sdk-types -p chia-sdk-signer -p chia-sdk-driver --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```
