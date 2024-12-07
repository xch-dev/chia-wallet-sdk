use std::collections::HashMap;

use chia_protocol::Bytes32;
use chia_sdk_types::Memos;
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::DriverError;

#[derive(ToClvm, FromClvm)]
#[clvm(list)]
struct InnerPuzzleMemos<I, R> {
    namespace: (),
    nonce: usize,
    restrictions: Vec<RestrictionMemos<R>>,
    has_children: bool,
    inner_memos: I,
}

#[derive(ToClvm, FromClvm)]
#[clvm(list)]
struct RestrictionMemos<M> {
    is_morpher: bool,
    puzzle_hash: Bytes32,
    memo: M,
}

#[derive(ToClvm, FromClvm)]
#[clvm(list)]
struct MemberMemos<M> {
    puzzle_hash: Bytes32,
    memo: M,
}

#[derive(ToClvm, FromClvm)]
#[clvm(list)]
struct MofNMemos<M> {
    m: usize,
    members: Vec<MemberMemos<M>>,
}
