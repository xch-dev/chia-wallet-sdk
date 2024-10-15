use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;

use crate::Condition;

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
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T> Conditions<T> {
    pub fn with(mut self, condition: impl Into<Condition<T>>) -> Self {
        self.conditions.push(condition.into());
        self
    }

    pub fn extend(mut self, conditions: impl IntoIterator<Item = impl Into<Condition<T>>>) -> Self {
        self.conditions
            .extend(conditions.into_iter().map(Into::into));
        self
    }

    pub fn extend_from_slice(mut self, conditions: &[Condition<T>]) -> Self
    where
        T: Clone,
    {
        self.conditions.extend_from_slice(conditions);
        self
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
