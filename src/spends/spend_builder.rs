use chia_protocol::CoinSpend;
use clvmr::NodePtr;

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
