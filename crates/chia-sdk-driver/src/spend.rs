use clvmr::NodePtr;

#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Spend {
    pub puzzle: NodePtr,
    pub solution: NodePtr,
}

impl Spend {
    pub fn new(puzzle: NodePtr, solution: NodePtr) -> Self {
        Self { puzzle, solution }
    }
}
