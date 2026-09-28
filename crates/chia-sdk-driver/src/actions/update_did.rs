use chia_protocol::Bytes32;

use crate::{Delta, Deltas, DriverError, HashedPtr, Id, SpendAction, SpendContext, Spends};

#[derive(Debug, Clone, Copy)]
pub struct UpdateDidAction {
    pub id: Id,
    pub new_recovery_list_hash: Option<Option<Bytes32>>,
    pub new_num_verifications_required: Option<u64>,
    pub new_metadata: Option<HashedPtr>,
}

impl UpdateDidAction {
    pub fn new(
        id: Id,
        new_recovery_list_hash: Option<Option<Bytes32>>,
        new_num_verifications_required: Option<u64>,
        new_metadata: Option<HashedPtr>,
    ) -> Self {
        Self {
            id,
            new_recovery_list_hash,
            new_num_verifications_required,
            new_metadata,
        }
    }
}

impl SpendAction for UpdateDidAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        *deltas.update(self.id) += Delta::new(1, 1);
        deltas.set_needed(self.id);
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

        if let Some(new_recovery_list_hash) = self.new_recovery_list_hash {
            did.child_info.recovery_list_hash = new_recovery_list_hash;
        }

        if let Some(new_num_verifications_required) = self.new_num_verifications_required {
            did.child_info.num_verifications_required = new_num_verifications_required;
        }

        if let Some(new_metadata) = self.new_metadata {
            did.child_info.metadata = new_metadata;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;

    use crate::{Action, BURN_PUZZLE_HASH, Relation};

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
                Action::update_did(
                    Id::New(0),
                    Some(Some(Bytes32::default())),
                    Some(2),
                    Some(metadata),
                ),
                Action::burn(Id::New(0), 1, hint),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
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

    #[test]
    fn test_action_update_existing_did_then_send_and_melt() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);
        let bob = sim.bls(0);
        let bob_hint = ctx.hint(bob.puzzle_hash)?;
        let metadata = ctx.alloc_hashed(&"Hello, world!")?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(&mut ctx, &[Action::create_empty_did()])?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), std::slice::from_ref(&alice.sk))?;

        let did = outputs.dids[&Id::New(0)];
        let id = Id::Existing(did.info.launcher_id);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(did);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::update_did(id, None, Some(1), Some(metadata)),
                Action::send(id, bob.puzzle_hash, 1, bob_hint),
            ],
        )?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let did = outputs.dids[&id];
        assert_eq!(did.info.p2_puzzle_hash, bob.puzzle_hash);
        assert_eq!(did.info.num_verifications_required, 1);
        assert_eq!(did.info.metadata, metadata);
        assert!(
            sim.coin_state(did.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        let actions = [
            Action::update_did(id, Some(None), None, None),
            Action::melt_singleton(id, 1),
        ];

        assert!(Deltas::from_actions(&actions).is_needed(&Id::Xch));

        let mut spends = Spends::new(bob.puzzle_hash);
        spends.add(did);
        spends.add(bob.coin);

        let deltas = spends.apply(&mut ctx, &actions)?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { bob.puzzle_hash => bob.pk },
        )?;

        sim.spend_coins(ctx.take(), &[bob.sk])?;

        assert!(outputs.dids.is_empty());
        assert_eq!(outputs.xch.len(), 1);
        assert_eq!(outputs.xch[0].puzzle_hash, bob.puzzle_hash);
        assert_eq!(outputs.xch[0].amount, 1);
        assert!(
            sim.coin_state(did.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_some())
        );

        Ok(())
    }
}
