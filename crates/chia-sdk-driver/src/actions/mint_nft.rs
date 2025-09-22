use chia_protocol::Bytes32;

use crate::{
    Asset, Deltas, DriverError, HashedPtr, Id, SingletonSpends, SpendAction, SpendContext,
    SpendKind, Spends,
};

#[derive(Debug, Clone, Copy)]
pub struct MintNftAction {
    pub parent_id: Id,
    pub metadata: HashedPtr,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub amount: u64,
}

impl MintNftAction {
    pub fn new(
        parent_id: Id,
        metadata: HashedPtr,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        amount: u64,
    ) -> Self {
        Self {
            parent_id,
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
            Id::Xch,
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
        deltas.update(Id::Xch).output += self.amount;
        deltas.update(Id::New(index)).input += self.amount;

        if matches!(self.parent_id, Id::Xch) {
            deltas.set_needed(Id::Xch);
        } else {
            let did = deltas.update(self.parent_id);
            did.input += 1;
            did.output += 1;
        }
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        let (p2_puzzle_hash, source_kind, launcher) = if matches!(self.parent_id, Id::Xch) {
            let (source, launcher) = spends.xch.create_launcher(self.amount)?;
            let source = &mut spends.xch.items[source];
            (source.asset.p2_puzzle_hash(), &mut source.kind, launcher)
        } else {
            let did = spends
                .dids
                .get_mut(&self.parent_id)
                .ok_or(DriverError::InvalidAssetId)?;
            let (source, launcher) = did.create_launcher(self.amount)?;
            let p2_puzzle_hash = did.last()?.asset.p2_puzzle_hash();
            let source = &mut did.lineage[source];
            (p2_puzzle_hash, &mut source.kind, launcher)
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
                spend.add_conditions(parent_conditions);
            }
            SpendKind::Settlement(_) => {
                return Err(DriverError::CannotEmitConditions);
            }
        }

        spends
            .nfts
            .insert(Id::New(index), SingletonSpends::new(eve_nft, true));

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
    fn test_action_mint_nft() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(&mut ctx, &[Action::mint_empty_nft()])?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

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

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::create_empty_did(),
                Action::mint_empty_nft_from_did(Id::New(0)),
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
        assert_eq!(did.info.p2_puzzle_hash, alice.puzzle_hash);

        let nft = outputs.nfts[&Id::New(1)];
        assert_ne!(sim.coin_state(nft.coin.coin_id()), None);
        assert_eq!(nft.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }
}
