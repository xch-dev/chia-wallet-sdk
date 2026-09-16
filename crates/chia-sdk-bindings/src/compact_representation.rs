pub trait CompactLineageProofExt
where
    Self: Sized,
{
    fn from_lineage_proof(proof: chia_puzzle_types::LineageProof) -> bindy::Result<Self>;
}

impl CompactLineageProofExt for chia_sdk_types::puzzles::CompactLineageProof {
    fn from_lineage_proof(proof: chia_puzzle_types::LineageProof) -> bindy::Result<Self> {
        Ok(proof.into())
    }
}

pub trait CompactCoinProofExt {}

impl CompactCoinProofExt for chia_sdk_types::puzzles::CompactCoinProof {}
