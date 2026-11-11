//! Silent payments (CHIP-0057): sending, scanning and payments through settlement coins.
//!
//! - **Sending.** [`crate::Action::silent_payment_send`] records an output in XCH or in a CAT the
//!   transaction spends. [`crate::Spends::prepare_with_silent_payment_keys`] derives the outputs
//!   from the spend group: every XCH coin spent with the standard puzzle and the first coin of
//!   each CAT asset, which [`crate::Relation::AssertConcurrent`] binds into one cycle. Spends
//!   outside the CHIP's closed list of eligible spends are rejected.
//! - **Scanning.** [`tweak_data_from_block_spends`] forms a block's spend groups, tweak points
//!   and wrapper list, and [`scan_from_tweaks`] matches the outputs with the scan secret key and
//!   the spend public key. [`TweakData`] has no transport fields.
//! - **Settlement coins.** [`derive_settlement_outputs`] is the CHIP's `DeriveSettlementOutputs`;
//!   [`crate::Action::silent_payment_requested`] and [`crate::Action::silent_payment_settle`]
//!   derive the same values in a transaction. [`spend_groups_from_block_spends`] gives each
//!   group's outputs for a light-wallet server.
//!
//! Protocol scalars are reduced through [`chia_sdk_types::silent_payments::ScalarField`], which
//! fixes the unsigned interpretation of hash output.

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
pub use send_keys::{SyntheticPublicKey, SyntheticSecretKey};
mod state;
pub use state::SilentPaymentState;
mod types;
pub use types::{DetectedSilentPaymentCoin, OutputMeta, TweakData};
pub(crate) use types::{SettledOtherPayment, SilentPaymentPending, SilentPaymentSource};
