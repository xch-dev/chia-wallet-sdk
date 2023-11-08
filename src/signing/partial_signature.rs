use chia_bls::Signature;

use crate::RequiredSignature;

#[derive(Debug, Clone, Default)]
pub struct PartialSignature {
    pub signature: Signature,
    pub missing_signatures: Vec<RequiredSignature>,
}

impl PartialSignature {
    pub fn new(signature: Signature, missing_signatures: Vec<RequiredSignature>) -> Self {
        Self {
            signature,
            missing_signatures,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.missing_signatures.is_empty()
    }

    pub fn into_complete_signature(self) -> Option<Signature> {
        self.is_complete().then_some(self.signature)
    }

    pub fn unwrap(self) -> Signature {
        self.into_complete_signature().unwrap()
    }
}

impl std::ops::Add for PartialSignature {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            signature: self.signature + &rhs.signature,
            missing_signatures: [self.missing_signatures, rhs.missing_signatures].concat(),
        }
    }
}
