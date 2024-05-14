use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{
        NFT_METADATA_UPDATER_PUZZLE_HASH, NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH, NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::{SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
    standard::{StandardArgs, STANDARD_PUZZLE_HASH},
    EveProof, Proof,
};
use clvm_traits::ToClvm;
use clvm_utils::{curry_tree_hash, tree_hash_atom, tree_hash_pair, CurriedProgram, ToTreeHash};
use clvmr::NodePtr;

use crate::{
    u16_to_bytes, ChainedSpend, CreatePuzzleAnnouncement, NftInfo, SpendContext, SpendError,
    SpendableLauncher, StandardNftSpend,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StandardMint<M> {
    pub metadata: M,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
    pub synthetic_key: PublicKey,
    pub owner_puzzle_hash: Bytes32,
    pub did_id: Bytes32,
    pub did_inner_puzzle_hash: Bytes32,
}

pub trait MintNft {
    fn mint_eve_nft<M>(
        self,
        ctx: &mut SpendContext,
        inner_puzzle_hash: Bytes32,
        metadata: M,
        royalty_puzzle_hash: Bytes32,
        royalty_percentage: u16,
    ) -> Result<(ChainedSpend, NftInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>;

    fn mint_standard_nft<M>(
        self,
        ctx: &mut SpendContext,
        mint: StandardMint<M>,
    ) -> Result<(ChainedSpend, NftInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>,
        Self: Sized,
    {
        let inner_puzzle_hash = CurriedProgram {
            program: STANDARD_PUZZLE_HASH,
            args: StandardArgs {
                synthetic_key: mint.synthetic_key,
            },
        }
        .tree_hash()
        .into();

        let (mut mint_nft, nft_info) = self.mint_eve_nft(
            ctx,
            inner_puzzle_hash,
            mint.metadata,
            mint.royalty_puzzle_hash,
            mint.royalty_percentage,
        )?;

        let (nft_spend, nft_info) = StandardNftSpend::new()
            .new_owner(mint.did_id, mint.did_inner_puzzle_hash)
            .transfer(mint.owner_puzzle_hash)
            .finish(ctx, mint.synthetic_key, nft_info)?;

        mint_nft.extend(nft_spend);

        Ok((mint_nft, nft_info))
    }
}

impl MintNft for SpendableLauncher {
    fn mint_eve_nft<M>(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
        metadata: M,
        royalty_puzzle_hash: Bytes32,
        royalty_percentage: u16,
    ) -> Result<(ChainedSpend, NftInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>,
    {
        let metadata_ptr = ctx.alloc(&metadata)?;
        let metadata_hash = ctx.tree_hash(metadata_ptr).into();

        let ownership_layer_hash = nft_ownership_layer_hash(
            None,
            nft_royalty_transfer_hash(
                self.coin().coin_id(),
                royalty_puzzle_hash,
                royalty_percentage,
            ),
            p2_puzzle_hash,
        );

        let nft_inner_puzzle_hash = nft_state_layer_hash(
            metadata_hash,
            NFT_METADATA_UPDATER_PUZZLE_HASH.into(),
            ownership_layer_hash,
        );

        let launcher_coin = self.coin();
        let (mut chained_spend, eve_coin) = self.spend(ctx, nft_inner_puzzle_hash, ())?;

        chained_spend
            .parent_conditions
            .push(ctx.alloc(CreatePuzzleAnnouncement {
                message: launcher_coin.coin_id().to_vec().into(),
            })?);

        let proof = Proof::Eve(EveProof {
            parent_coin_info: launcher_coin.parent_coin_info,
            amount: launcher_coin.amount,
        });

        let nft_info = NftInfo {
            launcher_id: launcher_coin.coin_id(),
            coin: eve_coin,
            nft_inner_puzzle_hash,
            p2_puzzle_hash,
            proof,
            metadata,
            metadata_updater_hash: NFT_METADATA_UPDATER_PUZZLE_HASH.into(),
            current_owner: None,
            royalty_puzzle_hash,
            royalty_percentage,
        };

        Ok((chained_spend, nft_info))
    }
}

pub fn nft_state_layer_hash(
    metadata_hash: Bytes32,
    metadata_updater_hash: Bytes32,
    inner_puzzle_hash: Bytes32,
) -> Bytes32 {
    let mod_hash = tree_hash_atom(&NFT_STATE_LAYER_PUZZLE_HASH);
    let metadata_updater_hash = tree_hash_atom(&metadata_updater_hash);

    curry_tree_hash(
        NFT_STATE_LAYER_PUZZLE_HASH,
        &[
            mod_hash,
            metadata_hash.into(),
            metadata_updater_hash,
            inner_puzzle_hash.into(),
        ],
    )
    .into()
}

pub fn nft_ownership_layer_hash(
    current_owner: Option<Bytes32>,
    transfer_program_hash: Bytes32,
    inner_puzzle_hash: Bytes32,
) -> Bytes32 {
    let mod_hash = tree_hash_atom(&NFT_OWNERSHIP_LAYER_PUZZLE_HASH);
    let current_owner_hash = match current_owner {
        Some(did_id) => tree_hash_atom(&did_id),
        None => tree_hash_atom(&[]),
    };

    curry_tree_hash(
        NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
        &[
            mod_hash,
            current_owner_hash,
            transfer_program_hash.into(),
            inner_puzzle_hash.into(),
        ],
    )
    .into()
}

pub fn nft_royalty_transfer_hash(
    launcher_id: Bytes32,
    royalty_puzzle_hash: Bytes32,
    royalty_percentage: u16,
) -> Bytes32 {
    let royalty_puzzle_hash = tree_hash_atom(&royalty_puzzle_hash);
    let royalty_percentage_hash = tree_hash_atom(&u16_to_bytes(royalty_percentage));

    let singleton_hash = tree_hash_atom(&SINGLETON_TOP_LAYER_PUZZLE_HASH);
    let launcher_id_hash = tree_hash_atom(&launcher_id);
    let launcher_puzzle_hash = tree_hash_atom(&SINGLETON_LAUNCHER_PUZZLE_HASH);

    let pair = tree_hash_pair(launcher_id_hash, launcher_puzzle_hash);
    let singleton_struct_hash = tree_hash_pair(singleton_hash, pair);

    curry_tree_hash(
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
        &[
            singleton_struct_hash,
            royalty_puzzle_hash,
            royalty_percentage_hash,
        ],
    )
    .into()
}

#[cfg(test)]
mod tests {
    use chia_bls::{sign, Signature};
    use chia_protocol::SpendBundle;
    use chia_puzzles::{
        nft::{
            NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NftStateLayerArgs,
            NFT_METADATA_UPDATER_PUZZLE_HASH, NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
            NFT_STATE_LAYER_PUZZLE_HASH,
        },
        singleton::{
            SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH,
        },
        DeriveSynthetic,
    };
    use clvm_utils::CurriedProgram;
    use clvmr::Allocator;

    use crate::{
        testing::SECRET_KEY, Chainable, CreateDid, IntermediateLauncher, Launcher,
        RequiredSignature, StandardDidSpend, StandardSpend, WalletSimulator,
    };

    use super::*;

    #[tokio::test]
    async fn test_bulk_mint() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let sk = SECRET_KEY.derive_synthetic();
        let pk = sk.public_key();

        let puzzle_hash = CurriedProgram {
            program: STANDARD_PUZZLE_HASH,
            args: StandardArgs { synthetic_key: pk },
        }
        .tree_hash()
        .into();

        let parent = sim.generate_coin(puzzle_hash, 3).await.coin;

        let (create_did, did_info) = Launcher::new(parent.coin_id(), 1)
            .create(&mut ctx)?
            .create_standard_did(&mut ctx, pk)?;

        StandardSpend::new()
            .chain(create_did)
            .finish(&mut ctx, parent, pk)?;

        let mint = StandardMint {
            metadata: (),
            royalty_puzzle_hash: puzzle_hash,
            royalty_percentage: 100,
            owner_puzzle_hash: puzzle_hash,
            synthetic_key: pk,
            did_id: did_info.launcher_id,
            did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
        };

        let _did_info = StandardDidSpend::new()
            .chain(
                IntermediateLauncher::new(did_info.coin.coin_id(), 0, 2)
                    .create(&mut ctx)?
                    .mint_standard_nft(&mut ctx, mint.clone())?
                    .0,
            )
            .chain(
                IntermediateLauncher::new(did_info.coin.coin_id(), 1, 2)
                    .create(&mut ctx)?
                    .mint_standard_nft(&mut ctx, mint.clone())?
                    .0,
            )
            .recreate()
            .finish(&mut ctx, pk, did_info)?;

        let coin_spends = ctx.take_spends();

        let required_signatures = RequiredSignature::from_coin_spends(
            &mut allocator,
            &coin_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )?;

        let mut aggregated_signature = Signature::default();

        for required in required_signatures {
            aggregated_signature += &sign(&sk, required.final_message());
        }

        let spend_bundle = SpendBundle::new(coin_spends, aggregated_signature);
        let ack = peer.send_transaction(spend_bundle).await?;

        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        Ok(())
    }

    #[test]
    fn test_state_layer_hash() {
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let inner_puzzle = ctx.alloc([1, 2, 3]).unwrap();
        let inner_puzzle_hash = ctx.tree_hash(inner_puzzle).into();

        let metadata = ctx.alloc([4, 5, 6]).unwrap();
        let metadata_hash = ctx.tree_hash(metadata).into();

        let nft_state_layer = ctx.nft_state_layer();

        let puzzle = ctx
            .alloc(CurriedProgram {
                program: nft_state_layer,
                args: NftStateLayerArgs {
                    mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                    metadata,
                    metadata_updater_puzzle_hash: NFT_METADATA_UPDATER_PUZZLE_HASH.into(),
                    inner_puzzle,
                },
            })
            .unwrap();
        let allocated_puzzle_hash = ctx.tree_hash(puzzle);

        let puzzle_hash = nft_state_layer_hash(
            metadata_hash,
            NFT_METADATA_UPDATER_PUZZLE_HASH.into(),
            inner_puzzle_hash,
        );

        assert_eq!(hex::encode(allocated_puzzle_hash), hex::encode(puzzle_hash));
    }

    #[test]
    fn test_ownership_layer_hash() {
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let inner_puzzle = ctx.alloc([1, 2, 3]).unwrap();
        let inner_puzzle_hash = ctx.tree_hash(inner_puzzle).into();

        let launcher_id = Bytes32::new([69; 32]);

        let royalty_puzzle_hash = Bytes32::new([34; 32]);
        let royalty_percentage = 100;

        let current_owner = Some(Bytes32::new([42; 32]));

        let nft_ownership_layer = ctx.nft_ownership_layer();
        let nft_royalty_transfer = ctx.nft_royalty_transfer();

        let transfer_program = ctx
            .alloc(CurriedProgram {
                program: nft_royalty_transfer,
                args: NftRoyaltyTransferPuzzleArgs {
                    singleton_struct: SingletonStruct {
                        mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                        launcher_id,
                        launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    },
                    royalty_puzzle_hash,
                    trade_price_percentage: royalty_percentage,
                },
            })
            .unwrap();
        let allocated_transfer_program_hash = ctx.tree_hash(transfer_program).into();

        let puzzle = ctx
            .alloc(CurriedProgram {
                program: nft_ownership_layer,
                args: NftOwnershipLayerArgs {
                    mod_hash: NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into(),
                    current_owner,
                    transfer_program,
                    inner_puzzle,
                },
            })
            .unwrap();
        let allocated_puzzle_hash = ctx.tree_hash(puzzle);

        let puzzle_hash = nft_ownership_layer_hash(
            current_owner,
            allocated_transfer_program_hash,
            inner_puzzle_hash,
        );

        let transfer_program_hash =
            nft_royalty_transfer_hash(launcher_id, royalty_puzzle_hash, royalty_percentage);

        assert_eq!(
            hex::encode(allocated_transfer_program_hash),
            hex::encode(transfer_program_hash)
        );

        assert_eq!(hex::encode(allocated_puzzle_hash), hex::encode(puzzle_hash));
    }
}
