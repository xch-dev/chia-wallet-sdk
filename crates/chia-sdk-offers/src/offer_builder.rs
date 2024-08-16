use chia_bls::Signature;
use chia_protocol::{Bytes32, CoinSpend, Program};
use chia_puzzles::{
    cat::CatArgs,
    nft::{NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NftStateLayerArgs},
    offer::{NotarizedPayment, Payment},
    singleton::SingletonArgs,
};
use chia_sdk_driver::{Nft, SpendContext, SpendError};

use chia_sdk_types::{announcement_id, AssertPuzzleAnnouncement};
use clvm_traits::ToClvm;
use clvm_utils::{CurriedProgram, ToTreeHash};
use clvmr::Allocator;
use indexmap::IndexMap;

use crate::Offer;

#[derive(Debug, Clone, Copy)]
pub struct NftPaymentInfo<M> {
    pub launcher_id: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
    pub current_owner: Option<Bytes32>,
    pub metadata: M,
}

impl<M> NftPaymentInfo<M>
where
    M: Clone,
{
    pub fn from_nft(nft: &Nft<M>) -> Self {
        Self {
            launcher_id: nft.info.singleton_struct.launcher_id,
            royalty_puzzle_hash: nft.info.royalty_puzzle_hash,
            royalty_percentage: nft.info.royalty_ten_thousandths,
            current_owner: None,
            metadata: nft.info.metadata.clone(),
        }
    }
}

#[derive(Debug, Clone)]
#[must_use]
pub struct OfferBuilder {
    nonce: Bytes32,
    requested_payments: IndexMap<Program, Vec<NotarizedPayment>>,
}

impl OfferBuilder {
    pub fn new(coin_ids: Vec<Bytes32>) -> Self {
        Self::with_nonce(calculate_nonce(coin_ids))
    }

    pub fn with_nonce(nonce: Bytes32) -> Self {
        Self {
            nonce,
            requested_payments: IndexMap::new(),
        }
    }

    pub fn nonce(&self) -> Bytes32 {
        self.nonce
    }

    pub fn request_standard_payments(
        self,
        ctx: &mut SpendContext,
        payments: Vec<Payment>,
    ) -> Result<(AssertPuzzleAnnouncement, Self), SpendError> {
        let puzzle = ctx.settlement_payments_puzzle()?;
        self.request_raw_payments(ctx, &puzzle, payments)
    }

    pub fn request_cat_payments(
        self,
        ctx: &mut SpendContext,
        asset_id: Bytes32,
        payments: Vec<Payment>,
    ) -> Result<(AssertPuzzleAnnouncement, Self), SpendError> {
        let settlement_payments_puzzle = ctx.settlement_payments_puzzle()?;
        let cat_puzzle = ctx.cat_puzzle()?;

        let puzzle = ctx.alloc(&CurriedProgram {
            program: cat_puzzle,
            args: CatArgs::new(asset_id, settlement_payments_puzzle),
        })?;

        self.request_raw_payments(ctx, &puzzle, payments)
    }

    pub fn request_nft_payments<M>(
        self,
        ctx: &mut SpendContext,
        payment_info: NftPaymentInfo<M>,
        payments: Vec<Payment>,
    ) -> Result<(AssertPuzzleAnnouncement, Self), SpendError>
    where
        M: ToClvm<Allocator>,
    {
        let settlement_payments_puzzle = ctx.settlement_payments_puzzle()?;
        let transfer_program = ctx.nft_royalty_transfer()?;
        let ownership_layer_puzzle = ctx.nft_ownership_layer()?;
        let state_layer_puzzle = ctx.nft_state_layer()?;
        let singleton_puzzle = ctx.singleton_top_layer()?;

        let transfer = CurriedProgram {
            program: transfer_program,
            args: NftRoyaltyTransferPuzzleArgs::new(
                payment_info.launcher_id,
                payment_info.royalty_puzzle_hash,
                payment_info.royalty_percentage,
            ),
        };

        let ownership = CurriedProgram {
            program: ownership_layer_puzzle,
            args: NftOwnershipLayerArgs::new(
                payment_info.current_owner,
                transfer,
                settlement_payments_puzzle,
            ),
        };

        let state = CurriedProgram {
            program: state_layer_puzzle,
            args: NftStateLayerArgs::new(payment_info.metadata, ownership),
        };

        let puzzle = ctx.alloc(&CurriedProgram {
            program: singleton_puzzle,
            args: SingletonArgs::new(payment_info.launcher_id, state),
        })?;

        self.request_raw_payments(ctx, &puzzle, payments)
    }

