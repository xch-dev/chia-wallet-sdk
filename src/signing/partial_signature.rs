use chia_bls::Signature;

use crate::RequiredSignature;

#[derive(Debug, Clone, Default)]
pub struct PartialSignature {
    pub signature: Signature,
    pub missing_signatures: Vec<RequiredSignature>,
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
