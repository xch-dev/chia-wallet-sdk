---
name: review-loop
description: Take a piece of chia-wallet-sdk work (an issue, a new puzzle, driver or binding, a bug fix, a follow-up to an open PR) all the way to a reviewed PR that passes the same checks as CI, running rounds of independent code review and fixing what they find, without the user having to ask for each review pass. Use this whenever the user asks to implement an issue "with reviews", to "drive this to completion", "review until clean", "keep reviewing and fixing", "take this through review", or hands over a task in the chia-wallet-sdk repo and expects a finished, reviewed PR rather than a first draft, even if they don't say "review loop". Not for a one-off review of an existing diff with no fix loop; use code-review for that.
---

# Review loop

The user's usual workflow is: an agent implements something, then the user asks for a review pass, the agent fixes the findings, and this repeats until the work is clean. This skill runs that loop for them. It was adapted from the `review-loop` skill in xch-dev/rue, where review rounds caught code that produced wrong results, missing test cases, and logic duplicated next to an existing helper. In this repo the costly mistakes are spends that fail, or succeed when they shouldn't, on paths no test exercises, parsers that accept a coin they shouldn't (or reject one they should accept), and bindings that drift between Rust, Node.js, WASM, Python and UniFFI.

The loop is: **scope and write the brief → implement → review → verify and fix → repeat until a stop condition → final checks → commit, PR and report.**

## 1. Scope the work and write the state file

Read the source first: `gh issue view <n> --comments` or `gh pr view <n> --comments`, plus the code it touches.

**Tracking issues.** If the issue lists several independent items, don't put them all on one branch. Do the items the request points to. If it names only the issue, ask which items: that's the one question worth asking up front, since a wrong guess wastes the whole run. Give each item its own branch, state file and loop, stack the branches with `gh stack`, and link the issue with "Part of #n" rather than "Closes #n".

**Open choices.** The user prefers that you make decisions and bring them up at the end, rather than stopping to ask. When the source leaves open anything on the reviewer brief's "for the user" list (public API names and shapes, binding signatures, which puzzle version or CHIP variant to support, scope), make the call: pick what fits the issue's intent and the neighbouring code, and the more conservative option when it's close. Record it under Decisions as provisional (see the template), and keep going. An issue asking for a new primitive, for example, may leave open whether to expose it in the bindings now or only in Rust; pick one, and the report offers the other. Ask up front only when no reasonable choice exists, such as an issue whose goals contradict each other.

**Follow-ups to an open PR.** Do what the user asked; if they didn't say, continue on their own `rigidity/` branches and stack on anyone else's. Base is set so that reviewers see only the new work.

- *Continuing on the PR's branch* (`gh pr checkout <n>`): keep its branch name, set Base to the commit the follow-up starts from, and push to that PR in section 7.
- *Stacking on top*: if the PR is already in a stack, `gh stack checkout <n>`; otherwise `gh pr checkout <n>` first, so its branch exists locally at the PR's head, then adopt it with `gh stack init <pr-branch>`. Then `gh stack add rigidity/<slug>`. Base is the parent branch, which stays right after a restack. The new branch gets its own PR in section 7, linking the original. A PR from a fork can't be stacked this way; continue on a new branch from its head, open a PR against the original PR's base that links it, and say so in the report.

The state file is the memory of the loop. Reviewers start with fresh context, and after a few rounds the implementer's own context is long, so without it later rounds re-raise settled points or undo earlier decisions. Write it to the session scratchpad (not the repo), at `<scratchpad>/review-loop/<branch-slug>.md`. Every review round reads it, and every fix round updates it.

```markdown
# <short title>

## Brief (shared with reviewers)
- Goal: <what this change must achieve, in the user's terms>
- Serves: <issue/PR link>
- Base: <origin/main, the parent branch in a gh stack, or the commit a PR follow-up starts from>
- Branch: <rigidity/...>
- Decisions: <each decision and why, noting who made it: the issue, the user, or the implementer. Mark choices that are the user's to confirm as "provisional, for the user: X, because Y; the alternative is Z". Include intended changes to existing public APIs or generated binding files and why.>
- Out of scope: <things deliberately left alone, and why>
- Settled: <points the user or the issue decided; not open for review>
- Previously rejected: <findings the implementer rejected in earlier rounds: the claim and the evidence>

## Decisions for the user
- <each provisional decision: the options, what you picked and why, and what switching would take>

## Review log (implementer only)
### Round 1
- [confirmed, major] <finding> -> fixed in <commit>, regression test <where>
- [rejected] <finding> -> <evidence>
- [deferred] <finding> -> <why, and where it's recorded>
```

## 2. Implement

Work on a `rigidity/` branch (lowercase; never `Rigidity/`), unless you're continuing someone else's PR branch. For a bug fix, write the failing regression test first. A new puzzle, driver or binding usually touches these layers, in order (`git show --stat 663711b2` or `32acc7b2` shows a full example):

