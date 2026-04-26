use std::{cmp::min, collections::HashSet};

use chia_protocol::Bytes32;

use crate::DriverError;

#[derive(Debug, Default, Clone)]
pub struct Facts {
    actual_expiration_time: Option<u64>,
    required_expiration_time: Option<u64>,
    reserved_fees: u128,
    asserted_puzzle_announcements: HashSet<Bytes32>,
}

impl Facts {
    pub fn extend(&mut self, other: &Facts) -> Result<(), DriverError> {
        if let Some(time) = other.actual_expiration_time {
            self.update_actual_expiration_time(time);
        }

        if let Some(time) = other.required_expiration_time {
            self.update_required_expiration_time(time);
        }

        self.reserved_fees += other.reserved_fees;

        self.asserted_puzzle_announcements
            .extend(other.asserted_puzzle_announcements.clone());

        Ok(())
    }

    pub fn actual_expiration_time(&self) -> Option<u64> {
        self.actual_expiration_time
    }

    pub fn required_expiration_time(&self) -> Option<u64> {
        self.required_expiration_time
    }

    pub fn reserved_fees(&self) -> u128 {
        self.reserved_fees
    }

    pub fn is_puzzle_announcement_asserted(&self, announcement_id: Bytes32) -> bool {
        self.asserted_puzzle_announcements
            .contains(&announcement_id)
    }

    /// Updates the transaction's expiration time to the minimum of the current expiration time and
    /// the given time. This is used to ensure that the transaction will not be valid after the given
    /// time (i.e., after a clawback expires).
    pub fn update_actual_expiration_time(&mut self, expiration_time: u64) {
        if let Some(old_time) = self.actual_expiration_time {
            self.actual_expiration_time = Some(min(old_time, expiration_time));
        } else {
            self.actual_expiration_time = Some(expiration_time);
        }
    }

    /// Updates the required expiration time to the minimum of the current required expiration time and
    /// the given time. This is used to ensure that the transaction will not be valid after the given
    /// time (i.e., after a clawback expires).
    pub fn update_required_expiration_time(&mut self, required_expiration_time: u64) {
        if let Some(old_time) = self.required_expiration_time {
            self.required_expiration_time = Some(min(old_time, required_expiration_time));
        } else {
            self.required_expiration_time = Some(required_expiration_time);
        }
    }

    /// Adds to the total reserved fees, from coins that have been validated to be linked.
    pub fn add_reserved_fees(&mut self, amount: u64) {
        self.reserved_fees += u128::from(amount);
    }

    /// Adds an announcement id to the set of asserted puzzle announcements.
    pub fn assert_puzzle_announcement(&mut self, announcement_id: Bytes32) {
        self.asserted_puzzle_announcements.insert(announcement_id);
    }
}
