use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes32, SpendBundle};
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment as SdkNotarizedPayment, Payment as SdkPayment},
};
use chia_sdk_driver::{AssetInfo, Offer, RequestedPayments, SpendContext};

use crate::{AsProgram, Program};

pub fn encode_offer(spend_bundle: SpendBundle) -> Result<String> {
    Ok(chia_sdk_driver::encode_offer(&spend_bundle)?)
}

pub fn decode_offer(offer: String) -> Result<SpendBundle> {
    Ok(chia_sdk_driver::decode_offer(&offer)?)
}

pub fn validate_offer(offer: String) -> Result<()> {
    chia_sdk_driver::validate_offer_str(&offer)?;
    Ok(())
}

pub fn from_input_spend_bundle(
    spend_bundle: SpendBundle,
    requested_payments_xch: Vec<NotarizedPayment>,
) -> Result<SpendBundle> {
    let mut ctx = SpendContext::new();

    let mut requested_payments = RequestedPayments::new();
    requested_payments.xch = requested_payments_xch
        .into_iter()
        .map(Into::into)
        .collect();

    let offer = Offer::from_input_spend_bundle(
        &mut ctx,
        spend_bundle,
        requested_payments,
        AssetInfo::new(),
    )?;

    Ok(offer.to_spend_bundle(&mut ctx)?)
}

#[derive(Clone)]
pub struct NotarizedPayment {
    pub nonce: Bytes32,
    pub payments: Vec<Payment>,
}

impl AsProgram for SdkNotarizedPayment {
    type AsProgram = NotarizedPayment;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        NotarizedPayment {
            nonce: self.nonce,
            payments: self.payments.iter().map(|p| p.as_program(clvm)).collect(),
        }
    }
}

impl From<NotarizedPayment> for SdkNotarizedPayment {
    fn from(notarized_payment: NotarizedPayment) -> Self {
        Self::new(
            notarized_payment.nonce,
            notarized_payment
                .payments
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }
}

#[derive(Clone)]
pub struct Payment {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub memos: Option<Program>,
}

impl AsProgram for SdkPayment {
    type AsProgram = Payment;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        Payment {
            puzzle_hash: self.puzzle_hash,
            amount: self.amount,
            memos: match self.memos {
                Memos::Some(memos) => Some(memos.as_program(clvm)),
                Memos::None => None,
            },
        }
    }
}

impl From<Payment> for SdkPayment {
    fn from(payment: Payment) -> Self {
        Self::new(
            payment.puzzle_hash,
            payment.amount,
            payment
                .memos
                .as_ref()
                .map_or(Memos::None, |m| Memos::Some(m.1)),
        )
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::Signature;
    use chia_protocol::Bytes32;

    use super::*;

    #[test]
    fn validate_offer_accepts_generated_offer() {
        let generated_offer = from_input_spend_bundle(
            SpendBundle::new(Vec::new(), Signature::default()),
            vec![NotarizedPayment {
                nonce: Bytes32::new([1; 32]),
                payments: vec![Payment {
                    puzzle_hash: Bytes32::new([2; 32]),
                    amount: 42,
                    memos: None,
                }],
            }],
        )
        .unwrap();

        let mut ctx = SpendContext::new();
        let parsed = Offer::from_spend_bundle(&mut ctx, &generated_offer).unwrap();
        assert_eq!(parsed.spend_bundle().coin_spends.len(), 0);
        assert_eq!(parsed.requested_payments().xch.len(), 1);
        assert_eq!(parsed.requested_payments().xch[0].payments[0].amount, 42);

        let encoded = encode_offer(generated_offer).unwrap();
        validate_offer(encoded).unwrap();
    }

    #[test]
    fn validate_offer_rejects_invalid_string() {
        assert!(validate_offer("not-an-offer".to_string()).is_err());
    }
}
