use std::collections::HashSet;

use chia_puzzle_types::offer::NotarizedPayment;

use crate::{Output, OutputSet};

/// The notarized payments that a settlement coin will make, and the coins they create.
///
/// Anyone can spend a settlement coin, so the payments it makes are asserted by a conditions spend
/// in the same transaction (see [`SpendKind::create_coin_with_assertion`](crate::SpendKind::create_coin_with_assertion)).
#[derive(Debug, Default, Clone)]
pub struct SettlementSpend {
    notarized_payments: Vec<NotarizedPayment>,
    outputs: HashSet<Output>,
}

impl SettlementSpend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends the notarized payment, and records the coins it creates as outputs of the spend.
    pub fn add_notarized_payment(&mut self, notarized_payment: NotarizedPayment) {
        for payment in &notarized_payment.payments {
            self.outputs
                .insert(Output::new(payment.puzzle_hash, payment.amount));
        }

        self.notarized_payments.push(notarized_payment);
    }

    /// The notarized payments for the settlement payments puzzle's solution.
    pub fn finish(self) -> Vec<NotarizedPayment> {
        self.notarized_payments
    }
}

impl OutputSet for SettlementSpend {
    fn has_output(&self, output: &Output) -> bool {
        self.outputs.contains(output)
    }

    fn can_run_cat_tail(&self) -> bool {
        false
    }

    fn missing_singleton_output(&self) -> bool {
        !self.outputs.iter().any(|output| output.amount % 2 == 1)
    }
}
