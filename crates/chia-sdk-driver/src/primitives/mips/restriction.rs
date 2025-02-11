use clvm_utils::TreeHash;

#[derive(Debug, Clone, Copy)]
pub struct Restriction {
    pub kind: RestrictionKind,
    pub puzzle_hash: TreeHash,
}

#[derive(Debug, Clone, Copy)]
pub enum RestrictionKind {
    MemberCondition,
    DelegatedPuzzleHash,
    DelegatedPuzzleWrapper,
}
