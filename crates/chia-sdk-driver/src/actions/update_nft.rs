use chia_protocol::Bytes32;
use chia_sdk_types::{
    Conditions,
    conditions::{TradePrice, TransferNft},
};

use crate::{
    Deltas, DriverError, Id, NftIdentity, SingletonInfo, Spend, SpendAction, SpendContext,
    SpendKind, Spends, assignment_puzzle_announcement_id,
};

#[derive(Debug, Default, Clone)]
pub struct TransferNftById {
    pub owner: Option<NftIdentity>,
    pub trade_prices: Vec<TradePrice>,
}

impl TransferNftById {
    pub fn new(owner: Option<NftIdentity>, trade_prices: Vec<TradePrice>) -> Self {
        Self {
            owner,
            trade_prices,
        }
    }

    pub fn with_did(did_id: Id, trade_prices: Vec<TradePrice>) -> Self {
        Self::new(Some(NftIdentity::Did(did_id)), trade_prices)
    }

    pub fn with_nft(nft_id: Id, trade_prices: Vec<TradePrice>) -> Self {
        Self::new(Some(NftIdentity::Nft(nft_id)), trade_prices)
    }

    pub fn unassigned(trade_prices: Vec<TradePrice>) -> Self {
        Self::new(None, trade_prices)
    }
}

fn assign_identity(
    identity_kind: &mut SpendKind,
    identity_launcher_id: Bytes32,
    identity_inner_puzzle_hash: Bytes32,
    target_puzzle_hash: Bytes32,
    target_launcher_id: Bytes32,
    trade_prices: Vec<TradePrice>,
) -> Result<TransferNft, DriverError> {
    let transfer_condition = TransferNft::new(
        Some(identity_launcher_id),
        trade_prices,
        Some(identity_inner_puzzle_hash),
    );

    match identity_kind {
        SpendKind::Conditions(spend) => {
            spend.add_conditions(
                Conditions::new()
                    .assert_puzzle_announcement(assignment_puzzle_announcement_id(
                        target_puzzle_hash,
                        &transfer_condition,
                    ))
                    .create_puzzle_announcement(target_launcher_id.into()),
            );
        }
        SpendKind::Settlement(_) => {
            return Err(DriverError::CannotEmitConditions);
        }
    }

    Ok(transfer_condition)
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
        deltas.update(self.id).input += 1;
        deltas.update(self.id).output += 1;
        deltas.set_needed(self.id);

        if let Some(transfer) = &self.transfer
            && let Some(owner) = transfer.owner
        {
            let owner_id = owner.id();
            deltas.update(owner_id).input += 1;
            deltas.update(owner_id).output += 1;
            deltas.set_needed(owner_id);
        }
    }

    fn spend(
        &self,
        _ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        let target = spends
            .nfts
            .get(&self.id)
            .ok_or(DriverError::InvalidAssetId)?
            .last()?;
        let target_puzzle_hash = target.asset.coin.puzzle_hash;
        let target_launcher_id = target.asset.info.launcher_id;

        if let Some(transfer) = self.transfer.clone() {
            let transfer_condition = if let Some(owner) = transfer.owner {
                match owner {
                    NftIdentity::Did(id) => {
                        let identity = spends
                            .dids
                            .get_mut(&id)
                            .ok_or(DriverError::MissingNftIdentity)?
                            .last_mut()?;
                        assign_identity(
                            &mut identity.kind,
                            identity.asset.info.launcher_id,
                            identity.asset.info.inner_puzzle_hash().into(),
                            target_puzzle_hash,
                            target_launcher_id,
                            transfer.trade_prices,
                        )?
                    }
                    NftIdentity::Nft(id) => {
                        let identity = spends
                            .nfts
                            .get_mut(&id)
                            .ok_or(DriverError::MissingNftIdentity)?
                            .last_mut()?;
                        assign_identity(
                            &mut identity.kind,
                            identity.asset.info.launcher_id,
                            identity.asset.info.inner_puzzle_hash().into(),
                            target_puzzle_hash,
                            target_launcher_id,
                            transfer.trade_prices,
                        )?
                    }
                }
            } else {
                TransferNft::new(None, transfer.trade_prices, None)
            };

            let nft = spends
                .nfts
                .get_mut(&self.id)
                .ok_or(DriverError::InvalidAssetId)?
                .last_mut()?;
            nft.child_info.transfer_condition = Some(transfer_condition);
        }

        let nft = spends
            .nfts
            .get_mut(&self.id)
            .ok_or(DriverError::InvalidAssetId)?
            .last_mut()?;
        nft.child_info
            .metadata_update_spends
            .extend_from_slice(&self.metadata_update_spends);

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

    use crate::{Action, HashedPtr, MetadataUpdate, Relation, UriKind};

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

        let metadata_update_spend = MetadataUpdate {
            kind: UriKind::Data,
            uri: "https://example.com/2".to_string(),
        }
        .spend(&mut ctx)?;
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
            MetadataUpdate {
                kind: UriKind::Data,
                uri: "https://example.com/2".to_string(),
            }
            .spend(&mut ctx)?,
            MetadataUpdate {
                kind: UriKind::Data,
                uri: "https://example.com/3".to_string(),
            }
            .spend(&mut ctx)?,
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
                    Some(TransferNftById::with_did(Id::New(0), vec![])),
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

    #[test]
    fn test_action_update_nft_owner_to_nft_chain() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(3);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::mint_empty_nft(),
                Action::mint_empty_nft(),
                Action::mint_empty_nft(),
                Action::update_nft(
                    Id::New(1),
                    Vec::new(),
                    Some(TransferNftById::with_nft(Id::New(0), vec![])),
                ),
                Action::update_nft(
                    Id::New(2),
                    Vec::new(),
                    Some(TransferNftById::with_nft(Id::New(1), vec![])),
                ),
            ],
        )?;

        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::AssertConcurrent,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let first = outputs.nfts[&Id::New(0)];
        let second = outputs.nfts[&Id::New(1)];
        let third = outputs.nfts[&Id::New(2)];

        assert_eq!(second.info.current_owner, Some(first.info.launcher_id));
        assert_eq!(third.info.current_owner, Some(second.info.launcher_id));
        assert_ne!(sim.coin_state(first.coin.coin_id()), None);
        assert_ne!(sim.coin_state(second.coin.coin_id()), None);
        assert_ne!(sim.coin_state(third.coin.coin_id()), None);

        Ok(())
    }

    #[test]
    fn test_action_update_nft_owner_missing_identity() {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();
        let alice = sim.bls(1);
        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let error = spends
            .apply(
                &mut ctx,
                &[
                    Action::mint_empty_nft(),
                    Action::update_nft(
                        Id::New(0),
                        Vec::new(),
                        Some(TransferNftById::with_nft(
                            Id::Existing(Bytes32::default()),
                            vec![],
                        )),
                    ),
                ],
            )
            .unwrap_err();

        assert!(matches!(error, DriverError::MissingNftIdentity));
    }
}
