use std::str::FromStr;

use bindy::Result;
use bip39::Mnemonic;
use chia_protocol::Bytes;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

pub fn mnemonic_from_entropy(entropy: Bytes) -> Result<String> {
    Ok(Mnemonic::from_entropy(&entropy)?.to_string())
}

pub fn mnemonic_to_entropy(mnemonic: String) -> Result<Bytes> {
    Ok(Mnemonic::from_str(&mnemonic)?.to_entropy().into())
}

pub fn verify_mnemonic(mnemonic: String) -> Result<bool> {
    Ok(Mnemonic::from_str(&mnemonic).is_ok())
}

pub fn generate_bytes(bytes: usize) -> Result<Bytes> {
    let mut rng = ChaCha20Rng::from_entropy();
    let mut buffer = vec![0; bytes];
    rng.fill_bytes(&mut buffer);
    Ok(buffer.into())
}

pub fn generate_mnemonic(use_24: bool) -> Result<String> {
    let mut rng = ChaCha20Rng::from_entropy();

    let mnemonic = if use_24 {
        let entropy: [u8; 32] = rng.gen();
        Mnemonic::from_entropy(&entropy)?
    } else {
        let entropy: [u8; 16] = rng.gen();
        Mnemonic::from_entropy(&entropy)?
    };

    Ok(mnemonic.to_string())
}

pub fn mnemonic_to_seed(mnemonic: String, password: String) -> Result<Bytes> {
    let mnemonic = Mnemonic::from_str(&mnemonic)?;
    Ok(mnemonic.to_seed(password).to_vec().into())
}
