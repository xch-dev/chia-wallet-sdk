use clvm_utils::TreeHash;

/// A restriction on a MIPS inner puzzle, which limits how it can be spent.
///
/// Restrictions are part of the puzzle hash (see [`mips_puzzle_hash`](crate::mips_puzzle_hash)).
/// When spending, the puzzle and solution of each restriction are looked up by
/// [`puzzle_hash`](Self::puzzle_hash) in [`MipsSpend::restrictions`](crate::MipsSpend::restrictions).
#[derive(Debug, Clone, Copy)]
pub struct Restriction {
    /// What the restriction validates.
    pub kind: RestrictionKind,
    /// The tree hash of the restriction puzzle.
    pub puzzle_hash: TreeHash,
}

/// The kind of a MIPS [`Restriction`].
#[derive(Debug, Clone, Copy)]
pub enum RestrictionKind {
    /// A member validator, which is run on the conditions output by the inner puzzle.
    MemberCondition,
    /// A delegated puzzle validator, which is run on the hash of the delegated puzzle.
    DelegatedPuzzleHash,
    /// A wrapper that must be applied around the delegated puzzle, so that it can modify or
    /// check the conditions that the delegated puzzle outputs.
    DelegatedPuzzleWrapper,
}
