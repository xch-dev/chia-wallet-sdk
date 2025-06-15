use chia_protocol::Bytes32;

use crate::{
    DriverError, FungibleAsset, HashedPtr, Id, SingletonSpends, SpendAction, SpendContext,
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
                spend.add_conditions(parent_conditions)?;
            }
        }

        let kind = source.kind.child();

        spends
            .dids
            .insert(Id::New(index), SingletonSpends::new(eve_did, kind));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;

    use crate::Action;

    use super::*;

    #[test]
    fn test_action_create_did() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new();
        spends.add_xch(alice.coin, SpendKind::conditions(vec![]));
        spends.apply(&mut ctx, &[Action::create_simple_did()])?;
        spends.create_change(&mut ctx, alice.puzzle_hash)?;
        spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk })?;

        let coin_spends = ctx.take();

        sim.spend_coins(coin_spends, &[alice.sk])?;

        assert_eq!(
            sim.unspent_coins(alice.puzzle_hash, false)
                .iter()
                .fold(0, |acc, coin| acc + coin.amount),
            1
        );

        Ok(())
    }
}
