use clvmr::NodePtr;

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Spend {
    puzzle: NodePtr,
    solution: NodePtr,
}

impl Spend {
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
