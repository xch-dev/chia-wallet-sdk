use chia_protocol::Bytes32;

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
pub struct OutputConstraints {
    pub singleton: bool,
    pub settlement: bool,
}

pub trait OutputSet {
    fn has_output(&self, output: &Output) -> bool;
    fn can_run_cat_tail(&self) -> bool;
    fn missing_singleton_output(&self) -> bool;

    fn find_amount(
        &self,
        puzzle_hash: Bytes32,
        output_constraints: &OutputConstraints,
    ) -> Option<u64> {
        (0..u64::MAX)
            .find(|amount| self.is_allowed(&Output::new(puzzle_hash, *amount), output_constraints))
    }

    fn is_allowed(&self, output: &Output, output_constraints: &OutputConstraints) -> bool {
        if output_constraints.singleton && output.amount % 2 == 1 {
            return false;
        }

        if output_constraints.settlement && output.amount == 0 {
            return false;
        }

        !self.has_output(output)
    }
}
