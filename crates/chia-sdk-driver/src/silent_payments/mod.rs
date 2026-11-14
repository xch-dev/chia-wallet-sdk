//! Silent payments (CHIP-0057) — wallet-side receive primitive and protocol helpers.
//!
//! This module implements the transport-agnostic scanner described in CHIP-0057.
//! A wallet that receives [`TweakData`] from any source (a future transport
//! protocol, the `chia_sdk_test` simulator helper, or a handcrafted fixture) can
//! detect payments addressed to its scan/spend key pair via [`scan_from_tweaks`].
//!
//! The protocol primitives — [`compute_shared_secret_from_tweak`],
//! [`derive_output_tweak`], [`derive_onetime_pk`], [`derive_onetime_sk`],
//! [`puzzle_hash_for_pk`] — are exposed publicly so the send-side action and any
//! caller that needs to compute shared secrets manually can reuse them without
//! round-tripping through the scanner.
//!
//! Forward compatibility with a future transport protocol: [`TweakData`] has no transport fields
//! (no `height`, no `block_hash`, no JSON envelope). A future transport client
//! constructs `TweakData` from its wire messages without breaking this module's
//! shape.
//!
//! # Spend groups on the send side
//!
//! A silent payment is derived from a *spend group* (CHIP-0057, "Inputs for
//! Shared Secret Derivation", "Spend Groups across Assets"): a single eligible
//! spend, or two or more eligible spends bound into one
//! `ASSERT_CONCURRENT_SPEND` cycle, whatever their asset. Sender and scanner
//! must compute the key sum and the smallest coin id over exactly the same
//! coins.
//!
//! [`crate::Spends`] derives the outputs from the coins the cycle of
//! [`crate::Relation::AssertConcurrent`] binds: every XCH coin the transaction
//! spends with the standard puzzle, including intermediate (ephemeral) coins
//! that are created and spent inside it, plus the first coin of each CAT asset
//! (a CAT, or a revocable CAT on the holder's path, held in the standard
//! puzzle; the other coins of an asset are tied to it by the CAT ring only),
//! with one term per coin. When there are two or more such coins the
//! transaction must be prepared with [`crate::Relation::AssertConcurrent`].
//!
//! Outputs may be XCH ([`crate::Action::silent_payment_send`]) or a CAT the
//! transaction spends ([`crate::Action::silent_payment_send_asset`]). A CAT
//! output is created by the first coin of its asset as the bare one-time puzzle
//! hash, hinted with it; the CAT layer (and the revocation layer) wraps the
//! coin. One `k` counter per scan key runs across every asset, and `K_MAX`
//! counts the outputs in every asset. Change is ordinary change, never a silent
//! payment.
//!
//! Spends outside the CHIP's closed list are rejected before any key is used:
//! DIDs, NFTs and options ([`crate::DriverError::SilentPaymentIneligibleAssetBundle`]),
//! revocable CATs spent through the issuer's hidden path
//! ([`crate::DriverError::SilentPaymentIssuerPathSpend`]) and CATs issued in
//! the same transaction ([`crate::DriverError::SilentPaymentNewCatUnsupported`]).
//!
//! # Scanning CAT outputs
//!
//! The Extension to CATs of CHIP-0057 widens the closed list of eligible
//! spends to a CAT coin and a revocable CAT coin (CHIP-0038, holder's path)
//! held in the standard puzzle, recognized by exact layer matching in
//! [`eligible_spend`]; one `ASSERT_CONCURRENT_SPEND` cycle over XCH and CAT
//! coins is one spend group. A CAT output is the one-time puzzle hash under the
//! [`Wrapper`] of its asset. [`tweak_data_from_block_spends`] gives the block's
//! wrapper list in [`TweakData::wrappers`] (every exact `cat_v2` spend adds its
//! wrapper, [`coin_wrapper`]), [`scan_from_tweaks`] tests every candidate bare
//! and under every wrapper of that list, and a [`DetectedSpCoin`] keeps the
//! bare one-time puzzle hash with the wrapper under which it matched.
//!
//! # Payments through settlement coins
//!
//! In an offer, a party's requested payment is created by a settlement coin,
//! not by a coin of the party (CHIP-0057 "Payments through Settlement
//! Coins"). A party is paid at a silent payment address through the
//! settlement coin of the payment's asset by deriving the one-time puzzle hash
//! from its own spend group, with the same `k` counter as its other outputs,
//! and asserting from a coin of the group the settlement coin's puzzle
//! announcement of the notarized payment that lists it: one notarized payment
//! and one announcement per asset, under the offer's nonce, over all of the
//! party's requested payments in that asset. An XCH payment is listed as
//! `(p amount ())` and a CAT payment as `(p amount (p))`.
//! [`derive_settlement_outputs`] is the CHIP's procedure as a pure function;
//! in a transaction, [`crate::Action::silent_payment_requested`] (the maker,
//! paid by the counterparty's settlement coin) and
//! [`crate::Action::silent_payment_settle`] (the taker, paid by a settlement
//! coin it spends) derive and assert the same values. Nothing offer-specific
//! is needed to scan: the payment is an addition of the block, and a CAT
//! settlement coin adds its wrapper to the block's wrapper list.
//! [`spend_groups_from_block_spends`] gives each spend group of a block with
//! its outputs for light-wallet cut-through, the children of the settlement
//! coins whose announcements the group asserts included; a light-wallet server
//! serves those, while a full scanner keeps matching every addition.
//!
//! All scalar reduction in this module flows through
//! [`chia_sdk_types::silent_payments::ScalarField`], which enforces the
//! unsigned-vs-signed byte-interpretation choice at the type level. See
//! `ScalarField::from_bytes_unsigned` for why unsigned reduction is mandatory
//! for protocol scalars.
//!
//! Hash routines in this module use `chia_sha2::Sha256` exclusively.

mod protocol;
pub use protocol::{
    aggregate_sender_sks, compute_input_hash, compute_shared_secret_from_tweak,
    compute_tweak_point, derive_one_time_puzzle_hash, derive_onetime_pk, derive_onetime_sk,
    derive_output_tweak, puzzle_hash_for_pk,
};
mod layers;
pub use layers::{
    Wrapper, coin_wrapper, coin_wrapper_from_node, eligible_spend, eligible_spend_from_nodes,
    sort_wrappers, wrap_puzzle_hash,
};
mod block_tweak_data;
pub use block_tweak_data::{
    SpendGroup, spend_groups_from_block_spends, tweak_data_from_block_spends,
};
mod scanner;
pub use scanner::{K_MAX_DEFAULT, SilentPaymentScan, scan_from_tweaks};
mod settlement;
pub use settlement::{
    OtherPayment, SettlementDerivation, SettlementGroupCoin, SettlementGroupOutput,
    SettlementNotarizedPayment, SettlementRecipient, derive_settlement_outputs, encoded_payment,
    settlement_announcement, settlement_assertion, settlement_puzzle_hash,
};
mod send_keys;
pub(crate) use send_keys::SilentPaymentSecretKeys;
pub use send_keys::{SyntheticPublicKey, SyntheticSecretKey};
mod types;
pub use types::{DetectedSpCoin, OutputMeta, TweakData};
pub(crate) use types::{SettledOtherPayment, SilentPaymentPending, SilentPaymentSource};
