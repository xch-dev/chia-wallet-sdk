use std::ops::Index;

use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;

use super::Condition;

/// A grow-only list of conditions which can be used when building spend bundles.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(transparent)]
pub struct Conditions<T = NodePtr> {
    conditions: Vec<Condition<T>>,
}

impl<T> Default for Conditions<T> {
    fn default() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }
}

impl Conditions<NodePtr> {
    /// Create a new empty list of conditions. To make inference easier for the compiler,
    /// the generic type defaults to [`NodePtr`], since that's the most general choice
    /// and common when building spend bundles.
    ///
    /// If you need to create an instance with a different generic type, use [`Conditions::default`] instead.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T> Conditions<T> {
    /// Gets the number of conditions.
    pub fn len(&self) -> usize {
        self.conditions.len()
    }

    /// Checks if there are no conditions.
    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }

    /// Gets an iterator over the conditions.
    pub fn iter(&self) -> impl Iterator<Item = &Condition<T>> {
        self.conditions.iter()
    }

    /// Converts the list of conditions into a vector.
    pub fn into_vec(self) -> Vec<Condition<T>> {
        self.conditions
    }

    /// Adds a condition and returns the updated list.
    pub fn with(mut self, condition: impl Into<Condition<T>>) -> Self {
        self.conditions.push(condition.into());
        self
    }

    /// Appends a list of conditions to the end from an iterator.
    pub fn extend(mut self, conditions: impl IntoIterator<Item = impl Into<Condition<T>>>) -> Self {
        self.conditions
            .extend(conditions.into_iter().map(Into::into));
        self
    }

    /// Appends a list of conditions to the end from a slice.
    pub fn extend_from_slice(mut self, conditions: &[Condition<T>]) -> Self
    where
        T: Clone,
    {
        self.conditions.extend_from_slice(conditions);
        self
    }

    /// Adds a condition to the end of the list.
    pub fn push(&mut self, condition: impl Into<Condition<T>>) {
        self.conditions.push(condition.into());
    }
}

impl<T> Index<usize> for Conditions<T> {
    type Output = Condition<T>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.conditions[index]
    }
}

impl<T> AsRef<[Condition<T>]> for Conditions<T> {
    fn as_ref(&self) -> &[Condition<T>] {
        &self.conditions
    }
}

impl<T> IntoIterator for Conditions<T> {
    type Item = Condition<T>;
    type IntoIter = std::vec::IntoIter<Condition<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.conditions.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a Conditions<T> {
    type Item = &'a Condition<T>;
    type IntoIter = std::slice::Iter<'a, Condition<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.conditions.iter()
    }
}

impl<T> From<Vec<Condition<T>>> for Conditions<T> {
    fn from(conditions: Vec<Condition<T>>) -> Self {
        Self { conditions }
    }
}
