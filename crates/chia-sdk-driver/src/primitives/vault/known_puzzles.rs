use std::collections::HashMap;

use clvm_utils::TreeHash;

use super::{MemberKind, RestrictionKind};

#[derive(Debug, Default, Clone)]
pub struct KnownPuzzles {
    pub restrictions: HashMap<TreeHash, RestrictionKind>,
    pub members: HashMap<TreeHash, MemberKind>,
}

impl KnownPuzzles {
    pub fn new() -> Self {
        Self::default()
    }
}
