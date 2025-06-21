use chia_protocol::Bytes32;
use chia_puzzle_types::{
    nft::{NftOwnershipLayerArgs, NftStateLayerArgs},
    singleton::SingletonArgs,
};
use chia_puzzles::NFT_STATE_LAYER_HASH;
use chia_sdk_types::{
    conditions::{CreateCoin, NewMetadataOutput},
    run_puzzle, Condition, Mod,
};
use clvm_traits::{clvm_list, FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DriverError, Layer, NftOwnershipLayer, NftStateLayer, Puzzle, RoyaltyTransferLayer,
    SingletonLayer, Spend,
};

pub type StandardNftLayers<M, I> =
    SingletonLayer<NftStateLayer<M, NftOwnershipLayer<RoyaltyTransferLayer, I>>>;

/// Information needed to construct the outer puzzle of an NFT.
/// It does not include the inner puzzle, which must be stored separately.
///
/// This type can be used on its own for parsing, or as part of the [`Nft`](crate::Nft) primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NftInfo<M> {
    /// The coin id of the launcher coin that created this NFT's singleton.
    pub launcher_id: Bytes32,

    /// The metadata stored in the [`NftStateLayer`]. This can only be updated by
    /// going through the [`metadata_updater_puzzle_hash`](NftInfo::metadata_updater_puzzle_hash).
    pub metadata: M,

    /// The puzzle hash of the metadata updater. This is used to update the metadata of the NFT.
    /// This is typically [`NFT_METADATA_UPDATER_DEFAULT_HASH`](chia_puzzles::NFT_METADATA_UPDATER_DEFAULT_HASH),
    /// which ensures the [`NftMetadata`](chia_puzzle_types::nft::NftMetadata) object remains immutable
    /// except for prepending additional URIs.
    ///
    /// A custom metadata updater can be used, however support in existing wallets and display
    /// services may be limited.
    pub metadata_updater_puzzle_hash: Bytes32,

    /// The current assigned owner of the NFT, if any. This is managed by the [`NftOwnershipLayer`].
    ///
    /// Historically this was always a DID, although it's possible to assign any singleton including a vault.
    ///
    /// It's intended to unassign the owner after transferring to an external wallet or creating an offer.
    pub current_owner: Option<Bytes32>,

    /// The puzzle hash to which royalties will be paid out to in offers involving this NFT.
    /// This is required even if the royalty is 0. Currently, all NFTs must use the default [`RoyaltyTransferLayer`],
    /// however this may change in the future.
    pub royalty_puzzle_hash: Bytes32,

    /// The royalty percentage to be paid out to the owner in offers involving this NFT.
    /// This is represented as hundredths of a percent, so 300 is 3%.
    pub royalty_basis_points: u16,

    /// The hash of the inner puzzle to this NFT.
    /// If you encode this puzzle hash as bech32m, it's the same as the current owner's address.
    pub p2_puzzle_hash: Bytes32,
}

