use clvmr::NodePtr;

pub trait Chainable {
    fn condition(self, condition: NodePtr) -> Self;
    fn chain(self, other: ChainedSpend) -> Self;

    fn conditions(mut self, conditions: impl IntoIterator<Item = NodePtr>) -> Self
    where
        Self: Sized,
    {
        for condition in conditions {
            self = self.condition(condition);
        }
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct ChainedSpend {
    pub parent_conditions: Vec<NodePtr>,
}

impl ChainedSpend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extend(&mut self, other: ChainedSpend) {
        self.parent_conditions.extend(other.parent_conditions);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InnerSpend {
    puzzle: NodePtr,
    solution: NodePtr,
}

impl InnerSpend {
    pub fn new(puzzle: NodePtr, solution: NodePtr) -> Self {
        Self { puzzle, solution }
    }

    pub fn puzzle(&self) -> NodePtr {
        self.puzzle
    }

    pub fn solution(&self) -> NodePtr {
        self.solution
    }
}
