use chia_protocol::Bytes32;

use crate::{Deltas, DriverError, HashedPtr, Id, SpendAction, SpendContext, Spends};

#[derive(Debug, Clone, Copy)]
pub struct UpdateDidAction {
    pub id: Id,
    pub recovery_list_hash: Option<Bytes32>,
    pub num_verifications_required: u64,
    pub metadata: HashedPtr,
}

impl UpdateDidAction {
    pub fn new(
        id: Id,
        recovery_list_hash: Option<Bytes32>,
        num_verifications_required: u64,
        metadata: HashedPtr,
    ) -> Self {
        Self {
            id,
            recovery_list_hash,
            num_verifications_required,
            metadata,
        }
    }
}

impl SpendAction for UpdateDidAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        let did = deltas.update(Some(self.id));
        did.input += 1;
        did.output += 1;
    }

    fn spend(
        &self,
        _ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        let did = spends
            .dids
            .get_mut(&self.id)
            .ok_or(DriverError::InvalidAssetId)?
            .last_mut()?;

        did.child_info.recovery_list_hash = self.recovery_list_hash;
        did.child_info.num_verifications_required = self.num_verifications_required;
        did.child_info.metadata = self.metadata;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;

    use crate::{Action, BURN_PUZZLE_HASH};

    use super::*;

    #[test]
    fn test_action_update_did() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let metadata = ctx.alloc_hashed(&"Hello, world!")?;
        let hint = ctx.hint(BURN_PUZZLE_HASH)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::create_empty_did(),
                Action::update_did(Id::New(0), Some(Bytes32::default()), 2, metadata),
                Action::burn(Id::New(0), 1, hint),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let did = outputs.dids[&Id::New(0)];
        assert_ne!(sim.coin_state(did.coin.coin_id()), None);
        assert_eq!(did.info.recovery_list_hash, Some(Bytes32::default()));
        assert_eq!(did.info.num_verifications_required, 2);
        assert_eq!(did.info.metadata, metadata);
        assert_eq!(did.info.p2_puzzle_hash, BURN_PUZZLE_HASH);
        assert_eq!(did.coin.amount, 1);

        Ok(())
    }
}
