use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_driver::{Bulletin, BulletinLayer};
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};

use crate::{Clvm, Program, Spend};

pub trait BulletinExt {
    fn conditions(&self, clvm: Clvm) -> Result<Vec<Program>>;
    fn spend(&self, spend: Spend) -> Result<()>;
}

impl BulletinExt for Bulletin {
    fn conditions(&self, clvm: Clvm) -> Result<Vec<Program>> {
        let mut ctx = clvm.0.lock().unwrap();

        let conditions = self.conditions(&mut ctx)?;

        let mut programs = Vec::new();

        for condition in conditions {
            let ptr = condition.to_clvm(&mut ctx)?;
            programs.push(Program(clvm.0.clone(), ptr));
        }

        Ok(programs)
    }

    fn spend(&self, spend: Spend) -> Result<()> {
        let mut ctx = spend.puzzle.0.lock().unwrap();

        self.spend(
            &mut ctx,
            chia_sdk_driver::Spend::new(spend.puzzle.1, spend.solution.1),
        )?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct CreatedBulletin {
    pub bulletin: Bulletin,
    pub parent_conditions: Vec<Program>,
}

pub fn bulletin_puzzle_hash(hidden_puzzle_hash: Bytes32) -> Result<Bytes32> {
    Ok(BulletinLayer::new(TreeHash::from(hidden_puzzle_hash))
        .tree_hash()
        .into())
}
