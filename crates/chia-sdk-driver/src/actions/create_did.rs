use chia_protocol::Bytes32;

use crate::{
    Asset, Deltas, DriverError, HashedPtr, Id, SingletonSpends, SpendAction, SpendContext,
    SpendKind, Spends,
};

#[derive(Debug, Clone, Copy)]
pub struct CreateDidAction {
    pub recovery_list_hash: Option<Bytes32>,
    pub num_verifications_required: u64,
    pub metadata: HashedPtr,
    pub amount: u64,
}

impl CreateDidAction {
    pub fn new(
        recovery_list_hash: Option<Bytes32>,
        num_verifications_required: u64,
        metadata: HashedPtr,
        amount: u64,
    ) -> Self {
        Self {
            recovery_list_hash,
            num_verifications_required,
            metadata,
            amount,
        }
    }
}

impl Default for CreateDidAction {
    fn default() -> Self {
        Self::new(None, 1, HashedPtr::NIL, 1)
    }
}

impl SpendAction for CreateDidAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        deltas.update(Id::New(index)).input += self.amount;
        deltas.update(Id::Xch).output += self.amount;
        deltas.set_needed(Id::Xch);
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        let (source, launcher) = spends.xch.create_launcher(self.amount)?;
        let source = &mut spends.xch.items[source];

        let (parent_conditions, eve_did) = launcher.create_eve_did(
            ctx,
            source.asset.p2_puzzle_hash(),
            self.recovery_list_hash,
            self.num_verifications_required,
            self.metadata,
        )?;

        match &mut source.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(parent_conditions);
            }
            SpendKind::Settlement(_) => {
                return Err(DriverError::CannotEmitConditions);
            }
        }

        spends
            .dids
            .insert(Id::New(index), SingletonSpends::new(eve_did, true));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;

    use crate::{Action, Relation};

    use super::*;

    #[test]
    fn test_action_create_did() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(&mut ctx, &[Action::create_empty_did()])?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let did = outputs.dids[&Id::New(0)];
        assert_ne!(sim.coin_state(did.coin.coin_id()), None);
        assert_eq!(did.info.recovery_list_hash, None);
        assert_eq!(did.info.num_verifications_required, 1);
        assert_eq!(did.info.metadata, HashedPtr::NIL);
        assert_eq!(did.info.p2_puzzle_hash, alice.puzzle_hash);
        assert_eq!(did.coin.amount, 1);

        Ok(())
    }
}