    pub fn request_raw_payments<P>(
        mut self,
        ctx: &mut SpendContext,
        puzzle: &P,
        payments: Vec<Payment>,
    ) -> Result<(AssertPuzzleAnnouncement, Self), SpendError>
    where
        P: ToClvm<Allocator>,
    {
        let puzzle_ptr = ctx.alloc(puzzle)?;
        let puzzle_hash = ctx.tree_hash(puzzle_ptr).into();
        let puzzle_reveal = ctx.serialize(&puzzle_ptr)?;

        let notarized_payment = NotarizedPayment {
            nonce: self.nonce,
            payments,
        };

        self.requested_payments
            .entry(puzzle_reveal)
            .or_default()
            .extend([notarized_payment.clone()]);

        let notarized_payment_ptr = ctx.alloc(&notarized_payment)?;
        let notarized_payment_hash = ctx.tree_hash(notarized_payment_ptr);

        let announcement = AssertPuzzleAnnouncement {
            announcement_id: announcement_id(puzzle_hash, notarized_payment_hash),
        };

        Ok((announcement, self))
    }

    pub fn make_payments(self) -> OfferBuilder {
        OfferBuilder {
            nonce: self.nonce,
            requested_payments: self.requested_payments,
        }
    }

    pub fn finish(
        self,
        offered_coin_spends: Vec<CoinSpend>,
        aggregated_signature: Signature,
    ) -> Result<Offer, SpendError> {
        Ok(Offer::new(
            self.requested_payments,
            offered_coin_spends,
            aggregated_signature,
        ))
    }
}

pub fn calculate_nonce(mut coin_ids: Vec<Bytes32>) -> Bytes32 {
    coin_ids.sort();
    coin_ids.tree_hash().into()
}

#[cfg(test)]
mod tests {
    use chia_bls::DerivableKey;
    use chia_protocol::{Coin, SpendBundle};
    use chia_puzzles::{offer::SETTLEMENT_PAYMENTS_PUZZLE_HASH, standard::StandardArgs};
    use chia_sdk_driver::Conditions;
    use chia_sdk_test::{secret_key, sign_transaction, Simulator};

    use crate::SettlementSpend;

    use super::*;

    #[tokio::test]
    async fn test_simple_offer() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let a_secret_key = secret_key()?.derive_unhardened(0);
        let a_public_key = a_secret_key.public_key();
        let a_puzzle_hash = StandardArgs::curry_tree_hash(a_public_key).into();

        let b_secret_key = secret_key()?.derive_unhardened(1);
        let b_public_key = b_secret_key.public_key();
        let b_puzzle_hash = StandardArgs::curry_tree_hash(b_public_key).into();

        let a = sim.mint_coin(a_puzzle_hash, 1000).await;
        let b = sim.mint_coin(b_puzzle_hash, 3000).await;

        let (announcement, partial_offer) = OfferBuilder::new(vec![a.coin_id()])
            .request_standard_payments(ctx, vec![Payment::new(a_puzzle_hash, b.amount)])?;

        ctx.spend_p2_coin(
            a,
            a_public_key,
            Conditions::new()
                .assert_raw_puzzle_announcement(announcement.announcement_id)
                .create_coin(SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), a.amount),
        )?;

        let coin_spends = ctx.take_spends();
        let signature = sign_transaction(&coin_spends, &[a_secret_key], &sim.config().constants)?;
        let a_offer = partial_offer
            .make_payments()
            .finish(coin_spends, signature)?;

        let (announcement, partial_offer) = OfferBuilder::new(vec![b.coin_id()])
            .request_standard_payments(ctx, vec![Payment::new(b_puzzle_hash, a.amount)])?;

        ctx.spend_p2_coin(
            b,
            b_public_key,
            Conditions::new()
                .assert_raw_puzzle_announcement(announcement.announcement_id)
                .create_coin(SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(), b.amount),
        )?;

        let coin_spends = ctx.take_spends();
        let signature = sign_transaction(&coin_spends, &[b_secret_key], &sim.config().constants)?;
        let b_offer = partial_offer
            .make_payments()
            .finish(coin_spends, signature)?;

        SettlementSpend::new(
            b_offer
                .requested_payments()
                .values()
                .next()
                .cloned()
                .unwrap(),
        )
        .finish(
            ctx,
            Coin::new(
                a.coin_id(),
                SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                a.amount,
            ),
        )?;

        SettlementSpend::new(
            a_offer
                .requested_payments()
                .values()
                .next()
                .cloned()
                .unwrap(),
        )
        .finish(
            ctx,
            Coin::new(
                b.coin_id(),
                SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                b.amount,
            ),
        )?;

        let spend_bundle = SpendBundle::new(
            [
                a_offer.offered_coin_spends().to_vec(),
                b_offer.offered_coin_spends().to_vec(),
                ctx.take_spends(),
            ]
            .concat(),
            a_offer.aggregated_signature() + b_offer.aggregated_signature(),
        );

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        Ok(())
    }
}
