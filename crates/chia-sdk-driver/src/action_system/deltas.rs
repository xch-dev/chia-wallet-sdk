use std::{
    collections::{HashMap, HashSet},
    ops::{Add, AddAssign, Neg},
};

use crate::Id;

#[derive(Debug, Default, Clone)]
pub struct Deltas {
    items: HashMap<Option<Id>, Delta>,
    needed: HashSet<Option<Id>>,
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

    pub fn set_id_needed(&mut self, id: Id) {
        self.needed.insert(Some(id));
    }

    pub fn set_xch_needed(&mut self) {
        self.needed.insert(None);
    }

    pub fn is_needed(&self, id: Id) -> bool {
        self.needed.contains(&Some(id))
    }

    pub fn is_xch_needed(&self) -> bool {
        self.needed.contains(&None)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Delta {
    pub input: u64,
    pub output: u64,
}

impl Delta {
    pub fn new(input: u64, output: u64) -> Self {
        Self { input, output }
    }
}

impl Add for Delta {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            input: self.input + rhs.input,
            output: self.output + rhs.output,
        }
    }
}

impl AddAssign for Delta {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Neg for Delta {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            input: self.output,
            output: self.input,
        }
    }
}
