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
//! Shared Secret Derivation"): a single standard-puzzle coin, or two or more
//! standard-puzzle coins bound into one `ASSERT_CONCURRENT_SPEND` cycle. Sender
//! and scanner must compute the key sum and the smallest coin id over exactly
//! the same coins.
//!
//! [`crate::Spends`] derives the outputs from every XCH coin the transaction
//! spends with the standard puzzle, including intermediate (ephemeral) coins
//! that are created and spent inside it, with one term per coin. When there are
//! two or more such coins the transaction must be prepared with
//! [`crate::Relation::AssertConcurrent`], which binds all of them into one
//! cycle. A silent payment cannot be combined with spends of other assets (CAT,
//! DID, NFT, option) in the same [`crate::Spends`], and its outputs are always
//! created by a coin of the group.
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
pub use block_tweak_data::tweak_data_from_block_spends;
mod scanner;
pub use scanner::{K_MAX_DEFAULT, SilentPaymentScan, scan_from_tweaks};
mod send_keys;
pub(crate) use send_keys::SilentPaymentSecretKeys;
pub use send_keys::{SyntheticPublicKey, SyntheticSecretKey};
mod types;
pub(crate) use types::SilentPaymentPending;
pub use types::{DetectedSpCoin, OutputMeta, TweakData};
