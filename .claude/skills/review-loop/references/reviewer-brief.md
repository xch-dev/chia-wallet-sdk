# Reviewer brief

Build each reviewer's prompt from the template below, filling in the Brief section of the state file, the base ref, the focus, and the checklist for that focus. Leave out the implementer's summary and the review log.

## Prompt template

```text
You are reviewing a change to chia-wallet-sdk (repo: xch-dev/chia-wallet-sdk), a Rust SDK for building Chia wallets and spending coins, with Node.js, WASM, Python, Go and .NET bindings, with fresh eyes. You have not seen the implementation being written, and that's on purpose: form your own view of the code.

What the change is for:
<the Brief section of the state file, verbatim>

Read the change with `git diff <base>...HEAD` (and `git log <base>..HEAD` for the commit messages). Read the surrounding code as needed. <For round 2+: "Since the last round, these areas changed: <files/functions>. Look closely there, but review the whole diff.">

Your focus this round: <Correctness | Tests and bindings | Simplification and reuse>. Other reviewers cover the other areas, so stay on yours.
<Or, with a single reviewer: "You cover all three focuses: correctness, tests and bindings, and simplification and reuse.">

<the checklist for that focus, from below, or all three for a single reviewer>

Rules:
- Don't edit files in the repo. You may write scratch files to <scratchpad>/review-loop/scratch/ and run tests to check a claim (`cargo test -p <crate> <name>`; a throwaway `Simulator` test is the usual way to check a spend). Put any such test in the scratch directory or revert it before you finish.
- Judge the change against the goal above. Points under "Settled" were decided by the user or the issue; don't re-raise them. Points under "Previously rejected" were rejected by the implementer; raise one again only if you have evidence the rejection is wrong, and say what it is. Decisions marked "provisional, for the user" will be put to the user at the end; raise one only with an argument the entry doesn't already give, and it will be passed on to them.
- Prefer a few findings you're confident in over many guesses. If you aren't sure, say so in the finding.
- If there's nothing worth reporting in your focus, say "No findings" and stop.

Report each finding in this format:

### <one-line claim>
- Severity: blocker | major | minor | nit
- Where: <path:line>
- Scenario: <a concrete input (a coin, spend, puzzle, or binding call) and the wrong result it gives, or for a non-bug, what's wrong and what it costs>
- Fix: <what to change>
- For the user: yes/no (see "Choices for the user" below)
```

Severities: **blocker** means a spend that fails or can be stolen, lost or misdirected funds, a parser that misidentifies a coin, or a panic on user input; **major** means a real bug in a less common case, a missing test for core behaviour, or a design that will clearly need redoing; **minor** means a small bug in a rare case, a missing edge-case test, or a clear simplification; **nit** means style or naming.

## Choices for the user

Mark "For the user: yes" when the fix depends on a choice the user should confirm, and that Settled doesn't already answer. The implementer makes a provisional choice and the user picks at the end, so this doesn't block anything:

- Public API: names, signatures and shapes of new or changed Rust items, and of the bindings in `bindings/*.json`.
- Breaking changes to existing public APIs, in Rust or in any binding.
- Which puzzle version, CHIP variant or on-chain format to support, when the source doesn't say.
- Whether to put a new puzzle behind a Cargo feature, and the feature's name.
- Changing the scope of the issue, or behaviour outside it.

Include this section in every reviewer prompt, after the finding format.

## Checklist: correctness

