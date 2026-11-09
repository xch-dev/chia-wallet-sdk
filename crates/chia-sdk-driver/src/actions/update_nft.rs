use chia_protocol::Bytes32;
use chia_sdk_types::{
    Conditions,
    conditions::{TradePrice, TransferNft},
};

use crate::{
    Delta, Deltas, DriverError, Id, SingletonInfo, Spend, SpendAction, SpendContext, SpendKind,
    Spends, assignment_puzzle_announcement_id,
};

/// Assigns an NFT to a DID or another NFT in the same transaction, or removes its current owner.
#[derive(Debug, Default, Clone)]
pub struct TransferNftById {
    /// The DID or NFT to assign the NFT to, which must be spent in the same transaction, or `None`
    /// to remove the NFT's current owner.
    pub owner_id: Option<Id>,
    /// The trade prices of the NFT, which determine the royalties that must be paid when it's
    /// transferred as part of an offer.
    pub trade_prices: Vec<TradePrice>,
}

impl TransferNftById {
    pub fn new(owner_id: Option<Id>, trade_prices: Vec<TradePrice>) -> Self {
        Self {
            owner_id,
            trade_prices,
        }
    }
}

/// Created by [`Action::update_nft`](crate::Action::update_nft).
#[derive(Debug, Clone)]
pub struct UpdateNftAction {
    pub id: Id,
    /// Spends of the metadata updater puzzle, which are run in order. Each one requires a separate
    /// spend of the NFT.
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
        *deltas.update(self.id) += Delta::new(1, 1);
        deltas.set_needed(self.id);

        if let Some(transfer) = &self.transfer
            && let Some(owner_id) = transfer.owner_id
        {
            *deltas.update(owner_id) += Delta::new(1, 1);
            deltas.set_needed(owner_id);
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

        let nft_puzzle_hash = nft.transfer_puzzle_hash(spends.intermediate_puzzle_hash);
        let nft_launcher_id = nft.asset.info.launcher_id;

        if let Some(transfer) = self.transfer.clone() {
            let transfer_condition = if let Some(owner_id) = transfer.owner_id {
                if let Some(did) = spends.dids.get_mut(&owner_id) {
                    let did = did.last_mut()?;

                    assign_owner(
                        &mut did.kind,
                        did.asset.info.launcher_id,
                        did.asset.info.inner_puzzle_hash().into(),
                        nft_puzzle_hash,
                        nft_launcher_id,
                        transfer.trade_prices,
                    )?
                } else if let Some(owner) = spends.nfts.get_mut(&owner_id) {
                    let owner = owner.last_mut()?;

                    assign_owner(
                        &mut owner.kind,
                        owner.asset.info.launcher_id,
                        owner.asset.info.inner_puzzle_hash().into(),
                        nft_puzzle_hash,
                        nft_launcher_id,
                        transfer.trade_prices,
                    )?
                } else {
                    return Err(DriverError::InvalidAssetId);
                }
            } else {
                TransferNft::new(None, transfer.trade_prices, None)
            };

            spends
                .nfts
                .get_mut(&self.id)
                .ok_or(DriverError::InvalidAssetId)?
                .last_mut()?
                .child_info
                .transfer_condition = Some(transfer_condition);
        }

        Ok(())
    }
}