- **Puzzle** in `crates/chia-sdk-types/src/puzzles/<name>.rs`: the serialized bytes and tree hash as constants, the curried and solution argument structs, and a test using `assert_puzzle_hash!` that the bytes hash to the constant. Re-export it from `puzzles.rs`. Take puzzle bytes from the canonical source (chia-blockchain, the CHIP, or the upstream `.clsp`/`.rue`) and say in the Brief where they came from.
- **Driver** in `crates/chia-sdk-driver/src/layers/` (a `Layer` with `construct_puzzle`, `parse_puzzle`, `construct_solution`, `parse_solution`) and, for a coin type users hold, `src/primitives/`. Add any new errors to `driver_error.rs`.
- **Driver tests** with `Simulator` from `chia-sdk-test`: spend the coin for real (`sim.spend_coins`), cover every spend path, parse the child back from its parent spend, and include cases that must fail (wrong signature, wrong amount, a coin of a different type that the parser must reject).
- **Bindings**: declare the API in `bindings/<area>.json`, implement it in `crates/chia-sdk-bindings/src/<area>.rs` and register it in `lib.rs`. Then regenerate what's committed: `napi/index.d.ts` and `napi/index.js` (`pnpm install && pnpm build:debug` in `napi/`) and `pyo3/chia_wallet_sdk.pyi` (`cargo run -p pyo3-stub-generator`). Add a Node.js test in `napi/__test__/<area>.spec.ts`; Python (`pyo3/tests`), WASM (`wasm/__test__`), Go (`go/chiawalletsdk`) and .NET (`dotnet/ChiaWalletSdk.Tests`) tests when the change touches something those tests cover or is specific to that binding.
- **Docs**: CI builds rustdoc with `-D warnings`, so every new public item needs a doc comment. Update `crates/chia-sdk-driver/docs.md` and `README.md` when they list the kind of thing you added.

Use what's already there before writing helpers: the existing layers and primitives (CAT, NFT, DID, singleton, vault/MIPS, clawback, option, offer), `SpendContext`, `Spend`/`SpendWithConditions`, `Conditions`, and `Simulator` helpers in `chia-sdk-test` (`sim.bls(amount)` for a funded key pair, `sim.pass_time`).

Before each review round, including the first, regenerate the committed binding files so reviewers see the real diff:

```bash
cargo run -p pyo3-stub-generator
(cd napi && pnpm install && pnpm build:debug)   # only if bindings changed
git status --short
```

Read the diff of `napi/index.d.ts` and `pyo3/chia_wallet_sdk.pyi`: a type or signature you didn't mean to change, or a removed export, usually means a regression or a wrong `bindings/*.json` entry. Fix unintended changes before the round starts; record intended changes to existing APIs under Decisions, so reviewers don't flag them again. Commit generated files with the change that caused them.

Commit before the first review round, so each round's fixes are a separate diff.

## 3. Review rounds

Each round, launch three reviewers **in parallel** with the Agent tool (`general-purpose`, `run_in_background: false`), one per focus:

1. **Correctness**: spends that fail or behave differently on chain, parsers that accept or reject the wrong coins, wrong amounts, hashes or conditions, panics, and binding conversions that lose or change data.
2. **Tests and bindings**: whether the tests would catch a regression in each behaviour the change adds, whether the bindings expose the change consistently, and whether diffs to the generated binding files are intended.
3. **Simplification and reuse**: duplicated logic, existing helpers that should be used, needless work, structure that doesn't match the surrounding code.

For a small diff (roughly under 100 changed lines), one reviewer covering all three focuses is enough; three reviewers on a ten-line change mostly produce nits.

Build each prompt from [references/reviewer-brief.md](references/reviewer-brief.md), which holds the template, the chia-wallet-sdk checklist for each focus, the severities, and which choices are for the user. Give reviewers the Brief section, the base ref, and the focus, and nothing else: not your summary of the change, your reasoning about why it's correct, or the review log. They read the diff (`git diff <base>...HEAD`) and form their own view, and that independence is what makes a round worth running. From round 2 they still review the whole diff against the base, since fixes break things elsewhere; name the areas that changed since the last round so they look closely there.

The built-in `code-review` skill is an alternative for the correctness focus on a large diff (`/code-review high`), but it doesn't see the Brief, so check its findings against the goal and the Settled points yourself.

## 4. Verify, then fix

Reviewers are sometimes confidently wrong. Before acting on a finding, check it:

- For a claimed bug, reproduce it: add the failing test first (usually a `Simulator` test that makes the spend or parse the finding describes), and run it with `cargo test -p <crate> <name>`. A finding you can't reproduce is rejected unless you can see from the code that it's right.
- For a claimed missing test, check whether an existing case already covers it.
- For a simplification, check that the suggested helper does what the reviewer thinks.
- For a finding marked "for the user", it's confirmed if the choice really is open: neither the source nor Settled answers it. Handle it as in section 5.

