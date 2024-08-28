use std::{cmp::Ordering, fmt};

use clvm_traits::{FromClvm, FromClvmError, ToClvm, ToClvmError};
use clvm_utils::{tree_hash, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use hex_literal::hex;

#[derive(Clone, Copy, Eq)]
pub struct HashedPtr {
    ptr: NodePtr,
    tree_hash: TreeHash,
}

impl HashedPtr {
    pub const NIL: Self = Self {
        ptr: NodePtr::NIL,
        tree_hash: TreeHash::new(hex!(
            "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"
        )),
    };

    pub fn new(ptr: NodePtr, tree_hash: TreeHash) -> Self {
        Self { ptr, tree_hash }
    }

    pub fn from_ptr(allocator: &Allocator, ptr: NodePtr) -> Self {
        Self::new(ptr, tree_hash(allocator, ptr))
    }

    pub fn ptr(&self) -> NodePtr {
        self.ptr
    }

    pub fn tree_hash(&self) -> TreeHash {
        self.tree_hash
    }
}

impl fmt::Debug for HashedPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HashedPtr({})", self.tree_hash)
    }
}

impl fmt::Display for HashedPtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.tree_hash)
    }
}

impl PartialEq for HashedPtr {
    fn eq(&self, other: &Self) -> bool {
        self.tree_hash == other.tree_hash
    }
}

impl PartialOrd for HashedPtr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.tree_hash.cmp(&other.tree_hash))
    }
}

impl Ord for HashedPtr {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tree_hash.cmp(&other.tree_hash)
    }
}

impl ToClvm<Allocator> for HashedPtr {
    fn to_clvm(&self, _encoder: &mut Allocator) -> Result<NodePtr, ToClvmError> {
        Ok(self.ptr)
    }
}

impl FromClvm<Allocator> for HashedPtr {
    fn from_clvm(decoder: &Allocator, node: NodePtr) -> Result<Self, FromClvmError> {
        Ok(Self::from_ptr(decoder, node))
    }
}

impl ToTreeHash for HashedPtr {
    fn tree_hash(&self) -> TreeHash {
        self.tree_hash
    }
}
