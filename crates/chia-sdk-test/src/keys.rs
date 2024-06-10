use bip39::Mnemonic;
use chia_bls::SecretKey;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub fn secret_key() -> Result<SecretKey, bip39::Error> {
    let mut rng = ChaCha8Rng::seed_from_u64(0);
    let entropy: [u8; 32] = rng.gen();
    let mnemonic = Mnemonic::from_entropy(&entropy)?;
    let seed = mnemonic.to_seed("");
    Ok(SecretKey::from_seed(&seed))
}
