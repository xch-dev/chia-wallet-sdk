use chia_protocol::Bytes32;
use chia_puzzle_types::{did::DidArgs, singleton::SingletonStruct};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{DidLayer, DriverError, HashedPtr, Layer, Puzzle, SingletonInfo, SingletonLayer};

pub type StandardDidLayers<I> = SingletonLayer<DidLayer<HashedPtr, I>>;

/// Information needed to construct the outer puzzle of a DID.
/// It does not include the inner puzzle, which must be stored separately.
///
/// This type can be used on its own for parsing, or as part of the [`Did`](crate::Did) primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DidInfo {
    /// The coin id of the launcher coin that created this DID's singleton.
    pub launcher_id: Bytes32,

    /// The hash of the recovery list. This is a very infrequently used feature
    /// and is not fully supported at this time.
    ///
    /// In the Chia reference wallet, the recovery list hash must be present
    /// even if recovery is disabled. However, in some other wallets it's allowed
    /// to be [`None`]. This is an on-chain cost optimization and simplification.
    pub recovery_list_hash: Option<Bytes32>,

    /// The number of verifications required to recover the DID.
    pub num_verifications_required: u64,

    /// The metadata stored in the [`DidLayer`]. This can be updated freely,
    /// but must be confirmed by an additional update spend to ensure wallets
    /// can sync it from the parent coin.
    pub metadata: HashedPtr,

    /// The hash of the inner puzzle to this DID.
    /// If you encode this puzzle hash as bech32m, it's the same as the current owner's address.
    pub p2_puzzle_hash: Bytes32,
}

impl DidInfo {
    pub fn new(
        launcher_id: Bytes32,
        recovery_list_hash: Option<Bytes32>,
        num_verifications_required: u64,
        metadata: HashedPtr,
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

    /// Parses a [`DidInfo`] from a [`Puzzle`] by extracting the [`DidLayer`].
    ///
    /// This will return a tuple of the [`DidInfo`] and its p2 puzzle.
    ///
    /// If the puzzle is not a DID, this will return [`None`] instead of an error.
    /// However, if the puzzle should have been a DID but had a parsing error, this will return an error.
    pub fn parse(
        allocator: &Allocator,
        puzzle: Puzzle,
    ) -> Result<Option<(Self, Puzzle)>, DriverError> {
        let Some(layers) = StandardDidLayers::<Puzzle>::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        let p2_puzzle = layers.inner_puzzle.inner_puzzle;

        Ok(Some((Self::from_layers(&layers), p2_puzzle)))
    }

    pub fn from_layers<I>(layers: &StandardDidLayers<I>) -> Self
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
    pub fn into_layers<I>(self, p2_puzzle: I) -> StandardDidLayers<I> {
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
}

impl SingletonInfo for DidInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
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
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use clvm_traits::ToClvm;

    use crate::{Launcher, SpendContext, StandardLayer};

    use super::*;

    #[test]
    fn test_parse_did_info() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let custom_metadata = ctx.alloc_hashed(&["Metadata".to_string(), "Example".to_string()])?;
        let (create_did, did) = Launcher::new(alice.coin.coin_id(), 1).create_did(
            ctx,
            None,
            1,
            custom_metadata,
            &alice_p2,
        )?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        let original_did = did;
        let _did = did.update(ctx, &alice_p2, Conditions::new())?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let puzzle_reveal = sim
            .puzzle_reveal(original_did.coin.coin_id())
            .expect("missing did puzzle");

        let mut allocator = Allocator::new();
        let ptr = puzzle_reveal.to_clvm(&mut allocator)?;
        let puzzle = Puzzle::parse(&allocator, ptr);
        let (did_info, p2_puzzle) = DidInfo::parse(&allocator, puzzle)?.expect("not a did");

        assert_eq!(did_info, original_did.info);
        assert_eq!(p2_puzzle.curried_puzzle_hash(), alice.puzzle_hash.into());

        Ok(())
    }
}
