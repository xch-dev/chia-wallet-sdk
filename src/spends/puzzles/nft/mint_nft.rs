use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{
        NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NftStateLayerArgs,
        NFT_METADATA_UPDATER_PUZZLE_HASH, NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
        NFT_ROYALTY_TRANSFER_PUZZLE_HASH, NFT_STATE_LAYER_PUZZLE_HASH,
    },
    singleton::SingletonStruct,
    standard::{StandardArgs, STANDARD_PUZZLE_HASH},
    EveProof, Proof,
};
use clvm_traits::ToClvm;
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    ChainedSpend, CreatePuzzleAnnouncement, NftInfo, SpendContext, SpendError, SpendableLauncher,
    StandardNftSpend,
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
        let metadata_hash = ctx.tree_hash(metadata_ptr);

        let transfer_program = CurriedProgram {
            program: NFT_ROYALTY_TRANSFER_PUZZLE_HASH,
            args: NftRoyaltyTransferPuzzleArgs {
                singleton_struct: SingletonStruct::new(self.coin().coin_id()),
                royalty_puzzle_hash,
                trade_price_percentage: royalty_percentage,
            },
        };

        let ownership_layer = CurriedProgram {
            program: NFT_OWNERSHIP_LAYER_PUZZLE_HASH,
            args: NftOwnershipLayerArgs {
                mod_hash: NFT_OWNERSHIP_LAYER_PUZZLE_HASH.into(),
                current_owner: None,
                transfer_program,
                inner_puzzle: TreeHash::from(p2_puzzle_hash),
            },
        };

        let nft_inner_puzzle_hash = CurriedProgram {
            program: NFT_STATE_LAYER_PUZZLE_HASH,
            args: NftStateLayerArgs {
                mod_hash: NFT_STATE_LAYER_PUZZLE_HASH.into(),
                metadata: metadata_hash,
                metadata_updater_puzzle_hash: NFT_METADATA_UPDATER_PUZZLE_HASH.into(),
                inner_puzzle: ownership_layer,
            },
        }
        .tree_hash()
        .into();

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

#[cfg(test)]
mod tests {
    use chia_bls::{sign, Signature};
    use chia_protocol::{Coin, SpendBundle};
    use chia_puzzles::DeriveSynthetic;
    use clvm_utils::CurriedProgram;
    use clvmr::Allocator;

    use crate::{
        testing::SECRET_KEY, Chainable, CreateCoinWithoutMemos, CreateDid, IntermediateLauncher,
        Launcher, RequiredSignature, StandardDidSpend, StandardSpend, WalletSimulator,
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

    #[tokio::test]
    async fn test_nonstandard_intermediate_mint() -> anyhow::Result<()> {
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

        let intermediate_coin = Coin::new(did_info.coin.coin_id(), puzzle_hash, 0);

        let (create_launcher, launcher) =
            Launcher::new(intermediate_coin.coin_id(), 1).create_from_intermediate(&mut ctx)?;

        let mint = StandardMint {
            metadata: (),
            royalty_puzzle_hash: puzzle_hash,
            royalty_percentage: 100,
            owner_puzzle_hash: puzzle_hash,
            synthetic_key: pk,
            did_id: did_info.launcher_id,
            did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
        };

        let (mint_nft, _nft_info) = launcher.mint_standard_nft(&mut ctx, mint)?;

        StandardDidSpend::new()
            .chain(mint_nft)
            .condition(ctx.alloc(CreateCoinWithoutMemos {
                puzzle_hash,
                amount: 0,
            })?)
            .recreate()
            .finish(&mut ctx, pk, did_info)?;

        StandardSpend::new()
            .chain(create_launcher)
            .finish(&mut ctx, intermediate_coin, pk)?;

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

    #[tokio::test]
    async fn test_nonstandard_intermediate_mint_recreated_did() -> anyhow::Result<()> {
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

        let intermediate_coin = Coin::new(did_info.coin.coin_id(), puzzle_hash, 0);

        let (create_launcher, launcher) =
            Launcher::new(intermediate_coin.coin_id(), 1).create_from_intermediate(&mut ctx)?;

        let mint = StandardMint {
            metadata: (),
            royalty_puzzle_hash: puzzle_hash,
            royalty_percentage: 100,
            owner_puzzle_hash: puzzle_hash,
            synthetic_key: pk,
            did_id: did_info.launcher_id,
            did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
        };

        let (mint_nft, _nft_info) = launcher.mint_standard_nft(&mut ctx, mint)?;

        let did_info = StandardDidSpend::new()
            .condition(ctx.alloc(CreateCoinWithoutMemos {
                puzzle_hash,
                amount: 0,
            })?)
            .recreate()
            .finish(&mut ctx, pk, did_info)?;

        StandardDidSpend::new()
            .chain(mint_nft)
            .recreate()
            .finish(&mut ctx, pk, did_info)?;

        StandardSpend::new()
            .chain(create_launcher)
            .finish(&mut ctx, intermediate_coin, pk)?;

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
}
