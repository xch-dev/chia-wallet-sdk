use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};

#[derive(Debug, Clone)]
pub struct VaultMemos<R, P> {
    pub nonce: usize,
    pub restriction_hints: Vec<RestrictionHint<R>>,
    pub puzzle_hint: P,
}

#[derive(Debug, Clone, Copy, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RestrictionHint<T> {
    pub member_not_delegated_puzzle: bool,
    pub puzzle_hash: Bytes32,
    pub memo: T,
}
