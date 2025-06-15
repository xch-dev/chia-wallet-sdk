use std::collections::HashSet;

use chia_protocol::Bytes32;
use chia_puzzles::SINGLETON_LAUNCHER_HASH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Output {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
}

impl Output {
    pub fn new(puzzle_hash: Bytes32, amount: u64) -> Self {
        Self {
            puzzle_hash,
            amount,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputConstraint {
    Singleton,
    Settlement,
}

impl OutputConstraint {
    pub fn is_allowed(&self, output: &Output, has_singleton_output: bool) -> bool {
        match self {
            Self::Singleton => output.amount % 2 == 0 && !has_singleton_output,
            Self::Settlement => output.amount > 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputSet {
    constraints: Vec<OutputConstraint>,
    outputs: HashSet<Output>,
    reserve_fee: u64,
    melted: bool,
}

impl OutputSet {
    pub fn new(constraints: Vec<OutputConstraint>) -> Self {
        Self {
            constraints,
            outputs: HashSet::new(),
            reserve_fee: 0,
            melted: false,
        }
    }

    pub fn amount(&self) -> u64 {
        self.reserve_fee
            + self
                .outputs
                .iter()
                .fold(0, |acc, output| acc + output.amount)
    }

    pub fn reserve_fee(&mut self, amount: u64) {
        self.reserve_fee += amount;
    }

    pub fn melt(&mut self) {
        self.melted = true;
    }

    pub fn constraints(&self) -> &[OutputConstraint] {
        &self.constraints
    }

    pub fn launcher_amount(&self) -> Option<u64> {
        (0..u64::MAX)
            .find(|&amount| self.is_allowed(&Output::new(SINGLETON_LAUNCHER_HASH.into(), amount)))
    }

    pub fn is_allowed(&self, output: &Output) -> bool {
        for constraint in &self.constraints {
            if !constraint.is_allowed(output, self.has_singleton_output()) {
                return false;
            }
        }

        !self.outputs.contains(output)
    }

    pub fn has_singleton_output(&self) -> bool {
        self.melted || self.outputs.iter().any(|output| output.amount % 2 == 1)
    }

    pub fn insert(&mut self, output: Output) {
        self.outputs.insert(output);
    }
}
