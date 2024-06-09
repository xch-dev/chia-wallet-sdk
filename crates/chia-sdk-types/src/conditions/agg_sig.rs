use chia_bls::PublicKey;
use chia_protocol::Bytes;
use clvm_traits::{apply_constants, FromClvm, ToClvm};

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AggSig {
    pub kind: AggSigKind,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSig {
    pub fn new(kind: AggSigKind, public_key: PublicKey, message: Bytes) -> Self {
        Self {
            kind,
            public_key,
            message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm, Hash)]
#[repr(u8)]
#[clvm(atom)]
pub enum AggSigKind {
    Parent = 43,
    Puzzle = 44,
    Amount = 45,
    PuzzleAmount = 46,
    ParentAmount = 47,
    ParentPuzzle = 48,
    Unsafe = 49,
    Me = 50,
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct AggSigParent {
    #[clvm(constant = 43)]
    pub opcode: u8,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSigParent {
    pub fn new(public_key: PublicKey, message: Bytes) -> Self {
        Self {
            public_key,
            message,
        }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct AggSigPuzzle {
    #[clvm(constant = 44)]
    pub opcode: u8,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSigPuzzle {
    pub fn new(public_key: PublicKey, message: Bytes) -> Self {
        Self {
            public_key,
            message,
        }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct AggSigAmount {
    #[clvm(constant = 45)]
    pub opcode: u8,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSigAmount {
    pub fn new(public_key: PublicKey, message: Bytes) -> Self {
        Self {
            public_key,
            message,
        }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct AggSigPuzzleAmount {
    #[clvm(constant = 46)]
    pub opcode: u8,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSigPuzzleAmount {
    pub fn new(public_key: PublicKey, message: Bytes) -> Self {
        Self {
            public_key,
            message,
        }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct AggSigParentAmount {
    #[clvm(constant = 47)]
    pub opcode: u8,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSigParentAmount {
    pub fn new(public_key: PublicKey, message: Bytes) -> Self {
        Self {
            public_key,
            message,
        }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct AggSigParentPuzzle {
    #[clvm(constant = 48)]
    pub opcode: u8,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSigParentPuzzle {
    pub fn new(public_key: PublicKey, message: Bytes) -> Self {
        Self {
            public_key,
            message,
        }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct AggSigUnsafe {
    #[clvm(constant = 49)]
    pub opcode: u8,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSigUnsafe {
    pub fn new(public_key: PublicKey, message: Bytes) -> Self {
        Self {
            public_key,
            message,
        }
    }
}

#[derive(ToClvm, FromClvm)]
#[apply_constants]
#[derive(Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct AggSigMe {
    #[clvm(constant = 50)]
    pub opcode: u8,
    pub public_key: PublicKey,
    pub message: Bytes,
}

impl AggSigMe {
    pub fn new(public_key: PublicKey, message: Bytes) -> Self {
        Self {
            public_key,
            message,
        }
    }
}
