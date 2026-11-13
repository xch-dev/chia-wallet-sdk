use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_sdk_driver::{self as sdk, SpendContext};

use crate::Spend;

use super::Vault;

#[derive(Clone)]
pub struct VaultDryRun {
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
    pub(crate) dry_run: sdk::VaultDryRun,
}

impl VaultDryRun {
    pub fn vault(&self) -> Result<Vault> {
        Ok(self.dry_run.vault.into())
    }

    pub fn cost_margin(&self) -> Result<u64> {
        Ok(self.dry_run.cost_margin())
    }

    pub fn spend(&self, delegated_spend: Spend) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();
        self.dry_run.spend(
            &mut ctx,
            sdk::Spend::new(delegated_spend.puzzle.1, delegated_spend.solution.1),
        )?;
        Ok(())
    }
}
