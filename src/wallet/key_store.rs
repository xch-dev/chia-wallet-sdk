use std::future::Future;

use chia_bls::PublicKey;

/// The necessary methods required for finding key information.
pub trait KeyStore {
    /// The error type for this key store.
    type Error;

    /// Returns the index for a given puzzle hash.
    fn pk_index(
        &mut self,
        pk: &PublicKey,
    ) -> impl Future<Output = Result<Option<u32>, Self::Error>>;
}
