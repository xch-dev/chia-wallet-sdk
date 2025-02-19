use chia_protocol::Bytes32;
use chia_puzzle_types::nft::{NftOwnershipLayerArgs, NftStateLayerArgs};
use chia_puzzles::NFT_STATE_LAYER_HASH;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::Allocator;

use crate::{
    DriverError, Layer, NftOwnershipLayer, NftStateLayer, Puzzle, RoyaltyTransferLayer,
    SingletonLayer,
};

pub type StandardNftLayers<M, I> =
    SingletonLayer<NftStateLayer<M, NftOwnershipLayer<RoyaltyTransferLayer, I>>>;

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftInfo<M> {
    pub launcher_id: Bytes32,
    pub metadata: M,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Bytes32,
}

impl<M> NftInfo<M> {
    pub fn new(
        launcher_id: Bytes32,
        metadata: M,
        metadata_updater_puzzle_hash: Bytes32,
        current_owner: Option<Bytes32>,
        royalty_puzzle_hash: Bytes32,
        royalty_ten_thousandths: u16,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            metadata,
            metadata_updater_puzzle_hash,
            current_owner,
            royalty_puzzle_hash,
            royalty_ten_thousandths,
            p2_puzzle_hash,
        }
    }

    /// Parses the NFT info and p2 puzzle that corresponds to the p2 puzzle hash.
    pub fn parse(
        allocator: &Allocator,
        puzzle: Puzzle,
    ) -> Result<Option<(Self, Puzzle)>, DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator>,
    {
        let Some(layers) = StandardNftLayers::<M, Puzzle>::parse_puzzle(allocator, puzzle)? else {
            return Ok(None);
        };

        let p2_puzzle = layers.inner_puzzle.inner_puzzle.inner_puzzle;

        Ok(Some((Self::from_layers(layers), p2_puzzle)))
    }

    pub fn from_layers<I>(layers: StandardNftLayers<M, I>) -> Self
    where
        I: ToTreeHash,
    {
        Self {
            launcher_id: layers.launcher_id,
            metadata: layers.inner_puzzle.metadata,
            metadata_updater_puzzle_hash: layers.inner_puzzle.metadata_updater_puzzle_hash,
            current_owner: layers.inner_puzzle.inner_puzzle.current_owner,
            royalty_puzzle_hash: layers
                .inner_puzzle
                .inner_puzzle
                .transfer_layer
                .royalty_puzzle_hash,
            royalty_ten_thousandths: layers
                .inner_puzzle
                .inner_puzzle
                .transfer_layer
                .royalty_ten_thousandths,
            p2_puzzle_hash: layers
                .inner_puzzle
                .inner_puzzle
                .inner_puzzle
                .tree_hash()
                .into(),
        }
    }

    #[must_use]
    pub fn into_layers<I>(self, p2_puzzle: I) -> StandardNftLayers<M, I> {
        SingletonLayer::new(
            self.launcher_id,
            NftStateLayer::new(
                self.metadata,
                self.metadata_updater_puzzle_hash,
                NftOwnershipLayer::new(
                    self.current_owner,
                    RoyaltyTransferLayer::new(
                        self.launcher_id,
                        self.royalty_puzzle_hash,
                        self.royalty_ten_thousandths,
                    ),
                    p2_puzzle,
                ),
            ),
        )
    }

    pub fn with_metadata<N>(self, metadata: N) -> NftInfo<N> {
        NftInfo {
            launcher_id: self.launcher_id,
            metadata,
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            current_owner: self.current_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_ten_thousandths: self.royalty_ten_thousandths,
            p2_puzzle_hash: self.p2_puzzle_hash,
        }
    }

    pub fn with_p2_puzzle_hash(self, p2_puzzle_hash: Bytes32) -> Self {
        Self {
            p2_puzzle_hash,
            ..self
        }
    }

    pub fn with_owner(self, owner: Option<Bytes32>) -> Self {
        Self {
            current_owner: owner,
            ..self
        }
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash
    where
        M: ToTreeHash,
    {
        CurriedProgram {
            program: TreeHash::new(NFT_STATE_LAYER_HASH),
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_HASH.into(),
                metadata: self.metadata.tree_hash(),
                metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
                inner_puzzle: NftOwnershipLayerArgs::curry_tree_hash(
                    self.current_owner,
                    RoyaltyTransferLayer::new(
                        self.launcher_id,
                        self.royalty_puzzle_hash,
                        self.royalty_ten_thousandths,
                    )
                    .tree_hash(),
                    self.p2_puzzle_hash.into(),
                ),
            },
        }
        .tree_hash()
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzle_types::nft::NftMetadata;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;

    use crate::{DidOwner, IntermediateLauncher, Launcher, NftMint, SpendContext, StandardLayer};

    use super::*;

    #[test]
    fn test_parse_nft_info() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(2);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_did, did) =
            Launcher::new(alice.coin.coin_id(), 1).create_simple_did(ctx, &alice_p2)?;
        alice_p2.spend(ctx, alice.coin, create_did)?;

        let mut metadata = NftMetadata::default();
        metadata.data_uris.push("example.com".to_string());

        let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(
                ctx,
                NftMint::new(
                    metadata,
                    alice.puzzle_hash,
                    300,
                    Some(DidOwner::from_did_info(&did.info)),
                ),
            )?;

        let _did = did.update(ctx, &alice_p2, mint_nft)?;
        let original_nft = nft.clone();
        let _nft = nft.transfer(ctx, &alice_p2, alice.puzzle_hash, Conditions::new())?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let puzzle_reveal = sim
            .puzzle_reveal(original_nft.coin.coin_id())
            .expect("missing nft puzzle");

        let mut allocator = Allocator::new();
        let ptr = puzzle_reveal.to_clvm(&mut allocator)?;
        let puzzle = Puzzle::parse(&allocator, ptr);
        let (nft_info, p2_puzzle) =
            NftInfo::<NftMetadata>::parse(&allocator, puzzle)?.expect("not an nft");

        assert_eq!(nft_info, original_nft.info);
        assert_eq!(p2_puzzle.curried_puzzle_hash(), alice.puzzle_hash.into());

        Ok(())
    }
}
