use chia_bls::PublicKey;

pub trait PuzzleGenerator: Send + Sync {
    fn puzzle_hash(public_key: &PublicKey) -> [u8; 32];
}
