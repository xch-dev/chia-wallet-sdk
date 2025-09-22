use bindy::Result;
use chia_bls::{aggregate_verify, sign, verify, DerivableKey, PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes, Bytes32, Bytes48, Bytes96};
use chia_puzzle_types::DeriveSynthetic;

pub trait SecretKeyExt: Sized {
    fn from_seed(seed: Bytes) -> Result<Self>;
    fn from_bytes(bytes: Bytes32) -> Result<Self>;
    fn to_bytes(&self) -> Result<Bytes32>;
    fn public_key(&self) -> Result<PublicKey>;
    fn sign(&self, message: Bytes) -> Result<Signature>;
    fn derive_unhardened(&self, index: u32) -> Result<Self>;
    fn derive_hardened(&self, index: u32) -> Result<Self>;
    fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self>;
    fn derive_hardened_path(&self, path: Vec<u32>) -> Result<Self>;
    fn derive_synthetic(&self) -> Result<Self>;
    fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Bytes32) -> Result<Self>;
}

impl SecretKeyExt for SecretKey {
    fn from_seed(seed: Bytes) -> Result<Self> {
        Ok(Self::from_seed(&seed))
    }

    fn from_bytes(bytes: Bytes32) -> Result<Self> {
        Ok(Self::from_bytes(&bytes.to_bytes())?)
    }

    fn to_bytes(&self) -> Result<Bytes32> {
        Ok(Bytes32::new(self.to_bytes()))
    }

    fn public_key(&self) -> Result<PublicKey> {
        Ok(self.public_key())
    }

    fn sign(&self, message: Bytes) -> Result<Signature> {
        Ok(sign(self, message))
    }

    fn derive_unhardened(&self, index: u32) -> Result<Self> {
        Ok(DerivableKey::derive_unhardened(self, index))
    }

    fn derive_hardened(&self, index: u32) -> Result<Self> {
        Ok(self.derive_hardened(index))
    }

    fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self> {
        let mut result = self.clone();

        for index in path {
            result = DerivableKey::derive_unhardened(&result, index);
        }

        Ok(result)
    }

    fn derive_hardened_path(&self, path: Vec<u32>) -> Result<Self> {
        let mut result = self.clone();

        for index in path {
            result = result.derive_hardened(index);
        }

        Ok(result)
    }

    fn derive_synthetic(&self) -> Result<Self> {
        Ok(DeriveSynthetic::derive_synthetic(self))
    }

    fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(DeriveSynthetic::derive_synthetic_hidden(
            self,
            &hidden_puzzle_hash.to_bytes(),
        ))
    }
}

pub trait PublicKeyExt: Sized {
    fn infinity() -> Result<Self>;
    fn aggregate(public_keys: Vec<Self>) -> Result<Self>;
    fn aggregate_verify(
        public_keys: Vec<Self>,
        messages: Vec<Bytes>,
        signature: Signature,
    ) -> Result<bool>;
    fn from_bytes(bytes: Bytes48) -> Result<Self>;
    fn to_bytes(&self) -> Result<Bytes48>;
    fn verify(&self, message: Bytes, signature: Signature) -> Result<bool>;
    fn fingerprint(&self) -> Result<u32>;
    fn is_infinity(&self) -> Result<bool>;
    fn is_valid(&self) -> Result<bool>;
    fn derive_unhardened(&self, index: u32) -> Result<Self>;
    fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self>;
    fn derive_synthetic(&self) -> Result<Self>;
    fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Bytes32) -> Result<Self>;
}

impl PublicKeyExt for PublicKey {
    fn infinity() -> Result<Self> {
        Ok(Self::default())
    }

    fn aggregate(mut public_keys: Vec<Self>) -> Result<Self> {
        if public_keys.is_empty() {
            return Self::infinity();
        }

        let mut result = public_keys.remove(0);

        for pk in public_keys {
            result += &pk;
        }

        Ok(result)
    }

    fn aggregate_verify(
        public_keys: Vec<Self>,
        messages: Vec<Bytes>,
        signature: Signature,
    ) -> Result<bool> {
        Ok(aggregate_verify(
            &signature,
            public_keys.iter().zip(messages.iter().map(Bytes::as_slice)),
        ))
    }

    fn from_bytes(bytes: Bytes48) -> Result<Self> {
        Ok(Self::from_bytes(&bytes.to_bytes())?)
    }

    fn to_bytes(&self) -> Result<Bytes48> {
        Ok(Bytes48::new(self.to_bytes()))
    }

    fn verify(&self, message: Bytes, signature: Signature) -> Result<bool> {
        Ok(verify(&signature, self, message))
    }

    fn fingerprint(&self) -> Result<u32> {
        Ok(self.get_fingerprint())
    }

    fn is_infinity(&self) -> Result<bool> {
        Ok(self.is_inf())
    }

    fn is_valid(&self) -> Result<bool> {
        Ok(self.is_valid())
    }

    fn derive_unhardened(&self, index: u32) -> Result<Self> {
        Ok(DerivableKey::derive_unhardened(self, index))
    }

    fn derive_unhardened_path(&self, path: Vec<u32>) -> Result<Self> {
        let mut result = *self;

        for index in path {
            result = DerivableKey::derive_unhardened(&result, index);
        }

        Ok(result)
    }

    fn derive_synthetic(&self) -> Result<Self> {
        Ok(DeriveSynthetic::derive_synthetic(self))
    }

    fn derive_synthetic_hidden(&self, hidden_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(DeriveSynthetic::derive_synthetic_hidden(
            self,
            &hidden_puzzle_hash.to_bytes(),
        ))
    }
}

pub trait SignatureExt: Sized {
    fn infinity() -> Result<Self>;
    fn aggregate(signatures: Vec<Self>) -> Result<Self>;
    fn from_bytes(bytes: Bytes96) -> Result<Self>;
    fn to_bytes(&self) -> Result<Bytes96>;
    fn is_infinity(&self) -> Result<bool>;
    fn is_valid(&self) -> Result<bool>;
}

impl SignatureExt for Signature {
    fn infinity() -> Result<Self> {
        Ok(Self::default())
    }

    fn aggregate(mut signatures: Vec<Self>) -> Result<Self> {
        if signatures.is_empty() {
            return Self::infinity();
        }

        let mut result = signatures.remove(0);

        for sig in signatures {
            result += &sig;
        }

        Ok(result)
    }

    fn from_bytes(bytes: Bytes96) -> Result<Self> {
        Ok(Self::from_bytes(&bytes.to_bytes())?)
    }

    fn to_bytes(&self) -> Result<Bytes96> {
        Ok(Bytes96::new(self.to_bytes()))
    }

    fn is_infinity(&self) -> Result<bool> {
        Ok(self == &Self::default())
    }

    fn is_valid(&self) -> Result<bool> {
        Ok(self.is_valid())
    }
}
