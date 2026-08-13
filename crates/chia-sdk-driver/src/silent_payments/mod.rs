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
//! # Multi-input send constraint
//!
//! A multi-input silent-payment bundle must consist of distinct-puzzle-hash,
//! non-ephemeral XCH inputs only — no CAT, DID, NFT, or intermediate
//! (ephemeral) coins. The sender binds those inputs into one strongly connected
//! component with [`crate::Relation::AssertConcurrent`], and the receiver
//! reconstructs the spend group as the set of same-`AssertConcurrent`-cycle
//! coins. For the receiver's reconstructed input set to equal the sender's exact
//! input set — and therefore for the two `input_hash` values to agree — the
//! `AssertConcurrent` cycle must span precisely the XCH inputs the sender
//! aggregated. If the cycle includes any other coin, the receiver's `input_hash`
//! diverges from the sender's and the output coin is undetectable by the
//! recipient. this implementation enforces this documented constraint; sending to a silent-payment
//! destination across mixed asset types or with extra cycle members is out of
//! scope.
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
    derive_one_time_puzzle_hash, derive_onetime_pk, derive_onetime_sk, derive_output_tweak,
    puzzle_hash_for_pk,
};
mod block_tweak_data;
pub use block_tweak_data::tweak_data_from_block_spends;
mod scanner;
pub use scanner::{K_MAX_DEFAULT, SilentPaymentScan, scan_from_tweaks};
mod send_keys;
pub use send_keys::{SyntheticPublicKey, SyntheticSecretKey};
mod types;
pub(crate) use types::SilentPaymentPending;
pub use types::{DetectedSpCoin, OutputMeta, TweakData};
