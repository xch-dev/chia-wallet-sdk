use chia_sdk_types::{
    conditions::{TradePrice, TransferNft},
    Conditions,
};

use crate::{
    assignment_puzzle_announcement_id, Deltas, DriverError, Id, Spend, SpendAction, SpendContext,
    SpendKind, Spends,
};

#[derive(Debug, Default, Clone)]
pub struct TransferNftById {
    pub did_id: Option<Id>,
    pub trade_prices: Vec<TradePrice>,
}

impl TransferNftById {
    pub fn new(did_id: Option<Id>, trade_prices: Vec<TradePrice>) -> Self {
        Self {
            did_id,
            trade_prices,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateNftAction {
    pub id: Id,
    pub metadata_update_spends: Vec<Spend>,
    pub transfer: Option<TransferNftById>,
}

impl UpdateNftAction {
    pub fn new(
        id: Id,
        metadata_update_spends: Vec<Spend>,
        transfer: Option<TransferNftById>,
    ) -> Self {
        Self {
            id,
            metadata_update_spends,
            transfer,
        }
    }
}

impl SpendAction for UpdateNftAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        let nft = deltas.update(self.id);
        nft.input += 1;
        nft.output += 1;

        if let Some(transfer) = &self.transfer {
            if let Some(did_id) = transfer.did_id {
                let did = deltas.update(did_id);
                did.input += 1;
                did.output += 1;
            }
        }
    }

    fn spend(
        &self,
        _ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        let nft = spends
            .nfts
            .get_mut(&self.id)
            .ok_or(DriverError::InvalidAssetId)?
            .last_mut()?;

        nft.child_info
            .metadata_update_spends
            .extend_from_slice(&self.metadata_update_spends);

        if let Some(transfer) = self.transfer.clone() {
            let transfer_condition = if let Some(did_id) = transfer.did_id {
                let did = spends
                    .dids
                    .get_mut(&did_id)
                    .ok_or(DriverError::InvalidAssetId)?
                    .last_mut()?;

                let transfer_condition = TransferNft::new(
                    Some(did.asset.info.launcher_id),
                    transfer.trade_prices,
                    Some(did.asset.info.inner_puzzle_hash().into()),
                );

                match &mut did.kind {
                    SpendKind::Conditions(spend) => {
                        spend.add_conditions(
                            Conditions::new()
                                .assert_puzzle_announcement(assignment_puzzle_announcement_id(
                                    nft.asset.coin.puzzle_hash,
                                    &transfer_condition,
                                ))
                                .create_puzzle_announcement(nft.asset.info.launcher_id.into()),
                        );
                    }
                    SpendKind::Settlement(_) => {
                        return Err(DriverError::CannotEmitConditions);
                    }
                }

                transfer_condition
            } else {
                TransferNft::new(None, transfer.trade_prices, None)
            };

            nft.child_info.transfer_condition = Some(transfer_condition);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_protocol::Bytes32;
    use chia_puzzle_types::nft::NftMetadata;
    use chia_puzzles::NFT_METADATA_UPDATER_DEFAULT_HASH;
    use chia_sdk_test::Simulator;
    use indexmap::indexmap;

    use crate::{Action, HashedPtr, MetadataUpdate, Relation};

    use super::*;

    #[test]
    fn test_action_update_nft_uri() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut metadata = NftMetadata {
            data_hash: Some(Bytes32::default()),
            data_uris: vec!["https://example.com/1".to_string()],
            ..Default::default()
        };
        let original_metadata = ctx.alloc_hashed(&metadata)?;

        let metadata_update_spend =
            MetadataUpdate::NewDataUri("https://example.com/2".to_string()).spend(&mut ctx)?;
        metadata
            .data_uris
            .insert(0, "https://example.com/2".to_string());
        let updated_metadata = ctx.alloc_hashed(&metadata)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::mint_nft(
                    original_metadata,
                    NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
                    Bytes32::default(),
                    0,
                    1,
                ),
                Action::update_nft(Id::New(0), vec![metadata_update_spend], None),
            ],
        )?;

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
        assert_eq!(nft.info.metadata, updated_metadata);

        Ok(())
    }

    #[test]
    fn test_action_update_nft_uri_twice() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1);

        let mut metadata = NftMetadata {
            data_hash: Some(Bytes32::default()),
            data_uris: vec!["https://example.com/1".to_string()],
            ..Default::default()
        };
        let original_metadata = ctx.alloc_hashed(&metadata)?;

        let metadata_update_spends = vec![
            MetadataUpdate::NewDataUri("https://example.com/2".to_string()).spend(&mut ctx)?,
            MetadataUpdate::NewDataUri("https://example.com/3".to_string()).spend(&mut ctx)?,
        ];
        metadata
            .data_uris
            .insert(0, "https://example.com/3".to_string());
        metadata
            .data_uris
            .insert(0, "https://example.com/2".to_string());
        let updated_metadata = ctx.alloc_hashed(&metadata)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::mint_nft(
                    original_metadata,
                    NFT_METADATA_UPDATER_DEFAULT_HASH.into(),
                    Bytes32::default(),
                    0,
                    1,
                ),
                Action::update_nft(Id::New(0), metadata_update_spends, None),
            ],
        )?;

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
        assert_eq!(nft.info.metadata, updated_metadata);

        Ok(())
    }

    #[test]
    fn test_action_update_nft_owner() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::create_empty_did(),
                Action::mint_nft(HashedPtr::NIL, Bytes32::default(), Bytes32::default(), 0, 1),
                Action::update_nft(
                    Id::New(1),
                    Vec::new(),
                    Some(TransferNftById::new(Some(Id::New(0)), vec![])),
                ),
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
        assert_eq!(nft.info.current_owner, Some(did.info.launcher_id));

        Ok(())
    }
}
