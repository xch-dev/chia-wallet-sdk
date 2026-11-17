//! Silent payments (CHIP-0057). Sending: [`crate::Action::silent_payment_send`] and
//! [`crate::Spends::prepare_with_silent_payment_keys`]. Scanning:
//! [`tweak_data_from_block_spends`] and [`scan_from_tweaks`]. Payments through settlement
//! coins: [`derive_settlement_outputs`], [`crate::Action::silent_payment_requested`] and
//! [`crate::Action::silent_payment_settle`].

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
