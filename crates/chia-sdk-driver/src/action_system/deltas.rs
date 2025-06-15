use std::collections::HashMap;

use crate::Id;

#[derive(Debug, Default, Clone)]
pub struct Deltas {
    items: HashMap<Option<Id>, Delta>,
    xch_needed: bool,
}

impl Deltas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: Option<Id>) -> Option<&Delta> {
        self.items.get(&id)
    }

    pub fn update(&mut self, id: Option<Id>) -> &mut Delta {
        self.items.entry(id).or_default()
    }

    pub fn set_xch_needed(&mut self) {
        self.xch_needed = true;
    }

    pub fn is_xch_needed(&self) -> bool {
        self.xch_needed
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Delta {
    pub input: u64,
    pub output: u64,
}

impl Delta {
    pub fn new() -> Self {
        Self::default()
    }
}
