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
        spend_cat_coins, testing::SECRET_KEY, BaseSpend, CatSpend, CreateCoinWithMemos, IssueCat,
        RequiredSignature, SpendContext, StandardSpend, WalletSimulator,
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
    async fn test_chia_offer() {
        let offer = "offer1qqr83wcuu2rykcmqvpsxvgqqemhmlaekcenaz02ma6hs5w600dhjlvfjn477nkwz369h88kll73h37fefnwk3qqnz8s0lle07zdk52smt8k62xtc53n9hxt4gpn4p0qp27dka6wa40d7djd0mrn86p4gvngckfmt877hr0h3uzekcus20afa0xwx54l3vhmtttsajt047dh5dnv4llk7c5nl84aczcgwktrmuezkmqfv2du9x5h4l9v8g6lx2ynpn068d07an2fl4r308aexve0nrj0h7gxv9g3dk78h4l7nzhlu0sk9vm8h6m7xzetn44em6ml6gwgkctnsancnkwwqyfctf7fkmf7pkmd735md735mdl5wj6d5l9rtlesmhpc97c97aaeu9w7kgm3k6uer5vnwg85fv4wp7cp7wakkphxwczhd6ddjtf24qe0t56jj7h8dcwsmjcq6wlte6p5aeytj8n870eftrwezkzjrjuv0frexu6z4s9wvzr0824uxpqeh9jl0y2w09nsxdp4a0fzwelftr7wz6xqlvnnmewgucrtmrsmrglz20wnt46a0evatzt0084jtt369w2kvszx7pqsvdzqrfk079usdf7hmmuxx5wgs2crn948xl7py7cgt0d4wvdg6g5kanvwh0j048ay4chdnw8fsh8av3u247kpzpd3a5qc447llpv9xea270hg55yyvdlsz3w6e946dkl40jkr0acntya6880nwl2j9uks8f88079evuxkyeeg4h5jl9amvmncskjevzu7nk5mll7nue34kunaffmuhmg7shl30mryt2dmjxmlm8l7wjcmlwjwlvdx48zugm26arer2urmpj7e7yltv3xvpnh0lqluphgm07r8psudp2f6l9runwl4enqf2xfnd7ds607fh46jfvfumdeyq2q3dy02ve74lspgyxzm2ryc5dmfmenur2h8ultcanyuw6464483m0m3vdzu5r8mzrhmx4s24qwtqktur34pg2qupqz5y5hehvtpfwq5hfu4gzw0l6dh0ehkled97xtdu0lquhdhhuhg7w3q62y7clzen99tzmsmuyuk8esaan5w4xdam33r0amjxhkf3hlj4mcm3ga93jdekkwez8k3hu70cl2tnn443n4nd95004jlf7tyltwl7s26u9v79gfnznt75a06lmxvkptwnanuh6axqucwmhx6zfn8d7y8ldx26l33ytl6m4kxwgp0dsr4yml4hxr7l5qzdlmlgjr5hmx9shrq4pghtq4pnnluke3cjmr0wc4h2ctc79l5tgu2hlz5q4fk07tac4xaw7ewdl2ug8dgvllvhatt642kzhym5hjy78rx00nn5wg3t60km4sxpxhzhjcn2sffxd2ljuw449cfndeah8kluh2wmg00yxzhandnuhh4044ekwanejnaujwhtll4egdx6wqa8za4mwejkmxn0h46mzand7qjxvts5vrah4x94tk0dkg7hfglawvd9d02lla24v5neegdzlldk3ll7v8wdq5q8kgq4fcnu54uz";
        let offer_data = decode_offer(offer).unwrap();
        let spend_bundle = decompress_offer(&offer_data).unwrap();

        panic!("{:#?}", &spend_bundle);
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
        let (issue_cat, cat_info) = IssueCat::new(&mut ctx, xch1.coin.coin_id())
            .condition(CreateCoinWithMemos {
                puzzle_hash: ph1,
                amount: 1000,
                memos: vec![ph1.to_vec().into()],
            })?
            .multi_issuance(pk1.clone(), 1000)?;

        let coin_spends = StandardSpend::new(&mut ctx, xch1.coin.clone())
            .chain(issue_cat)
            .finish(pk1.clone())?;

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

        let requests = OfferBuilder::new(&mut ctx, vec![cat1.coin_id(), xch2.coin_id()])
            .request_cat_payments(
                cat_info.asset_id,
                vec![Payment::WithMemos(PaymentWithMemos {
                    puzzle_hash: ph2,
                    amount: 1000,
                    memos: vec![ph2.to_vec().into()],
                })],
            )?
            .finish();

        let mut coin_spends = requests.coin_spends;

        let xch_spends = StandardSpend::new(&mut ctx, xch2)
            .settlement_coin(1000)?
            .conditions(requests.assertions)?
            .finish(pk2)?;

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

        let conditions = ctx.alloc(vec![CreateCoinWithMemos {
            puzzle_hash: SETTLEMENT_PAYMENTS_PUZZLE_HASH.into(),
            amount: 1000,
            memos: vec![SETTLEMENT_PAYMENTS_PUZZLE_HASH.to_vec().into()],
        }])?;

        let cat_spend = spend_cat_coins(
            &mut ctx,
            cat_info.asset_id,
            &[CatSpend {
                coin: cat1,
                synthetic_key: pk1,
                conditions,
                p2_puzzle_hash: ph1,
                extra_delta: 0,
                lineage_proof: LineageProof {
                    parent_coin_info: xch1.coin.coin_id(),
                    inner_puzzle_hash: cat_info.eve_inner_puzzle_hash,
                    amount: 1000,
                },
            }],
        )?
        .remove(0);

        spend_bundle.aggregated_signature += &sign_tx(RequiredSignature::from_coin_spend(
            &mut allocator,
            &cat_spend,
            WalletSimulator::AGG_SIG_ME.into(),
        )?);

        spend_bundle.coin_spends.push(cat_spend);

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        Ok(())
    }

    pub const EXAMPLE_OFFER_DATA: [u8; 535] = hex!(
        "
        000678bb1ce2864b63606060622000726f47eb64fdea99caadb02b31f402df8f25f7274d2b562a6948cc62fff2cf7ddb5f983a783b75b4f61fadfd476bffd1da7fb4f61fadfdff0f54ed0f2edc1798effb9cf0aeb5d178dbdcc8a809b9476219d70798cf5d1b18b733b06b778dac5615541913a35ffc9c6b79778a6a3bea8e75083c0f7c30f34513ebfadb7c2aa5b367b2666442ab8ad1160511dd7f4688cc02b7736e8f8b26afbc77f7c16e9be7278c6b264f78aa9ec179f0eca3ecab768cec8cb7804afa2aebba6ef2c85f0c9c7f24382974f3652bc11b5ddf7e3afdec79dd9f35df5ba8ef7f13f30b2294812cfca0127770d376c987dd1384d9edc2ad7ace7a8779fabdb8ad9dfd30e45bca1d3f1e4266409a3040eb901b31c379a11ed8e00d4d87387be4baff3c61be5d2b7e6fd6ef33212f4e9877764cb40e2bb8b7dda3f0ef31813b077856a4f4cfcfb635d173b2132d026b6d80a4f6ff0bceefbff736c667d13d5beeaf0a3b4f7b6f2fb1329d7de9d9c28e8b739edfffa378f93934fe082903b9c6feff0213d765d75a5c6bf8f4fda66cb799c45174c24ff6ade792bdcea68b2326fbdcf27e0f0e9f29af827f9a54bc9f9dd411f939abe88e5de7b13501129a0106ae9eb78d97d65bd43d916f543fa1abad2b76ec958e7c91813ef7f4c0b4876bee6ed9b4e4e86cd5cd2bacd61f7bb6a2a13e67dff2e902ae0b3acf773818f84cde55fb6fc7f6b2905dd676df5f0300bd5673c3
        "
    );
}
