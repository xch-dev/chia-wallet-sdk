mod offer_builder;
mod offer_compression;
mod offer_encoding;
mod settlement_payments;

pub use offer_builder::*;
pub use offer_compression::*;
pub use offer_encoding::*;
pub use settlement_payments::*;

use chia_protocol::{CoinSpend, SpendBundle};

#[derive(Debug, Clone)]
pub struct Offer {
    offered_spend_bundle: SpendBundle,
    requested_payment_spends: Vec<CoinSpend>,
}

impl From<SpendBundle> for Offer {
    fn from(spend_bundle: SpendBundle) -> Self {
        let (requested_payment_spends, coin_spends): (_, Vec<_>) = spend_bundle
            .coin_spends
            .into_iter()
            .partition(|coin_spend| {
                coin_spend
                    .coin
                    .parent_coin_info
                    .iter()
                    .all(|byte| *byte == 0)
            });

        let offered_spend_bundle = SpendBundle::new(coin_spends, spend_bundle.aggregated_signature);

        Self {
            offered_spend_bundle,
            requested_payment_spends,
        }
    }
}

impl From<Offer> for SpendBundle {
    fn from(offer: Offer) -> Self {
        let mut spend_bundle = offer.offered_spend_bundle;

        spend_bundle
            .coin_spends
            .extend(offer.requested_payment_spends);

        spend_bundle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_bls::{sign, DerivableKey, SecretKey, Signature};
    use chia_protocol::{Bytes32, Coin, SpendBundle};
    use chia_wallet::{
        cat::cat_puzzle_hash,
        offer::SETTLEMENT_PAYMENTS_PUZZLE_HASH,
        standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
        DeriveSynthetic, LineageProof,
    };
    use clvmr::Allocator;
    use hex_literal::hex;

    use crate::{
        testing::SECRET_KEY, CatSpend, CreateCoinWithMemos, IssueCat, RequiredSignature,
        SpendContext, StandardSpend, WalletSimulator,
    };

    fn sk1() -> SecretKey {
        SECRET_KEY
            .derive_unhardened(0)
            .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
    }

    fn sk2() -> SecretKey {
        SECRET_KEY
            .derive_unhardened(1)
            .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
    }

    fn sign_tx(required_signatures: Vec<RequiredSignature>) -> Signature {
        let sk1 = sk1();
        let sk2 = sk2();

        let pk1 = sk1.public_key();
        let pk2 = sk2.public_key();

        let mut aggregated_signature = Signature::default();

        for req in required_signatures {
            if req.public_key() == &pk1 {
                let sig = sign(&sk1, &req.final_message());
                aggregated_signature += &sig;
            } else if req.public_key() == &pk2 {
                let sig = sign(&sk2, &req.final_message());
                aggregated_signature += &sig;
            } else {
                panic!("unexpected public key");
            }
        }

        aggregated_signature
    }

    #[tokio::test]
    async fn test_offer() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let pk1 = sk1().public_key();
        let pk2 = sk2().public_key();

        let ph1 = Bytes32::new(standard_puzzle_hash(&pk1));
        let ph2 = Bytes32::new(standard_puzzle_hash(&pk2));

        let xch1 = sim.generate_coin(ph1, 1000).await;

        // Issue CAT.
        let (issue_cat, cat_info) = IssueCat::new(xch1.coin.coin_id())
            .condition(ctx.alloc(CreateCoinWithMemos {
                puzzle_hash: ph1,
                amount: 1000,
                memos: vec![ph1.to_vec().into()],
            })?)
            .multi_issuance(&mut ctx, pk1.clone(), 1000)?;

        let coin_spends = StandardSpend::new().chain(issue_cat).finish(
            &mut ctx,
            xch1.coin.clone(),
            pk1.clone(),
        )?;

        let mut spend_bundle = SpendBundle::new(coin_spends, Signature::default());

        let required_signatures = RequiredSignature::from_coin_spends(
            &mut allocator,
            &spend_bundle.coin_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )?;

        spend_bundle.aggregated_signature = sign_tx(required_signatures);

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        // Create offer for CAT coin.
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let cat1_ph = cat_puzzle_hash(cat_info.asset_id.into(), ph1.into()).into();
        let cat1 = Coin::new(cat_info.eve_coin.coin_id(), cat1_ph, 1000);

        let xch2 = sim.generate_coin(ph2, 1000).await.coin;

        let requests = OfferBuilder::new(vec![cat1.coin_id(), xch2.coin_id()])
            .request_cat_payments(
                &mut ctx,
                cat_info.asset_id,
                vec![Payment::WithMemos(PaymentWithMemos {
                    puzzle_hash: ph2,
                    amount: 1000,
                    memos: vec![ph2.to_vec().into()],
                })],
            )?
            .finish();

        let mut coin_spends = requests.coin_spends;

        let xch_spends = StandardSpend::new()
            .condition(ctx.alloc(CreateCoinWithMemos {
                puzzle_hash: SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                amount: 1000,
                memos: vec![SETTLEMENT_PAYMENTS_PUZZLE_HASH.to_vec().into()],
            })?)
            .conditions(requests.parent_conditions)
            .finish(&mut ctx, xch2, pk2)?;

        let aggregated_signature = sign_tx(RequiredSignature::from_coin_spends(
            &mut allocator,
            &xch_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )?);

        coin_spends.extend(xch_spends);

        let spend_bundle = SpendBundle::new(coin_spends, aggregated_signature);
        let offer_data = compress_offer(spend_bundle)?;

        assert_eq!(hex::encode(&offer_data), hex::encode(EXAMPLE_OFFER_DATA));

        // Decompress offer and accept it (we know the only requested payment is the CAT).
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let offer = Offer::from(decompress_offer(&offer_data)?);

        let mut spend_bundle = offer.offered_spend_bundle;

        let lineage_proof = LineageProof {
            parent_coin_info: xch1.coin.coin_id(),
            inner_puzzle_hash: cat_info.eve_inner_puzzle_hash,
            amount: 1000,
        };

        let (inner_spend, _) = StandardSpend::new()
            .condition(ctx.alloc(CreateCoinWithMemos {
                puzzle_hash: SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
                amount: 1000,
                memos: vec![SETTLEMENT_PAYMENTS_PUZZLE_HASH.to_vec().into()],
            })?)
            .inner_spend(&mut ctx, pk1)?;

        let cat_spends = CatSpend::new(cat_info.asset_id)
            .spend(cat1, inner_spend, lineage_proof, 0)
            .finish(&mut ctx)?;

        spend_bundle.aggregated_signature += &sign_tx(RequiredSignature::from_coin_spends(
            &mut allocator,
            &cat_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )?);

        spend_bundle.coin_spends.extend(cat_spends);

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        Ok(())
    }

