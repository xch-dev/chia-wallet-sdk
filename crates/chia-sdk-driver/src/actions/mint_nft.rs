use chia_protocol::Bytes32;

use crate::{
    Asset, Delta, Deltas, DriverError, HashedPtr, Id, SingletonSpends, SpendAction, SpendContext,
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
        *deltas.update(Id::Xch) += Delta::new(0, self.amount);
        *deltas.update(Id::New(index)) += Delta::new(self.amount, 0);

        if !matches!(self.parent_id, Id::Xch) {
            *deltas.update(self.parent_id) += Delta::new(1, 1);
        }

        deltas.set_needed(self.parent_id);
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

    #[test]
    fn test_action_mint_nft_from_existing_did() -> Result<()> {
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

        sim.spend_coins(ctx.take(), std::slice::from_ref(&alice.sk))?;

        let did = outputs.dids[&Id::New(0)];
        let did_id = Id::Existing(did.info.launcher_id);

        let actions = [Action::mint_empty_nft_from_did(did_id)];

        let deltas = Deltas::from_actions(&actions);
        assert!(deltas.is_needed(&did_id));
        assert_eq!(deltas.get(&did_id), Some(&Delta::new(1, 1)));
        assert_eq!(deltas.get(&Id::Xch), Some(&Delta::new(0, 1)));

        let xch = sim.new_coin(alice.puzzle_hash, 1);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(did);
        spends.add(xch);

        let deltas = spends.apply(&mut ctx, &actions)?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let new_did = outputs.dids[&did_id];
        assert_eq!(new_did.coin.parent_coin_info, did.coin.coin_id());
        assert!(
            sim.coin_state(new_did.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        let nft = outputs.nfts[&Id::New(0)];
        assert!(
            sim.coin_state(nft.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );
        assert_eq!(nft.info.p2_puzzle_hash, alice.puzzle_hash);

        Ok(())
    }

    #[test]
    fn test_action_mint_multiple_nfts_from_did() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(4);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::create_empty_did(),
                Action::mint_empty_nft_from_did(Id::New(0)),
                Action::mint_empty_nft_from_did(Id::New(0)),
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

        let mut launcher_ids = Vec::new();

        for index in 1..=3 {
            let nft = outputs.nfts[&Id::New(index)];
            assert!(
                sim.coin_state(nft.coin.coin_id())
                    .is_some_and(|state| state.spent_height.is_none())
            );
            launcher_ids.push(nft.info.launcher_id);
        }

        launcher_ids.sort();
        launcher_ids.dedup();
        assert_eq!(launcher_ids.len(), 3);

        let did = outputs.dids[&Id::New(0)];
        assert!(
            sim.coin_state(did.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        Ok(())
    }

    #[test]
    fn test_action_mint_nft_from_did_and_send_did() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);
        let bob = chia_sdk_test::BlsPair::new(1);
        let bob_hint = ctx.hint(bob.puzzle_hash)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::create_empty_did(),
                Action::mint_empty_nft_from_did(Id::New(0)),
                Action::send(Id::New(0), bob.puzzle_hash, 1, bob_hint),
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
        assert_eq!(did.info.p2_puzzle_hash, bob.puzzle_hash);
        assert!(
            sim.coin_state(did.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        let nft = outputs.nfts[&Id::New(1)];
        assert_eq!(nft.info.p2_puzzle_hash, alice.puzzle_hash);
        assert!(
            sim.coin_state(nft.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        Ok(())
    }
}
