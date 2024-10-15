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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nil_hashed_ptr() {
        let allocator = Allocator::new();
        let atom = allocator.atom(HashedPtr::NIL.ptr);
        assert!(atom.as_ref().is_empty());

        assert_eq!(HashedPtr::NIL, HashedPtr::NIL);
        assert_eq!(HashedPtr::NIL.ptr(), NodePtr::NIL);
        assert_eq!(
            HashedPtr::NIL.tree_hash(),
            tree_hash(&allocator, NodePtr::NIL)
        );
    }

    #[test]
    fn test_hashed_ptr() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let ptr = ["Hello", " ", "world", "!"].to_clvm(&mut allocator)?;
        let hashed_ptr = HashedPtr::from_ptr(&allocator, ptr);
        assert_eq!(hashed_ptr.ptr(), ptr);
        assert_eq!(hashed_ptr.tree_hash(), tree_hash(&allocator, ptr));
        assert_eq!(hashed_ptr, hashed_ptr);
        assert_eq!(hashed_ptr, HashedPtr::new(ptr, hashed_ptr.tree_hash()));

        Ok(())
    }

    #[test]
    fn test_hashed_ptr_roundtrip() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let ptr = "hello".to_clvm(&mut allocator)?;
        let hashed_ptr = HashedPtr::from_ptr(&allocator, ptr);

        let new_ptr = hashed_ptr.to_clvm(&mut allocator)?;
        assert_eq!(ptr, new_ptr);

        let new_hashed_ptr = HashedPtr::from_clvm(&allocator, new_ptr)?;
        assert_eq!(hashed_ptr, new_hashed_ptr);

        Ok(())
    }

    #[test]
    fn test_hashed_ptr_to_treehash() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let ptr = "hello".to_clvm(&mut allocator)?;
        let hashed_ptr = HashedPtr::from_ptr(&allocator, ptr);
        let tree_hash = ToTreeHash::tree_hash(&hashed_ptr);
        assert_eq!(tree_hash, hashed_ptr.tree_hash());

        Ok(())
    }

    #[test]
    fn test_hashed_ptr_order() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        let mut ptrs = Vec::new();

        for i in 0..5 {
            let ptr = i.to_clvm(&mut allocator)?;
            ptrs.push(HashedPtr::from_ptr(&allocator, ptr));
        }

        ptrs.sort();

        let hashes: Vec<TreeHash> = ptrs.into_iter().map(|ptr| ptr.tree_hash()).collect();

        assert_eq!(
            hashes,
            [
                TreeHash::new(hex!(
                    "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"
                )),
                TreeHash::new(hex!(
                    "9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2"
                )),
                TreeHash::new(hex!(
                    "a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f63222"
                )),
                TreeHash::new(hex!(
                    "a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5"
                )),
                TreeHash::new(hex!(
                    "c79b932e1e1da3c0e098e5ad2c422937eb904a76cf61d83975a74a68fbb04b99"
                ))
            ]
        );

        Ok(())
    }
}