General:
- Does the change do what the goal says, all of it, and nothing outside it?
- Edge cases: zero amounts, odd amounts and change, amounts near `u64::MAX`, many coins in one spend, empty lists, ephemeral coins (spent in the same block they're created), and the first spend of a coin after its launcher.
- Errors: are failures returned as `DriverError` (or the crate's error type) rather than panicking, and do they say what went wrong?
- Performance: anything quadratic in the number of coins or spends, or CLVM allocated repeatedly where it could be built once?

Puzzles, drivers and spends (the most costly source of bugs in this repo):
- **Puzzle bytes and hashes**: do the bytes match the canonical source, and does an `assert_puzzle_hash!` test check that they hash to the constant? Are curried arguments in the order the puzzle expects?
- **Spend paths**: is every path the puzzle allows constructed correctly, and is each one exercised by a spend in the `Simulator`? Do the conditions (amounts, puzzle hashes, memos, hints, announcements and messages) match what the puzzle asserts?
- **Parsing**: does `parse_puzzle` / `parse_child` reject a puzzle that only looks similar (a different mod hash, a different inner layer, a wrong curried argument), and accept every valid form, including ones created by other wallets? A parser that accepts the wrong coin can make a wallet track or spend funds it shouldn't.
- **Lineage and singletons**: are lineage proofs, launcher IDs and singleton struct hashes built from the right coin? Does the child keep the same identity (asset ID, launcher ID) as the parent?
- **Value and security**: can a spend be altered by a third party (an unsigned condition, a missing `AGG_SIG_ME`, an announcement not asserted)? Are fees and change accounted for, so no value is created or burned unintentionally?
- **Offers and settlement**: treat the counterparty's offer as untrusted input. Do puzzle reveals match the coins they claim to spend? Are duplicate settlement payments, unsupported requested assets, and old settlement or compression versions rejected? Are royalties and requested amounts computed from what's actually committed, and are different asset kinds (such as revocable and normal CATs) kept apart? Does coin selection count each coin once and respect its limits?
- **Clear signing** (`clear_signing`): does a change to what a spend does also change what's shown to the signer, and can a spend show the signer something it doesn't do?

Bindings:
- Does each binding convert values without losing data (`u64` and larger amounts as `bigint`, byte lengths checked)?
- Do conversions from binding input fail cleanly (out-of-range numbers, wrong byte lengths, invalid enums) instead of panicking?

## Checklist: tests and bindings

- Is there a test for each behaviour the change adds, including the failures it should produce (a spend that must be rejected, a coin the parser must not accept)?
- Do driver tests actually spend the coins in the `Simulator`, rather than only building the spend? Do they parse the child back and check its fields?
- Does each test exercise what it's named for, and would it fail if the behaviour it covers regressed? A test that passes either way is worth a finding.
- Bindings: is the new API tested from at least one binding (usually `napi/__test__/<area>.spec.ts`), and from the binding the change is specific to, if any (`pyo3/tests`, `wasm/__test__`, `go/chiawalletsdk`, `dotnet/ChiaWalletSdk.Tests`)?
- Do `bindings/*.json` and the Rust implementation in `crates/chia-sdk-bindings` agree?
- Generated files (`git diff <base>...HEAD -- napi/index.d.ts napi/index.js pyo3/chia_wallet_sdk.pyi`): is every change explained by the goal? A changed signature or a removed export in an unrelated API needs a reason.
- Are new public items documented (CI runs rustdoc with `-D warnings`), and are `README.md` and `crates/chia-sdk-driver/docs.md` updated where they list this kind of feature?

## Checklist: simplification and reuse

- Is there an existing helper for this? Check the layers in `crates/chia-sdk-driver/src/layers/` and primitives in `src/primitives/`, `SpendContext` (`alloc`, `curry`, `tree_hash`, `spend`), `Spend` and `SpendWithConditions`, `Conditions` builders in `chia-sdk-types`, and the `Simulator` helpers in `chia-sdk-test` (`sim.bls(amount)`, `sim.new_coin`, `sim.pass_time`) and the key pairs in `key_pairs.rs`.
- Is logic duplicated between this change and an existing layer or primitive (for example, a hand-built singleton or CAT wrapper instead of composing the existing layers)?
- Is anything computed repeatedly that could be computed once (a puzzle hash, an allocated puzzle), or cloned where a borrow would do?
- Does a `match` use `_ =>` where an exhaustive match would make a new variant a compile error?
- Does the code follow the patterns of the neighbouring layers, primitives and bindings, so it reads like the rest of the crate?
- Dead code, unused parameters, unused dependencies (`cargo machete`), and comments that restate the code.
