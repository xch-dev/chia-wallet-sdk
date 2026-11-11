use crate::{Deltas, DriverError, SpendAction, SpendContext, Spends};
#[cfg(feature = "chip-0057")]
use crate::{SilentPaymentSendAction, SilentPaymentSettledAction};

/// A CHIP-0057 silent payment action, held by [`Action::SilentPayment`](crate::Action::SilentPayment).
///
/// The type always exists, so that [`Action`](crate::Action) has the same variants whether or not
/// the `chip-0057` feature is enabled. Its own variants exist only with the feature; without it
/// the type has no values.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SilentPaymentAction {
    /// See [`Action::silent_payment_send`](crate::Action::silent_payment_send).
    #[cfg(feature = "chip-0057")]
    Send(SilentPaymentSendAction),
    /// See [`Action::silent_payment_requested`](crate::Action::silent_payment_requested) and
    /// [`Action::silent_payment_settle`](crate::Action::silent_payment_settle).
    #[cfg(feature = "chip-0057")]
    Settled(SilentPaymentSettledAction),
}

#[cfg_attr(not(feature = "chip-0057"), allow(unused_variables))]
impl SpendAction for SilentPaymentAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        match *self {
            #[cfg(feature = "chip-0057")]
            Self::Send(ref action) => action.calculate_delta(deltas, index),
            #[cfg(feature = "chip-0057")]
            Self::Settled(ref action) => action.calculate_delta(deltas, index),
        }
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        match *self {
            #[cfg(feature = "chip-0057")]
            Self::Send(ref action) => action.spend(ctx, spends, index),
            #[cfg(feature = "chip-0057")]
            Self::Settled(ref action) => action.spend(ctx, spends, index),
        }
    }
}
