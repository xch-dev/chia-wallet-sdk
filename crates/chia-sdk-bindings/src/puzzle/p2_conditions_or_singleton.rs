use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_driver::{P2ConditionsOrSingleton, P2ConditionsOrSingletonVersion};
use clvm_utils::{ToTreeHash, TreeHash};

pub trait P2ConditionsOrSingletonExt: Sized {
    fn from_quoted_conditions_hash(
        launcher_id: Bytes32,
        nonce: usize,
        quoted_conditions_hash: Bytes32,
        version: P2ConditionsOrSingletonVersion,
    ) -> Result<Self>;
    fn calculate_fixed_conditions_hash(
        quoted_conditions_hash: Bytes32,
        version: P2ConditionsOrSingletonVersion,
    ) -> Result<Bytes32>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
}

impl P2ConditionsOrSingletonExt for P2ConditionsOrSingleton {
    fn from_quoted_conditions_hash(
        launcher_id: Bytes32,
        nonce: usize,
        quoted_conditions_hash: Bytes32,
        version: P2ConditionsOrSingletonVersion,
    ) -> Result<Self> {
        Ok(Self::from_quoted_conditions_hash(
            launcher_id,
            nonce,
            quoted_conditions_hash,
            version,
        ))
    }

    fn calculate_fixed_conditions_hash(
        quoted_conditions_hash: Bytes32,
        version: P2ConditionsOrSingletonVersion,
    ) -> Result<Bytes32> {
        Ok(Self::calculate_fixed_conditions_hash(
            quoted_conditions_hash,
            version,
        ))
    }

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(self.tree_hash())
    }
}
