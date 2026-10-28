use std::sync::{Arc, Mutex};

use chia_sdk_driver::{HashedPtr, SpendContext};
use clvm_traits::ToClvm;
use clvmr::{Allocator, NodePtr};

use crate::{Program, Spend};

pub(crate) trait AsProgram {
    type AsProgram;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram;
}

pub(crate) trait AsPtr {
    type AsPtr;

    fn as_ptr(&self, allocator: &Allocator) -> Self::AsPtr;
}

impl AsProgram for NodePtr {
    type AsProgram = Program;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        Program(clvm.clone(), *self)
    }
}

impl AsProgram for HashedPtr {
    type AsProgram = Program;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        Program(clvm.clone(), self.ptr())
    }
}

impl AsPtr for Program {
    type AsPtr = HashedPtr;

    fn as_ptr(&self, allocator: &Allocator) -> Self::AsPtr {
        HashedPtr::from_ptr(allocator, self.1)
    }
}

impl AsProgram for chia_sdk_driver::Spend {
    type AsProgram = Spend;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        Spend {
            puzzle: Program(clvm.clone(), self.puzzle),
            solution: Program(clvm.clone(), self.solution),
        }
    }
}

/// Allocates each condition, and wraps it in a [`Program`] that shares the context.
pub(crate) fn conditions_to_programs<T>(
    ctx: &mut SpendContext,
    clvm: &Arc<Mutex<SpendContext>>,
    conditions: impl IntoIterator<Item = T>,
) -> bindy::Result<Vec<Program>>
where
    T: ToClvm<Allocator>,
{
    conditions
        .into_iter()
        .map(|condition| Ok(Program(clvm.clone(), ctx.alloc(&condition)?)))
        .collect()
}