Mark each finding in the review log as **confirmed**, **rejected** (with the evidence), or **deferred** (real but out of scope, with where it's recorded), and add rejected ones to the Brief's Previously rejected list and deferred ones to Out of scope (with where they're recorded), so later rounds don't raise them again. Fix every confirmed blocker and major finding, and minor ones when they're cheap. Each confirmed correctness finding gets a regression test. Nits are optional.

Then run the quick checks for what you touched (`cargo test -p <crate>`, plus `pnpm test` in `napi/` if bindings changed), regenerate the binding files as in section 2, update the Brief, and commit (section 7).

## 5. When to stop

- **Clean**: a round with no confirmed findings at major or above. Stop there; never start a round just for minor findings or nits.
- **Out of rounds**: stop after round 3 even if findings remain, and list them in the report. If round 3's fixes were more than trivial, first run one more round with only the correctness reviewer, to check the fixes; fix what it confirms and report it, without another round. A change that still has major findings after three rounds usually has a design problem that another round won't fix, and the user should look.

Whatever the stop, still run the final checks and open or update the PR, so the user has the work in hand.

**Decisions during the loop.** These don't end the loop or interrupt the user. When a confirmed finding raises a choice that's the user's to confirm (the list is in the reviewer brief), make it as in section 1: keep the current choice or switch, whichever the evidence favours, record it under Decisions as provisional so later reviewers don't raise it again, and add it to Decisions for the user. A reviewer's new argument against a provisional decision goes into that entry, so the user sees both sides. The PR and the report carry every provisional decision, and the user picks at the end.

## 6. Final checks

These are the checks in `.github/workflows/rust.yml` and `pyo3.yml`'s stub check, plus `-D warnings` on clippy (stricter than CI, as the user wants). Fix anything they report:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo machete
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --workspace --exclude chia-wallet-sdk-napi --exclude chia-wallet-sdk-py --exclude chia-wallet-sdk-wasm --exclude chia-sdk-bindings --exclude bindy --exclude bindy-macro --exclude pyo3-stub-generator --exclude chia-sdk-cli --all-features
cargo test --release --workspace --exclude chia-wallet-sdk-napi --exclude chia-sdk-derive --exclude chia-wallet-sdk-py --exclude chia-wallet-sdk-wasm --exclude chia-wallet-sdk-uniffi --exclude chia-sdk-bindings --exclude bindy --exclude bindy-macro --all-features
cargo run -p pyo3-stub-generator; git diff --exit-code pyo3/chia_wallet_sdk.pyi
```

- `rust.yml` also runs the docs build and doctests without `--all-features`, and `cargo check` on each crate with and without features (the `check-crates` job). Run the ones for crates you changed, and all of them if you changed a crate's features or `Cargo.toml`; a feature-gated import is the usual failure.
- If bindings changed: `cargo check -p chia-sdk-bindings -F napi` (and `-F wasm`, `-F pyo3`, `-F uniffi`), `pnpm test` in `napi/` after `pnpm build:debug`, and `git diff --exit-code napi/index.d.ts napi/index.js` after the rebuild.
- CI also runs the WASM tests (`pnpm test` in `wasm/`), the Python tests (`maturin develop && pytest` in `pyo3/` in a venv), and the Go and .NET UniFFI tests (`go/generate.sh`, `dotnet/generate.sh`). Run them if the change is specific to that binding or touches what their tests cover. Say in the report which of these you didn't run.
- If these checks led to changes beyond formatting, say so in the report, since no reviewer saw them.

## 7. Commits, PR and report

**Commits.** One focused commit per logical change. Messages follow the git log: a plain sentence as the subject that says what changed ("Keep comments in suggestions"), or "Address review findings in <area>" with a `-` bulleted body when one round fixes several small things. No prefix like `feat:`. No AI attribution of any kind: no `Co-Authored-By`, no "Generated with" line.

**PR.** Push and open a PR with `gh pr create --base <branch>`, where the branch is Base without the `origin/` prefix (`main`, not `origin/main`), or push to the existing PR when continuing on its branch. The title follows the same style; the body says what changed and why, links the issue ("Closes #n", or "Part of #n" for a tracking issue) or the PR it follows up, and lists the provisional decisions for the user. For stacked work, use `gh stack` rather than setting PR bases by hand: `gh stack submit --auto --open` creates the PRs with generated titles (without `--open` they're drafts), so set each title and body afterwards with `gh pr edit <n> --title ... --body ...`.

**Report.** End with this, in the reply to the user:

```markdown
## <title> — <PR link>

<two or three sentences: what was done, and what the user should look at first>

### Review rounds
- Round 1: <n> findings, <n> confirmed. Caught: <the substantive ones, one line each>
- Round 2: ...
- Stopped because: <clean / out of rounds>

### Rejected findings
- <finding> — <why it was rejected>

### Left for you
- <each provisional decision: what you picked and why, the alternatives, and what switching would take>
- <confirmed findings left unfixed, if the loop ran out of rounds>

### Checks
<each final check and whether it passed; note changes to generated binding files and that they're intended>
```

Keep the state file until the user has seen the report; it's the place to look if they ask why something was decided.