/// Adds the announcements to the owner's spend that authorize assigning the NFT to it, and returns
/// the transfer condition for the NFT's spend.
fn assign_owner(
    owner_kind: &mut SpendKind,
    owner_launcher_id: Bytes32,
    owner_inner_puzzle_hash: Bytes32,
    nft_puzzle_hash: Bytes32,
    nft_launcher_id: Bytes32,
    trade_prices: Vec<TradePrice>,
) -> Result<TransferNft, DriverError> {
    let transfer_condition = TransferNft::new(
        Some(owner_launcher_id),
        trade_prices,
        Some(owner_inner_puzzle_hash),
    );

    match owner_kind {
        SpendKind::Conditions(spend) => {
            spend.add_conditions(
                Conditions::new()
                    .assert_puzzle_announcement(assignment_puzzle_announcement_id(
                        nft_puzzle_hash,
                        &transfer_condition,
                    ))
                    .create_puzzle_announcement(nft_launcher_id.into()),
            );
        }
        SpendKind::Settlement(_) => {
            return Err(DriverError::CannotEmitConditions);
        }
    }

    Ok(transfer_condition)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_protocol::Coin;
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
            .insert(0, "https://example.com/2".to_string());
        metadata
            .data_uris
            .insert(0, "https://example.com/3".to_string());
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

    #[test]
    fn test_action_assign_existing_nft_and_send() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);
        let bob = sim.bls(0);
        let bob_hint = ctx.hint(bob.puzzle_hash)?;

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::create_empty_did(), Action::mint_empty_nft()],
        )?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), std::slice::from_ref(&alice.sk))?;

        let did = outputs.dids[&Id::New(0)];
        let nft = outputs.nfts[&Id::New(1)];
        let did_id = Id::Existing(did.info.launcher_id);
        let nft_id = Id::Existing(nft.info.launcher_id);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(did);
        spends.add(nft);

        let deltas = spends.apply(
            &mut ctx,
            &[
                Action::update_nft(
                    nft_id,
                    Vec::new(),
                    Some(TransferNftById::new(Some(did_id), vec![])),
                ),
                Action::send(nft_id, bob.puzzle_hash, 1, bob_hint),
            ],
        )?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let did = outputs.dids[&did_id];
        assert_eq!(did.info.p2_puzzle_hash, alice.puzzle_hash);
        assert!(
            sim.coin_state(did.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        let nft = outputs.nfts[&nft_id];
        assert_eq!(nft.info.p2_puzzle_hash, bob.puzzle_hash);
        assert_eq!(nft.info.current_owner, Some(did.info.launcher_id));
        assert!(
            sim.coin_state(nft.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        let mut spends = Spends::new(bob.puzzle_hash);
        spends.add(nft);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::update_nft(
                nft_id,
                Vec::new(),
                Some(TransferNftById::new(None, vec![])),
            )],
        )?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { bob.puzzle_hash => bob.pk },
        )?;

        sim.spend_coins(ctx.take(), &[bob.sk])?;

        let nft = outputs.nfts[&nft_id];
        assert_eq!(nft.info.p2_puzzle_hash, bob.puzzle_hash);
        assert_eq!(nft.info.current_owner, None);
        assert!(
            sim.coin_state(nft.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

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
                    Some(TransferNftById::new(Some(Id::New(0)), vec![])),
                ),
                Action::update_nft(
                    Id::New(2),
                    Vec::new(),
                    Some(TransferNftById::new(Some(Id::New(1)), vec![])),
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

        assert_eq!(first.info.current_owner, None);
        assert_eq!(second.info.current_owner, Some(first.info.launcher_id));
        assert_eq!(third.info.current_owner, Some(second.info.launcher_id));

        for nft in [first, second, third] {
            assert!(
                sim.coin_state(nft.coin.coin_id())
                    .is_some_and(|state| state.spent_height.is_none())
            );
        }

        Ok(())
    }

    #[test]
    fn test_action_assign_existing_nft_to_existing_nft() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(2);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(alice.coin);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::mint_empty_nft(), Action::mint_empty_nft()],
        )?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), std::slice::from_ref(&alice.sk))?;

        let owner = outputs.nfts[&Id::New(0)];
        let nft = outputs.nfts[&Id::New(1)];
        let owner_id = Id::Existing(owner.info.launcher_id);
        let nft_id = Id::Existing(nft.info.launcher_id);

        let mut spends = Spends::new(alice.puzzle_hash);
        spends.add(owner);
        spends.add(nft);

        let deltas = spends.apply(
            &mut ctx,
            &[Action::update_nft(
                nft_id,
                Vec::new(),
                Some(TransferNftById::new(Some(owner_id), vec![])),
            )],
        )?;
        let outputs = spends.finish_with_keys(
            &mut ctx,
            &deltas,
            Relation::None,
            &indexmap! { alice.puzzle_hash => alice.pk },
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let owner = outputs.nfts[&owner_id];
        let nft = outputs.nfts[&nft_id];
        assert_eq!(owner.info.current_owner, None);
        assert_eq!(nft.info.current_owner, Some(owner.info.launcher_id));
        assert!(
            sim.coin_state(nft.coin.coin_id())
                .is_some_and(|state| state.spent_height.is_none())
        );

        Ok(())
    }

    #[test]
    fn test_action_update_nft_missing_owner() {
        let mut ctx = SpendContext::new();
        let mut spends = Spends::new(Bytes32::default());
        spends.add(Coin::new(Bytes32::default(), Bytes32::default(), 1));

        let error = spends
            .apply(
                &mut ctx,
                &[
                    Action::mint_empty_nft(),
                    Action::update_nft(
                        Id::New(0),
                        Vec::new(),
                        Some(TransferNftById::new(
                            Some(Id::Existing(Bytes32::default())),
                            vec![],
                        )),
                    ),
                ],
            )
            .unwrap_err();

        assert!(matches!(error, DriverError::InvalidAssetId));
    }
}