impl<M> NftInfo<M> {
    pub fn new(
        launcher_id: Bytes32,
        metadata: M,
        metadata_updater_puzzle_hash: Bytes32,
        current_owner: Option<Bytes32>,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            launcher_id,
            metadata,
            metadata_updater_puzzle_hash,
            current_owner,
            royalty_puzzle_hash,
            royalty_basis_points,
            p2_puzzle_hash,
        }
    }

    pub fn with_metadata<N>(self, metadata: N) -> NftInfo<N> {
        NftInfo {
            launcher_id: self.launcher_id,
            metadata,
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            current_owner: self.current_owner,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_basis_points: self.royalty_basis_points,
            p2_puzzle_hash: self.p2_puzzle_hash,
        }
    }

    /// Parses an [`NftInfo`] from a [`Puzzle`] by extracting the [`NftStateLayer`] and [`NftOwnershipLayer`].
    ///
    /// This will return a tuple of the [`NftInfo`] and its p2 puzzle.
    ///
    /// If the puzzle is not an NFT, this will return [`None`] instead of an error.
    /// However, if the puzzle should have been an NFT but had a parsing error, this will return an error.
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
            royalty_basis_points: layers
                .inner_puzzle
                .inner_puzzle
                .transfer_layer
                .royalty_basis_points,
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
                        self.royalty_basis_points,
                    ),
                    p2_puzzle,
                ),
            ),
        )
    }

    /// Calculates the inner puzzle hash of the NFT singleton.
    ///
    /// This includes both the [`NftStateLayer`] and [`NftOwnershipLayer`], but not the [`SingletonLayer`].
    pub fn inner_puzzle_hash(&self) -> TreeHash
    where
        M: ToTreeHash,
    {
        NftStateLayerArgs {
            mod_hash: NFT_STATE_LAYER_HASH.into(),
            metadata: self.metadata.tree_hash(),
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            inner_puzzle: NftOwnershipLayerArgs::curry_tree_hash(
                self.current_owner,
                RoyaltyTransferLayer::new(
                    self.launcher_id,
                    self.royalty_puzzle_hash,
                    self.royalty_basis_points,
                )
                .tree_hash(),
                self.p2_puzzle_hash.into(),
            ),
        }
        .curry_tree_hash()
    }

    /// Calculates the full puzzle hash of the NFT, which is the hash of the outer [`SingletonLayer`].
    pub fn puzzle_hash(&self) -> TreeHash
    where
        M: ToTreeHash,
    {
        SingletonArgs::new(self.launcher_id, self.inner_puzzle_hash()).curry_tree_hash()
    }

    /// Parses the child of an [`NftInfo`] from the p2 spend.
    ///
    /// This will automatically run the transfer program or metadata updater, if
    /// they are revealed in the p2 spend's output conditions. This way the returned
    /// [`NftInfo`] will have the correct owner (if present) and metadata.
    pub fn child_from_p2_spend(
        &self,
        allocator: &mut Allocator,
        spend: Spend,
    ) -> Result<(Self, CreateCoin<NodePtr>), DriverError>
    where
        M: Clone + ToClvm<Allocator> + FromClvm<Allocator>,
    {
        let output = run_puzzle(allocator, spend.puzzle, spend.solution)?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let mut create_coin = None;
        let mut new_owner = None;
        let mut new_metadata = None;

        for condition in conditions {
            match condition {
                Condition::CreateCoin(condition) if condition.amount % 2 == 1 => {
                    create_coin = Some(condition);
                }
                Condition::TransferNft(condition) => {
                    new_owner = Some(condition);
                }
                Condition::UpdateNftMetadata(condition) => {
                    new_metadata = Some(condition);
                }
                _ => {}
            }
        }

        let Some(create_coin) = create_coin else {
            return Err(DriverError::MissingChild);
        };

        let mut info = self.clone();

        if let Some(new_owner) = new_owner {
            info.current_owner = new_owner.launcher_id;
        }

        if let Some(new_metadata) = new_metadata {
            let metadata_updater_solution = clvm_list!(
                &self.metadata,
                self.metadata_updater_puzzle_hash,
                new_metadata.updater_solution
            )
            .to_clvm(allocator)?;

            let output = run_puzzle(
                allocator,
                new_metadata.updater_puzzle_reveal,
                metadata_updater_solution,
            )?;

            let output =
                NewMetadataOutput::<M, NodePtr>::from_clvm(allocator, output)?.metadata_info;
            info.metadata = output.new_metadata;
            info.metadata_updater_puzzle_hash = output.new_updater_puzzle_hash;
        }

        info.p2_puzzle_hash = create_coin.puzzle_hash;

        Ok((info, create_coin))
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzle_types::nft::NftMetadata;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::{conditions::TransferNft, Conditions};

    use crate::{IntermediateLauncher, Launcher, NftMint, SpendContext, StandardLayer};

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
                    Some(TransferNft::new(
                        Some(did.info.launcher_id),
                        Vec::new(),
                        Some(did.info.inner_puzzle_hash().into()),
                    )),
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