    pub const EXAMPLE_OFFER_DATA: [u8; 536] = hex!(
        "
        000678bb1ce2864b63606060622000726f47eb64fdea99caadb02b31f402df8f25f7274d2b562a6948cc62fff2cf7ddb5f983a783b75b4f61fadfd476bffd1da7fb4f61fadfdff0f54ed0f2edc1798effb9cf0aeb5d178dbdcc8a809b9476219d70798cf5d1b18b733b06b778dac5615541913a35ffc9c6b79778a6a3bea8e75083c0f7c30f34513ebfadb7c2aa5b367b2666442ab8ad1160511dd7f4688cc824572f653769c7cfbc2ce6ddbe2a95f0c549f2d7ec23d25e3e6139d83d3a7fe605704a6c3057d95755d3779e42f06ce3f129c14baf9b295e08dae6f3f9d7ef6bceecf9aef2dd4f7bf89f90511ca40167e50893bb869bbe4c3ee09c2ec76e1563d67bdc33cfd5edcd6ce7e18f22de58e1f0f2133204d18a075c88d98e1bc500f6cf086a6439c3d72dd7f9e30dfae15bf37ebf799901727cc3b3b265a8715dcdbee51f8f798c09d033c2b52fae767db9ae839d9891681b5364052fbff05e7f7df7b1be3b3e89e2df757859da7bdb7975899cebef46c61c7c539cfefff51bcfc1c1a7f8494815c63ff7f41f992f999193b4259b6987a1fffcb5550ef18bc29965f34f2f4c260fec87ae339cce0f069f9d3b2309a85f9f1c1236e7797fa3f6d64ffb539e2db7b6f8df36e69a71ffecbfe7ec27ea6d6c3d58edb3a8e941fc8793d375a70c2b28bba6fe30eefebf3c85c5f992bc8efc371b27f6e9cd9c2c0f2becb0db25b9eb6ac77bf67d63a6fd9a7d9c94f7f56a60100f2b976f4
        "
    );
}
