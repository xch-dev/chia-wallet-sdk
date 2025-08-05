use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes32, SpendBundle};
use chia_puzzle_types::{
    offer::{NotarizedPayment as SdkNotarizedPayment, Payment as SdkPayment},
    Memos,
};
use chia_sdk_driver::SpendContext;
use clvmr::Allocator;

use crate::{AsProgram, AsPtr, Program};

pub fn encode_offer(spend_bundle: SpendBundle) -> Result<String> {
    Ok(chia_sdk_driver::encode_offer(&spend_bundle)?)
}

pub fn decode_offer(offer: String) -> Result<SpendBundle> {
    Ok(chia_sdk_driver::decode_offer(&offer)?)
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

impl AsPtr for NotarizedPayment {
    type AsPtr = SdkNotarizedPayment;

    fn as_ptr(&self, allocator: &Allocator) -> Self::AsPtr {
        SdkNotarizedPayment::new(
            self.nonce,
            self.payments.iter().map(|p| p.as_ptr(allocator)).collect(),
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

impl AsPtr for Payment {
    type AsPtr = SdkPayment;

    fn as_ptr(&self, _allocator: &Allocator) -> Self::AsPtr {
        SdkPayment::new(
            self.puzzle_hash,
            self.amount,
            self.memos
                .as_ref()
                .map_or(Memos::None, |m| Memos::Some(m.1)),
        )
    }
}
