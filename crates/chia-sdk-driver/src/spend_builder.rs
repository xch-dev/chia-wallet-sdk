use clvmr::NodePtr;

pub trait Chainable {
    fn condition(self, condition: NodePtr) -> Self;
    fn chain(self, other: ChainedSpend) -> Self;
}

#[derive(Debug, Default, Clone)]
pub struct ChainedSpend {
    parent_conditions: Vec<NodePtr>,
}

impl ChainedSpend {
    pub fn new(parent_conditions: Vec<NodePtr>) -> Self {
        Self { parent_conditions }
    }

    pub fn extend(&mut self, other: ChainedSpend) {
        self.parent_conditions.extend(other.parent_conditions);
    }

    pub fn parent_condition(&mut self, condition: NodePtr) {
        self.parent_conditions.push(condition);
    }

    pub fn parent_conditions(&self) -> &[NodePtr] {
        &self.parent_conditions
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
