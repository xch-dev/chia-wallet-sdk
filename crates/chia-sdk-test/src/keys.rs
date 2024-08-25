use bip39::Mnemonic;
use chia_bls::SecretKey;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub fn test_secret_keys(no_keys: usize) -> Result<Vec<SecretKey>, bip39::Error> {
    let mut rng = ChaCha8Rng::seed_from_u64(0);

    let mut keys = Vec::with_capacity(no_keys);

    for _ in 0..no_keys {
        let entropy: [u8; 32] = rng.gen();
        let mnemonic = Mnemonic::from_entropy(&entropy)?;
        let seed = mnemonic.to_seed("");
        let sk = SecretKey::from_seed(&seed);
        keys.push(sk);
    }

    Ok(keys)
}

#[allow(clippy::missing_panics_doc)]
pub fn test_secret_key() -> Result<SecretKey, bip39::Error> {
    Ok(test_secret_keys(1)?
        .pop()
        .expect("Unable to get secret key"))
}
