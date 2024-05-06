use chia_protocol::CoinSpend;
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

#[derive(Debug, Clone)]
pub struct ChainedSpend {
    pub coin_spends: Vec<CoinSpend>,
    pub parent_conditions: Vec<NodePtr>,
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
