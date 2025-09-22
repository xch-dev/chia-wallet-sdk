use std::{
    collections::{HashMap, HashSet},
    ops::{Add, AddAssign, Neg},
};

use crate::{Action, Id, SpendAction};

#[derive(Debug, Default, Clone)]
pub struct Deltas {
    items: HashMap<Id, Delta>,
    needed: HashSet<Id>,
}

impl Deltas {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_actions(actions: &[Action]) -> Self {
        let mut deltas = Self::new();
        for (index, action) in actions.iter().enumerate() {
            action.calculate_delta(&mut deltas, index);
        }
        deltas
    }

    pub fn ids(&self) -> impl Iterator<Item = &Id> {
        self.items.keys()
    }

    pub fn get(&self, id: &Id) -> Option<&Delta> {
        self.items.get(id)
    }

    pub fn update(&mut self, id: Id) -> &mut Delta {
        self.items.entry(id).or_default()
    }

    pub fn set_needed(&mut self, id: Id) {
        self.needed.insert(id);
    }

    pub fn is_needed(&self, id: &Id) -> bool {
        self.needed.contains(id)
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
