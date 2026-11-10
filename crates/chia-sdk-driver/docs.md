Drivers for constructing, parsing, and spending the puzzles of coins on the Chia blockchain.

This crate sits between the raw CLVM types (in `chia-sdk-types` and `chia-puzzle-types`) and a
wallet. It knows how standard puzzles are put together, so that you can build spend bundles for
XCH, CATs, DIDs, NFTs, vaults, offers, and more, and parse the coin spends of others.

# Quick start

This example uses [`chia_sdk_test::Simulator`](https://docs.rs/chia-sdk-test), an in-memory
blockchain, to send XCH with the standard puzzle, and then to issue a CAT with the action system.

```
use chia_sdk_driver::{Action, Id, Relation, SpendContext, Spends, StandardLayer};
use chia_sdk_test::Simulator;
use chia_sdk_types::Conditions;
use indexmap::indexmap;

let mut sim = Simulator::new();
let mut ctx = SpendContext::new();

// Alice has a coin of 1,000 mojos with the standard puzzle, and Bob has none
let alice = sim.bls(1_000);
let bob = sim.bls(0);

// Spend Alice's coin, sending 400 mojos to Bob and the rest back to Alice
let alice_p2 = StandardLayer::new(alice.pk);
let bob_hint = ctx.hint(bob.puzzle_hash)?;
let alice_hint = ctx.hint(alice.puzzle_hash)?;

alice_p2.spend(
    &mut ctx,
    alice.coin,
    Conditions::new()
        .create_coin(bob.puzzle_hash, 400, bob_hint)
        .create_coin(alice.puzzle_hash, 600, alice_hint),
)?;

// The coin spends are collected by the context, and need to be signed by Alice
sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

// Now, Alice uses her change to issue 600 of a new CAT, and sends 100 of it to Bob
let change = sim.unspent_coins(alice.puzzle_hash, false)[0];

let mut spends = Spends::new(alice.puzzle_hash);
spends.add(change);

let deltas = spends.apply(
    &mut ctx,
    &[
        Action::single_issue_cat(None, 600),
        Action::send(Id::New(0), bob.puzzle_hash, 100, bob_hint),
    ],
)?;

// Any value that isn't sent elsewhere is returned to Alice as change
let outputs = spends.finish_with_keys(
    &mut ctx,
    &deltas,
    Relation::None,
    &indexmap! { alice.puzzle_hash => alice.pk },
)?;

sim.spend_coins(ctx.take(), &[alice.sk])?;

let cats = &outputs.cats[&Id::New(0)];
assert!(cats.iter().any(|cat| cat.info.p2_puzzle_hash == bob.puzzle_hash && cat.coin.amount == 100));
assert!(cats.iter().any(|cat| cat.info.p2_puzzle_hash == alice.puzzle_hash && cat.coin.amount == 500));
# Ok::<(), anyhow::Error>(())
```

# Spend context

Puzzles and solutions are CLVM values, which are allocated in an [`Allocator`](clvmr::Allocator).
The [`SpendContext`] wraps an allocator, caches the puzzle mods that have been allocated so that
they are only deserialized once, and collects the [`CoinSpend`](chia_protocol::CoinSpend)s of a
transaction. Almost every function in this crate takes a `&mut SpendContext`.

A [`Spend`] is an allocated puzzle and solution that isn't tied to a coin yet. Spends are passed
between layers, used as delegated spends, and finally serialized with [`SpendContext::spend`].

# Puzzles and layers

Chia coins have a puzzle, which controls how they can be spent. The solution is passed to the
puzzle as its arguments, and the output is a list of conditions. Most puzzles are curried, which
means that some arguments are fixed ahead of time. [`Puzzle::parse`] splits a puzzle into its mod
hash and curried arguments, so that it can be identified.

A puzzle consists of multiple layers composed together. A [`Layer`] is a subset of the logic that
makes up a smart coin, which can be constructed into a puzzle and parsed back out of one. Most
layers wrap an inner puzzle (also called an "inner puzzle layer"), and the solution is broken up into
inner solutions in the same way. For example, a CAT is a [`CatLayer`] around a p2 puzzle such as
the [`StandardLayer`], and an NFT is a [`SingletonLayer`] around an [`NftStateLayer`] around an
[`NftOwnershipLayer`].

Generally, you can parse and construct the individual layers separately. This allows them to be
composed together freely. However, there are sometimes additional restraints which limit the ways
they can be mixed. For example, the [`CatLayer`] cannot have another [`CatLayer`] as its inner
puzzle, due to the way it's written. This would create an error when validating the announcements.

## P2 puzzles

A p2 puzzle (meaning "pay to") controls the ownership of the coin. A simple example of this is
the [`P2DelegatedConditionsLayer`], which requires a signature from a single public key and
outputs a list of conditions from the solution.

The "standard transaction" (which is [`p2_delegated_puzzle_or_hidden_puzzle.clsp`]) is a kind of
p2 puzzle that adds additional flexibility. Specifically, support for a hidden puzzle, and usage of
a delegated puzzle instead of directly outputting conditions. It's implemented by the
[`StandardLayer`].

Generally, the p2 puzzle is the base layer in a coin's puzzle, and everything else builds on top
of it to restrict the way it can be spent or attach state. P2 puzzles that can be spent by
outputting conditions implement [`SpendWithConditions`].

# Primitives

Layers deal with individual puzzles, while primitives deal with entire assets. A primitive (such
as [`Cat`], [`Did`], [`Nft`], [`OptionContract`], or [`Vault`]) contains the coin along with all of
the information needed to construct its outer puzzles and spend it, except for the p2 puzzle.
They are designed to be stored in a database, and provide helpers to parse a child from its
parent's coin spend (such as [`Cat::parse_children`]) and to spend the coin with an inner spend.

- [`Cat`] is a fungible token, which can optionally be revocable by its issuer.
- [`Did`], [`Nft`], [`OptionContract`], and [`Vault`] are [`Singleton`]s, which are created with
  a [`Launcher`].
- A [`Vault`] is a singleton whose custody is a MIPS puzzle, made of members (such as keys) and
  restrictions (such as timelocks) combined with m-of-n logic. It's spent with a [`MipsSpend`].
- [`Clawback`], [`ClawbackV2`], [`StreamedAsset`], and [`Bulletin`] are other kinds of coins.
- [`SecureTheBag`] commits to many payments with a single coin, which anyone can later unroll into
  the payments. [`SecuredMint`] uses a bag of pre-launcher coins to secure an NFT bulk mint.
- Some primitives are behind feature flags: `Auction` is an English auction for an NFT,
  `PartialOffer` is a CHIP-0052 offer which can be filled in parts at a fixed price, and `Fusion`
  is a CHIP-0021 singleton which swaps the NFTs on one side for the NFTs on the other. The
  CHIP-0008 `SplitterLayer` isn't a primitive, but it divides a coin between the recipients of a
  fixed payout scheme. A `PlotNft` is a v1.0 singleton which farmers use to self pool, join a
  pool, and leave it through a waiting room, and which claims the pool rewards farmed to it.

# Actions

The action system builds a transaction from high level [`Action`]s, such as sending an asset,
issuing a CAT, minting an NFT, or paying a fee. The selected coins are added to [`Spends`], and the
actions are applied in order. Then, [`Spends::finish_with_keys`] (or [`Spends::prepare`] for
custom p2 puzzles) calculates the change, links the spends together, and returns the [`Outputs`].
See the [`action_system`] module for the details.

# Offers

An [`Offer`] trades the coins that the maker spends to the settlement payments puzzle for the
[`RequestedPayments`] that the taker must make. Use [`Offer::from_input_spend_bundle`] to create an
offer from the maker's spends, [`Offer::from_spend_bundle`] to parse an untrusted offer, and
[`Offer::take`] to complete it with the taker's spends. The [`OfferCoins`] of an offer can be added
to [`Spends`], and [`RequestedPayments::actions`] makes the requested payments. See [`Offer`] for
an example.

# Clear signing

[`parse_vault_transaction`] summarizes what a vault's signature authorizes in a [`VaultTransaction`],
such as the coins it spends, the payments it makes and receives, and the offers it links, so that
a wallet or hardware signer can show the effects of a transaction before it's signed. Information
that can't be verified from the revealed spends is left out.

# Feature flags

- `offer-compression`: Compressing and bech32m encoding offers, with functions such as
  `encode_offer` and `decode_offer`.
- `chip-0008`: The `SplitterLayer` (CHIP-0008), which anyone can spend to divide a coin between
  the recipients of a fixed payout scheme.
- `chip-0021`: NFT fusion (CHIP-0021), with the `Fusion` primitive and the `FusionLayer` and
  `P2FusionLayer`, which swap the NFTs on one side of a fusion for the NFTs on the other.
- `chip-0035`: Data layer stores (CHIP-0035), with the `Datastore` primitive and the
  `DelegationLayer`, `WriterLayer`, and `OracleLayer`.
- `chip-0037`: The `P2ControllerPuzzleLayer` and `P2Eip712MessageLayer` (CHIP-0037), which let
  coins be controlled by another coin, or by an Ethereum wallet that signs EIP-712 messages.
- `chip-0052`: Partial offers (CHIP-0052), with `PartialOfferInfo`, `PartialOffer`, and
  `PartialOfferBundle`. With `offer-compression`, they can also be encoded as `partial1...`
  strings with `encode_partial_offer` and `decode_partial_offer`.
- `pooling`: Plot NFTs for pooling, with the `PlotNft` primitive, `Launcher::mint_plot_nft`, and
  the `SingletonV1Layer`, `PoolMemberLayer`, `PoolWaitingRoomLayer`, and
  `P2SingletonOrDelayedLayer`.
- `action-layer`: The action layer and the apps built on it, such as the `CATalog` and `XCHandles`
  registries and the reward distributor. This also enables `chip-0035`.
- `auction`: English auctions for NFTs, with the `Auction` primitive and
  `Launcher::launch_auction`. This also enables `action-layer`.

[`p2_delegated_puzzle_or_hidden_puzzle.clsp`]: https://github.com/Chia-Network/chia-blockchain/blob/bd022b0c9b0d3e0bc13a0efebba9f22417ca64b5/chia/wallet/puzzles/p2_delegated_puzzle_or_hidden_puzzle.clsp
