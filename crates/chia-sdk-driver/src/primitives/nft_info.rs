use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NftStateLayerArgs,
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
    },
    singleton::SingletonStruct,
};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};

#[derive(Debug, Clone, Copy)]
pub struct NftInfo<M> {
    pub singleton_struct: SingletonStruct,
    pub metadata: M,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Bytes32,
}

impl<M> NftInfo<M> {
    pub fn new(
        singleton_struct: SingletonStruct,
        metadata: M,
        current_owner: Option<Bytes32>,
        royalty_puzzle_hash: Bytes32,
        royalty_ten_thousandths: u16,
        p2_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            singleton_struct,
            metadata,
            current_owner,
            royalty_puzzle_hash,
            royalty_ten_thousandths,
            p2_puzzle_hash,
        }
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash
    where
        M: ToTreeHash,
    {
        NftStateLayerArgs::curry_tree_hash(
            self.metadata.tree_hash(),
            NftOwnershipLayerArgs::curry_tree_hash(
                self.current_owner,
                CurriedProgram {
                    program: NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
                    args: NftRoyaltyTransferPuzzleArgs {
                        singleton_struct: self.singleton_struct,
                        royalty_puzzle_hash: self.royalty_puzzle_hash,
                        royalty_ten_thousandths: self.royalty_ten_thousandths,
                    },
                }
                .tree_hash(),
                self.p2_puzzle_hash.into(),
            ),
        )
    }
}
