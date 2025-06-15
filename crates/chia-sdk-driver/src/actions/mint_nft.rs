use chia_protocol::Bytes32;

use crate::{
    Deltas, DriverError, FungibleAsset, HashedPtr, Id, SingletonAsset, SingletonSpends,
    SpendAction, SpendContext, SpendKind, Spends,
};

#[derive(Debug, Clone, Copy)]
pub struct MintNftAction {
    pub parent_did_id: Option<Id>,
    pub metadata: HashedPtr,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub amount: u64,
}

impl MintNftAction {
    pub fn new(
        parent_did_id: Option<Id>,
        metadata: HashedPtr,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        amount: u64,
    ) -> Self {
        Self {
            parent_did_id,
            metadata,
            metadata_updater_puzzle_hash,
            royalty_puzzle_hash,
            royalty_basis_points,
            amount,
        }
    }
}

impl Default for MintNftAction {
    fn default() -> Self {
        Self::new(
            None,
            HashedPtr::NIL,
            Bytes32::default(),
            Bytes32::default(),
            0,
            1,
        )
    }
}

impl SpendAction for MintNftAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        deltas.update(None).output += self.amount;
        deltas.update(Some(Id::New(index))).input += self.amount;

        if let Some(did_id) = self.parent_did_id {
            let did = deltas.update(Some(did_id));
            did.input += 1;
            did.output += 1;
        } else {
            deltas.set_xch_needed();
        }
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        let (p2_puzzle_hash, source_kind, launcher) = if let Some(id) = self.parent_did_id {
            let did = spends
                .dids
                .get_mut(&id)
                .ok_or(DriverError::InvalidAssetId)?;
            let (source, launcher) = did.create_launcher(self.amount)?;
            let p2_puzzle_hash = did.last()?.asset.p2_puzzle_hash();
            let source = &mut did.lineage[source];
            (p2_puzzle_hash, &mut source.kind, launcher)
        } else {
            let (source, launcher) = spends.xch.create_launcher(self.amount)?;
            let source = &mut spends.xch.items[source];
            (source.asset.p2_puzzle_hash(), &mut source.kind, launcher)
        };

        let (parent_conditions, eve_nft) = launcher.mint_eve_nft(
            ctx,
            p2_puzzle_hash,
            self.metadata,
            self.metadata_updater_puzzle_hash,
            self.royalty_puzzle_hash,
            self.royalty_basis_points,
        )?;

        match source_kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(parent_conditions)?;
            }
        }

        let kind = source_kind.child();

        spends
            .nfts
            .insert(Id::New(index), SingletonSpends::new(eve_nft, kind, true));

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
    fn test_action_mint_nft() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new();
        spends.add_xch(alice.coin, SpendKind::conditions(vec![]));

        let deltas = spends.apply(&mut ctx, &[Action::mint_empty_nft()])?;
        spends.create_change(&mut ctx, &deltas, alice.puzzle_hash)?;

        let outputs =
            spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk })?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let nft = outputs.nfts[&Id::New(0)];
        assert_ne!(sim.coin_state(nft.coin.coin_id()), None);
        assert_eq!(nft.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }

    #[test]
    fn test_action_mint_nft_from_did() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);

        let mut spends = Spends::new();
        spends.add_xch(alice.coin, SpendKind::conditions(vec![]));

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::create_empty_did(),
                Action::mint_empty_nft_from_did(Id::New(0)),
            ],
        )?;
        spends.create_change(&mut ctx, &deltas, alice.puzzle_hash)?;

        let outputs =
            spends.finish_with_keys(&mut ctx, &indexmap! { alice.puzzle_hash => alice.pk })?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let did = outputs.dids[&Id::New(0)];
        assert_ne!(sim.coin_state(did.coin.coin_id()), None);
        assert_eq!(did.info.p2_puzzle_hash, alice.puzzle_hash);

        let nft = outputs.nfts[&Id::New(1)];
        assert_ne!(sim.coin_state(nft.coin.coin_id()), None);
        assert_eq!(nft.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }
}
