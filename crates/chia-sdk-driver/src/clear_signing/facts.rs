use std::{
    cmp::{max, min},
    collections::HashSet,
};

use chia_protocol::Bytes32;

/// Conditions that are known to be output if the transaction is valid, collected from the vault spend and the
/// verified spends while parsing a [`VaultTransaction`](crate::VaultTransaction).
///
/// Only conditions whose output is guaranteed by the vault's signature should be recorded here, since they're used
/// to decide which settlement payments and child spends are asserted, how much fee is reserved, and when the
/// transaction expires.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    actual_expiration_time: Option<u64>,
    actual_earliest_time: Option<u64>,
    required_expiration_time: Option<u64>,
    reserved_fees: u128,
    asserted_puzzle_announcements: HashSet<Bytes32>,
    asserted_spends: HashSet<Bytes32>,
}

impl Facts {
    /// Records an `ASSERT_BEFORE_SECONDS_ABSOLUTE`, keeping the earliest time. The transaction is invalid at or
    /// after this time.
    pub fn update_actual_expiration_time(&mut self, expiration_time: u64) {
        if let Some(old_time) = self.actual_expiration_time {
            self.actual_expiration_time = Some(min(old_time, expiration_time));
        } else {
            self.actual_expiration_time = Some(expiration_time);
        }
    }

    /// Updates the time the transaction can be valid from to the maximum of the current earliest time and
    /// the given time. This is used to ensure that the transaction will not be valid before the given
    /// time (i.e., before a clawback expires).
    pub fn update_actual_earliest_time(&mut self, earliest_time: u64) {
        if let Some(old_time) = self.actual_earliest_time {
            self.actual_earliest_time = Some(max(old_time, earliest_time));
        } else {
            self.actual_earliest_time = Some(earliest_time);
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

    /// Adds a `RESERVE_FEE` amount to the total, from coins that have been validated to be linked.
    pub fn add_reserved_fees(&mut self, amount: u64) {
        self.reserved_fees += u128::from(amount);
    }

    /// Adds an announcement id to the set of asserted puzzle announcements.
    pub fn assert_puzzle_announcement(&mut self, announcement_id: Bytes32) {
        self.asserted_puzzle_announcements.insert(announcement_id);
    }

    /// Adds a coin id to the set of spends asserted with `ASSERT_CONCURRENT_SPEND`.
    pub fn assert_spend(&mut self, coin_id: Bytes32) {
        self.asserted_spends.insert(coin_id);
    }

    /// The earliest asserted expiration time, or `None` if the transaction never expires.
    pub fn actual_expiration_time(&self) -> Option<u64> {
        self.actual_expiration_time
    }

    /// The latest asserted `ASSERT_SECONDS_ABSOLUTE` time, before which the transaction is invalid, or `None` if
    /// there's no such assertion.
    pub fn actual_earliest_time(&self) -> Option<u64> {
        self.actual_earliest_time
    }

    /// The earliest time the transaction must expire by, or `None` if there's no requirement.
    pub fn required_expiration_time(&self) -> Option<u64> {
        self.required_expiration_time
    }

    /// The total of the recorded reserved fees.
    pub fn reserved_fees(&self) -> u128 {
        self.reserved_fees
    }

    /// Whether the puzzle announcement is asserted.
    pub fn is_puzzle_announcement_asserted(&self, announcement_id: Bytes32) -> bool {
        self.asserted_puzzle_announcements
            .contains(&announcement_id)
    }

    /// Whether the coin's spend is asserted with `ASSERT_CONCURRENT_SPEND`.
    pub fn is_spend_asserted(&self, coin_id: Bytes32) -> bool {
        self.asserted_spends.contains(&coin_id)
    }
}
