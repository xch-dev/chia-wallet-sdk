use chia_protocol::Bytes32;
use chia_puzzles::{did::DidArgs, singleton::SingletonStruct};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{DidLayer, DriverError, Layer, Puzzle, SingletonLayer};

pub type StandardDidLayers<M, I> = SingletonLayer<DidLayer<M, I>>;

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DidInfo<M> {
    pub launcher_id: Bytes32,
    pub recovery_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: M,
    pub p2_puzzle_hash: Bytes32,
}

impl<M> DidInfo<M> {
    pub fn new(
        launcher_id: Bytes32,
        recovery_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            recovery_list_hash,
            num_verifications_required,
            metadata,
            p2_puzzle_hash,
        }
    }

    /// Parses the DID info and p2 puzzle that corresponds to the p2 puzzle hash.
    pub fn parse(
        allocator: &Allocator,
        puzzle: Puzzle,
    ) -> Result<Option<(Self, Puzzle)>, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator>,
    {
        let Some(layers) = StandardDidLayers::<M, Puzzle>::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        let p2_puzzle = layers.inner_puzzle.inner_puzzle;

        Ok(Some((Self::from_layers(layers), p2_puzzle)))
    }

    pub fn from_layers<I>(layers: StandardDidLayers<M, I>) -> Self
    where
        I: ToTreeHash,
    {
        Self {
            launcher_id: layers.launcher_id,
            recovery_list_hash: layers.inner_puzzle.recovery_list_hash,
            num_verifications_required: layers.inner_puzzle.num_verifications_required,
            metadata: layers.inner_puzzle.metadata,
            p2_puzzle_hash: layers.inner_puzzle.inner_puzzle.tree_hash().into(),
        }
    }

    #[must_use]
    pub fn into_layers<I>(self, p2_puzzle: I) -> StandardDidLayers<M, I> {
        SingletonLayer::new(
            self.launcher_id,
            DidLayer::new(
                self.launcher_id,
                self.recovery_list_hash,
                self.num_verifications_required,
                self.metadata,
                p2_puzzle,
            ),
        )
    }

    pub fn with_metadata<N>(self, metadata: N) -> DidInfo<N> {
        DidInfo {
            launcher_id: self.launcher_id,
            recovery_list_hash: self.recovery_list_hash,
            num_verifications_required: self.num_verifications_required,
            metadata,
            p2_puzzle_hash: self.p2_puzzle_hash,
        }
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash
    where
        M: ToTreeHash,
    {
        DidArgs::curry_tree_hash(
            self.p2_puzzle_hash.into(),
            self.recovery_list_hash,
            self.num_verifications_required,
            SingletonStruct::new(self.launcher_id),
            self.metadata.tree_hash(),
        )
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Coin;
    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::test_secret_key;

    use crate::{Launcher, SpendContext, StandardLayer};

    use super::*;

    #[test]
    fn test_parse_did() -> anyhow::Result<()> {
        let pk = test_secret_key()?.public_key();
        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = Coin::new(Bytes32::default(), puzzle_hash, 1);
        let ctx = &mut SpendContext::new();

        let custom_metadata = vec!["Metadata".to_string(), "Example".to_string()];
        let (create_did, did) = Launcher::new(coin.coin_id(), 1).create_did(
            ctx,
            Bytes32::default(),
            1,
            custom_metadata,
            &StandardLayer::new(pk),
        )?;

        Ok(())
    }
}
