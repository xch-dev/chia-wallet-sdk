use bindy::Result;
use chia_protocol::{Bytes32, BytesImpl};
use chia_secp::{
    K1PublicKey as K1PublicKeyRs, K1SecretKey as K1SecretKeyRs, K1Signature as K1SignatureRs,
    R1PublicKey as R1PublicKeyRs, R1SecretKey as R1SecretKeyRs, R1Signature as R1SignatureRs,
};

#[derive(Clone)]
pub struct K1SecretKey(pub K1SecretKeyRs);

impl K1SecretKey {
    pub fn from_bytes(bytes: Bytes32) -> Result<Self> {
        Ok(Self(K1SecretKeyRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<Bytes32> {
        Ok(Bytes32::new(self.0.to_bytes()))
    }

    pub fn public_key(&self) -> Result<K1PublicKey> {
        Ok(K1PublicKey(self.0.public_key()))
    }

    pub fn sign_prehashed(&self, prehashed: Bytes32) -> Result<K1Signature> {
        Ok(K1Signature(self.0.sign_prehashed(&prehashed.to_bytes())?))
    }
}

#[derive(Clone, Copy)]
pub struct K1PublicKey(pub K1PublicKeyRs);

impl K1PublicKey {
    pub fn from_bytes(bytes: BytesImpl<33>) -> Result<Self> {
        Ok(Self(K1PublicKeyRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<BytesImpl<33>> {
        Ok(BytesImpl::new(self.0.to_bytes()))
    }

    pub fn fingerprint(&self) -> Result<u32> {
        Ok(self.0.fingerprint())
    }

    pub fn verify_prehashed(&self, prehashed: Bytes32, signature: K1Signature) -> Result<bool> {
        Ok(self.0.verify_prehashed(&prehashed.to_bytes(), &signature.0))
    }
}

#[derive(Clone, Copy)]
pub struct K1Signature(pub K1SignatureRs);

impl K1Signature {
    pub fn from_bytes(bytes: BytesImpl<64>) -> Result<Self> {
        Ok(Self(K1SignatureRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<BytesImpl<64>> {
        Ok(BytesImpl::new(self.0.to_bytes()))
    }
}

#[derive(Clone)]
pub struct R1SecretKey(pub R1SecretKeyRs);

impl R1SecretKey {
    pub fn from_bytes(bytes: Bytes32) -> Result<Self> {
        Ok(Self(R1SecretKeyRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<Bytes32> {
        Ok(Bytes32::new(self.0.to_bytes()))
    }

    pub fn public_key(&self) -> Result<R1PublicKey> {
        Ok(R1PublicKey(self.0.public_key()))
    }

    pub fn sign_prehashed(&self, prehashed: Bytes32) -> Result<R1Signature> {
        Ok(R1Signature(self.0.sign_prehashed(&prehashed.to_bytes())?))
    }
}

#[derive(Clone, Copy)]
pub struct R1PublicKey(pub R1PublicKeyRs);

impl R1PublicKey {
    pub fn from_bytes(bytes: BytesImpl<33>) -> Result<Self> {
        Ok(Self(R1PublicKeyRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<BytesImpl<33>> {
        Ok(BytesImpl::new(self.0.to_bytes()))
    }

    pub fn fingerprint(&self) -> Result<u32> {
        Ok(self.0.fingerprint())
    }

    pub fn verify_prehashed(&self, prehashed: Bytes32, signature: R1Signature) -> Result<bool> {
        Ok(self.0.verify_prehashed(&prehashed.to_bytes(), &signature.0))
    }
}

#[derive(Clone, Copy)]
pub struct R1Signature(pub R1SignatureRs);

impl R1Signature {
    pub fn from_bytes(bytes: BytesImpl<64>) -> Result<Self> {
        Ok(Self(R1SignatureRs::from_bytes(&bytes.to_bytes())?))
    }

    pub fn to_bytes(&self) -> Result<BytesImpl<64>> {
        Ok(BytesImpl::new(self.0.to_bytes()))
    }
}
