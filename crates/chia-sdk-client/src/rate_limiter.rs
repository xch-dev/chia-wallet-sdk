use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use chia_protocol::{Message, ProtocolMessageTypes};

use crate::RateLimits;

#[derive(Debug, Clone)]
pub struct RateLimiter {
    incoming: bool,
    reset_seconds: u64,
    period: u64,
    message_counts: HashMap<ProtocolMessageTypes, u32>,
    message_cumulative_sizes: HashMap<ProtocolMessageTypes, u32>,
    limit_factor: f64,
    non_tx_count: u32,
    non_tx_size: u32,
}

impl RateLimiter {
    pub fn new(incoming: bool, reset_seconds: u64, limit_factor: f64) -> Self {
        Self {
            incoming,
            reset_seconds,
            period: time() / reset_seconds,
            message_counts: HashMap::new(),
            message_cumulative_sizes: HashMap::new(),
            limit_factor,
            non_tx_count: 0,
            non_tx_size: 0,
        }
    }

    pub fn handle_message(&mut self, message: &Message, settings: &RateLimits) -> bool {
        let size: u32 = message.data.len().try_into().expect("Message too large");
        let period = time() / self.reset_seconds;

        if self.period != period {
            self.period = period;
            self.message_counts.clear();
            self.message_cumulative_sizes.clear();
            self.non_tx_count = 0;
            self.non_tx_size = 0;
        }

        let new_message_count = self.message_counts.get(&message.msg_type).unwrap_or(&0) + 1;
        let new_cumulative_size = self
            .message_cumulative_sizes
            .get(&message.msg_type)
            .unwrap_or(&0)
            + size;
        let mut new_non_tx_count = self.non_tx_count;
        let mut new_non_tx_size = self.non_tx_size;

        let passed = 'checker: {
            let mut limits = settings.default_settings;

            if let Some(tx_limits) = settings.tx.get(&message.msg_type) {
                limits = *tx_limits;
            } else if let Some(other_limits) = settings.other.get(&message.msg_type) {
                limits = *other_limits;

                new_non_tx_count += 1;
                new_non_tx_size += size;

                if f64::from(new_non_tx_count)
                    > f64::from(settings.non_tx_frequency) * self.limit_factor
                {
                    break 'checker false;
                }

                if f64::from(new_non_tx_size)
                    > f64::from(settings.non_tx_max_total_size) * self.limit_factor
                {
                    break 'checker false;
                }
            }

            let max_total_size = limits
                .max_total_size
                .unwrap_or(limits.frequency * limits.max_size);

            if f64::from(new_message_count) > f64::from(limits.frequency) * self.limit_factor {
                break 'checker false;
            }

            if size > limits.max_size {
                break 'checker false;
            }

            if f64::from(new_cumulative_size) > f64::from(max_total_size) * self.limit_factor {
                break 'checker false;
            }

            true
        };

        if self.incoming || passed {
            *self.message_counts.entry(message.msg_type).or_default() = new_message_count;
            *self
                .message_cumulative_sizes
                .entry(message.msg_type)
                .or_default() = new_cumulative_size;
            self.non_tx_count = new_non_tx_count;
            self.non_tx_size = new_non_tx_size;
        }

        passed
    }
}

fn time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}
