use chia_bls::PublicKey;

pub trait PuzzleGenerator: Send + Sync {
    fn puzzle_hash(&self, public_key: &PublicKey) -> [u8; 32];
}
