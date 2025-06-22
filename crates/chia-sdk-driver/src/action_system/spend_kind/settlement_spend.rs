use std::collections::HashSet;

use chia_puzzle_types::offer::NotarizedPayment;

use crate::{Output, OutputSet};

#[derive(Debug, Default, Clone)]
pub struct SettlementSpend {
    notarized_payments: Vec<NotarizedPayment>,
    outputs: HashSet<Output>,
}

impl SettlementSpend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_notarized_payment(&mut self, notarized_payment: NotarizedPayment) {
        for payment in &notarized_payment.payments {
            self.outputs
                .insert(Output::new(payment.puzzle_hash, payment.amount));
        }

        self.notarized_payments.push(notarized_payment);
    }

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
