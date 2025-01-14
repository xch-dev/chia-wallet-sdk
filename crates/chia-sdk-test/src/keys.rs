use bip39::Mnemonic;
use chia_bls::SecretKey;
use chia_secp::{K1SecretKey, R1SecretKey};
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

pub fn test_secret_key() -> Result<SecretKey, bip39::Error> {
    Ok(test_secret_keys(1)?
        .pop()
        .expect("Unable to get secret key"))
}

pub fn test_k1_keys(no_keys: usize) -> Result<Vec<K1SecretKey>, signature::Error> {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let mut keys = Vec::with_capacity(no_keys);

    for _ in 0..no_keys {
        keys.push(K1SecretKey::from_bytes(&rng.gen())?);
    }

    Ok(keys)
}

pub fn test_k1_key() -> Result<K1SecretKey, signature::Error> {
    Ok(test_k1_keys(1)?.pop().expect("Unable to get secret key"))
}

pub fn test_r1_keys(no_keys: usize) -> Result<Vec<R1SecretKey>, signature::Error> {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let mut keys = Vec::with_capacity(no_keys);

    for _ in 0..no_keys {
        keys.push(R1SecretKey::from_bytes(&rng.gen())?);
    }

    Ok(keys)
}

pub fn test_r1_key() -> Result<R1SecretKey, signature::Error> {
    Ok(test_r1_keys(1)?.pop().expect("Unable to get secret key"))
}
