use crate::AssertedRequestedPayment;

/// Aggregate description of the future offer this transaction's pre-split coins enable.
///
/// Per-leg details live on each pre-split child via
/// [`P2PuzzleType::OfferPreSplit`](crate::P2PuzzleType::OfferPreSplit). This struct only carries
/// the cross-leg rollup that isn't visible from a single child — the total fee the offer chain
/// commits to and the notarized payments the offer asserts will be paid back when taken.
///
/// All fields are conditional on the offer actually being taken later. None of this is folded
/// into the main transaction's `fee_paid` / `reserved_fee` / `received_payments`.
#[derive(Debug, Clone)]
pub struct LinkedOffer {
    /// Sum of `ReserveFee` amounts across all of the offer pre-split coins' fixed conditions.
    pub reserved_fee: u64,
    /// Notarized payments asserted by every pre-split leg's fixed conditions, resolved against
    /// the requested payments revealed in this transaction. Only payments that all legs agree on
    /// (and that have a matching reveal) are included.
    pub requested_payments: Vec<AssertedRequestedPayment>,
}
