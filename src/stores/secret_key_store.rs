use std::future::Future;

use chia_bls::{PublicKey, SecretKey};

use crate::PublicKeyStore;

/// Keeps track of and derives wallet secret keys by index.
pub trait SecretKeyStore: PublicKeyStore + Sync {
    /// Gets the secret key at a given index.
    fn secret_key(&self, index: u32) -> impl Future<Output = Option<SecretKey>> + Send;

    /// Gets the derivation index of a secret key.
    fn index_of_sk(&self, secret_key: &SecretKey) -> impl Future<Output = Option<u32>> + Send {
        let public_key = secret_key.public_key();
        async move { self.index_of_pk(&public_key).await }
    }

    /// Gets the secret key for a given public key.
    fn to_secret_key(
        &self,
        public_key: &PublicKey,
    ) -> impl Future<Output = Option<SecretKey>> + Send {
        let public_key = public_key.clone();
        async move {
            let index = self.index_of_pk(&public_key).await?;
            self.secret_key(index).await
        }
    }
}
