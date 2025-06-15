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
