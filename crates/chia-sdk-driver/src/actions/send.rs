use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use chia_sdk_types::Conditions;

use crate::{DriverError, Id, Output, SpendAction, SpendContext, SpendKind, Spends};

#[derive(Debug, Clone, Copy)]
pub struct SendAction {
    pub id: Option<Id>,
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub memos: Memos,
}

impl SendAction {
    pub fn new(id: Option<Id>, puzzle_hash: Bytes32, amount: u64, memos: Memos) -> Self {
        Self {
            id,
            puzzle_hash,
            amount,
            memos,
        }
    }
}

impl SpendAction for SendAction {
    fn spend(&self, ctx: &mut SpendContext, spends: &mut Spends) -> Result<(), DriverError> {
        let output = Output::new(self.puzzle_hash, self.amount);

        let spend = if let Some(id) = self.id {
            let Some(cat) = spends.cats.get_mut(&id) else {
                return Err(DriverError::InvalidAssetId);
            };
            let source = cat.get_source_for_output(ctx, &output)?;
            &mut cat.items[source].kind
        } else {
            let source = spends.xch.get_source_for_output(ctx, &output)?;
            &mut spends.xch.items[source].kind
        };

        match spend {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(Conditions::new().create_coin(
                    self.puzzle_hash,
                    self.amount,
                    self.memos,
                ))?;
            }
        }

        Ok(())
    }
}
