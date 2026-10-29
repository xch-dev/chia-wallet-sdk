use std::{
    collections::{HashMap, HashSet},
    ops::{Add, AddAssign, Neg},
};

use crate::{Action, Id, SpendAction};

/// The total amount of each asset that the actions consume and produce.
///
/// This is intended to be calculated before coin selection, so that the caller knows how much of
/// each asset needs to be added to the [`Spends`](crate::Spends) before the actions are applied.
/// For each id, the caller should select at least `output - input` worth of coins. If the result
/// is zero but [`Deltas::is_needed`] returns true, at least one coin must still be selected (for
/// example, the singleton being updated, or an XCH coin to create a launcher from).
#[derive(Debug, Default, Clone)]
pub struct Deltas {
    items: HashMap<Id, Delta>,
    needed: HashSet<Id>,
}

impl Deltas {
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculates the deltas of a list of actions, which must be the same list (in the same order)
    /// that is later passed to [`Spends::apply`](crate::Spends::apply), since [`Id::New`] refers to
    /// actions by index.
    pub fn from_actions(actions: &[Action]) -> Self {
        let mut deltas = Self::new();
        for (index, action) in actions.iter().enumerate() {
            action.calculate_delta(&mut deltas, index);
        }
        deltas
    }

    /// Every id that the actions refer to, including those with a zero delta.
    pub fn ids(&self) -> impl Iterator<Item = &Id> {
        self.items.keys()
    }

    pub fn get(&self, id: &Id) -> Option<&Delta> {
        self.items.get(id)
    }

    /// The delta for the id, which is created as zero if it doesn't exist yet.
    pub fn update(&mut self, id: Id) -> &mut Delta {
        self.items.entry(id).or_default()
    }

    /// Marks that at least one coin of the asset must be selected, even if the delta is zero.
    pub fn set_needed(&mut self, id: Id) {
        self.needed.insert(id);
        self.items.entry(id).or_default();
    }

    /// Whether at least one coin of the asset must be selected, even if the delta is zero.
    pub fn is_needed(&self, id: &Id) -> bool {
        self.needed.contains(id)
    }
}

/// The amount of an asset that is added to (input) and removed from (output) the transaction.
///
/// These are [`u128`] because the total of many coin amounts can exceed [`u64::MAX`], for example
/// in a malicious offer. Such a transaction fails with [`DriverError::InsufficientFunds`] or
/// [`DriverError::AmountOverflow`] rather than wrapping around.
///
/// [`DriverError::InsufficientFunds`]: crate::DriverError::InsufficientFunds
/// [`DriverError::AmountOverflow`]: crate::DriverError::AmountOverflow
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Delta {
    /// The amount the actions add, for example by issuing a CAT or melting a singleton into XCH.
    pub input: u128,
    /// The amount the actions remove, for example by sending it or paying a fee.
    pub output: u128,
}

impl Delta {
    pub fn new(input: u64, output: u64) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
        }
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
