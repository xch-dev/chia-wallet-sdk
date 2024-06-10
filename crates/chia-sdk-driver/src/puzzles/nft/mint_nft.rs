use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::{
    nft::{NftOwnershipLayerArgs, NftRoyaltyTransferPuzzleArgs, NftStateLayerArgs},
    standard::StandardArgs,
    EveProof, Proof,
};
use chia_sdk_types::{conditions::NewNftOwner, puzzles::NftInfo};
use clvm_traits::ToClvm;
use clvmr::NodePtr;

use crate::{Conditions, Launcher, SpendContext, SpendError};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StandardMint<M> {
    pub metadata: M,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_percentage: u16,
    pub synthetic_key: PublicKey,
    pub owner_puzzle_hash: Bytes32,
    pub owner_did: Option<OwnerDid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OwnerDid {
    pub did_id: Bytes32,
    pub did_inner_puzzle_hash: Bytes32,
}

pub trait MintNft {
    fn mint_eve_nft<M>(
        self,
        ctx: &mut SpendContext<'_>,
        inner_puzzle_hash: Bytes32,
        metadata: M,
        royalty_puzzle_hash: Bytes32,
        royalty_percentage: u16,
    ) -> Result<(Conditions, NftInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>;

    fn mint_standard_nft<M>(
        self,
        ctx: &mut SpendContext<'_>,
        mint: StandardMint<M>,
    ) -> Result<(Conditions, NftInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr> + Clone,
        Self: Sized,
    {
        let inner_puzzle_hash = StandardArgs::curry_tree_hash(mint.synthetic_key).into();

        let (mint_nft, nft_info) = self.mint_eve_nft(
            ctx,
            inner_puzzle_hash,
            mint.metadata,
            mint.royalty_puzzle_hash,
            mint.royalty_percentage,
        )?;

        let (spend_eve, nft_info) = ctx.spend_standard_nft(
            &nft_info,
            mint.synthetic_key,
            mint.owner_puzzle_hash,
            mint.owner_did.map(|owner_did| {
                NewNftOwner::new(
                    Some(owner_did.did_id),
                    Vec::new(),
                    Some(owner_did.did_inner_puzzle_hash),
                )
            }),
            Conditions::new(),
        )?;

        Ok((mint_nft.extend(spend_eve), nft_info))
    }
}

impl MintNft for Launcher {
    fn mint_eve_nft<M>(
        self,
        ctx: &mut SpendContext<'_>,
        p2_puzzle_hash: Bytes32,
        metadata: M,
        royalty_puzzle_hash: Bytes32,
        royalty_percentage: u16,
    ) -> Result<(Conditions, NftInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>,
    {
        let metadata_ptr = ctx.alloc(&metadata)?;
        let metadata_hash = ctx.tree_hash(metadata_ptr);

        let transfer_program = NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
            self.coin().coin_id(),
            royalty_puzzle_hash,
            royalty_percentage,
        );

        let ownership_layer =
            NftOwnershipLayerArgs::curry_tree_hash(None, transfer_program, p2_puzzle_hash.into());

        let nft_inner_puzzle_hash =
            NftStateLayerArgs::curry_tree_hash(metadata_hash, ownership_layer).into();

        let launcher_coin = self.coin();
        let (mut chained_spend, eve_coin) = self.spend(ctx, nft_inner_puzzle_hash, ())?;

        chained_spend =
            chained_spend.create_puzzle_announcement(launcher_coin.coin_id().to_vec().into());

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
            current_owner: None,
            royalty_puzzle_hash,
            royalty_percentage,
        };

        Ok((chained_spend, nft_info))
    }
}

#[cfg(test)]
pub use tests::nft_mint;

#[cfg(test)]
mod tests {
    use crate::{
        puzzles::{IntermediateLauncher, Launcher},
        CreateDid,
    };

    use super::*;

    use chia_consensus::gen::{
        conditions::EmptyVisitor, run_block_generator::run_block_generator,
        solution_generator::solution_generator,
    };
    use chia_protocol::Coin;
    use chia_puzzles::nft::NftMetadata;
    use chia_sdk_test::{secret_key, test_transaction, Simulator};
    use clvmr::Allocator;

    pub fn nft_mint(
        synthetic_key: PublicKey,
        owner_did: Option<OwnerDid>,
    ) -> StandardMint<NftMetadata> {
        StandardMint {
            metadata: NftMetadata {
                edition_number: 1,
                edition_total: 1,
                data_uris: vec!["https://example.com/data".to_string()],
                data_hash: Some(Bytes32::new([1; 32])),
                metadata_uris: vec!["https://example.com/metadata".to_string()],
                metadata_hash: Some(Bytes32::new([2; 32])),
                license_uris: vec!["https://example.com/license".to_string()],
                license_hash: Some(Bytes32::new([3; 32])),
            },
            royalty_puzzle_hash: Bytes32::new([4; 32]),
            royalty_percentage: 300,
            owner_puzzle_hash: StandardArgs::curry_tree_hash(synthetic_key).into(),
            owner_did,
            synthetic_key,
        }
    }

    #[test]
    fn test_nft_mint_cost() -> anyhow::Result<()> {
        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = Coin::new(Bytes32::new([0; 32]), puzzle_hash, 1);

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (create_did, did_info) =
            Launcher::new(coin.coin_id(), 1).create_standard_did(ctx, pk)?;
        ctx.spend_p2_coin(coin, pk, create_did)?;

        // We don't want to count the DID creation.
        ctx.take_spends();

        let coin = Coin::new(Bytes32::new([1; 32]), puzzle_hash, 1);
        let (mint_nft, _nft_info) = IntermediateLauncher::new(did_info.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_standard_nft(ctx, nft_mint(pk, None))?;
        let _did_info = ctx.spend_standard_did(
            &did_info,
            pk,
            mint_nft.create_coin_announcement(b"$".to_vec().into()),
        )?;
        ctx.spend_p2_coin(
            coin,
            pk,
            Conditions::new().assert_coin_announcement(did_info.coin.coin_id(), "$"),
        )?;

        let coin_spends = ctx.take_spends();

        let generator = solution_generator(
            coin_spends
                .iter()
                .map(|cs| (cs.coin, cs.puzzle_reveal.clone(), cs.solution.clone())),
        )?;
        let conds = run_block_generator::<Vec<u8>, EmptyVisitor>(
            &mut allocator,
            &generator,
            &[],
            11_000_000_000,
            0,
        )?;

        assert_eq!(conds.cost, 127_780_546);

        Ok(())
    }

    #[tokio::test]
    async fn test_bulk_mint() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 3).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (create_did, did_info) =
            Launcher::new(coin.coin_id(), 1).create_standard_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let mint = StandardMint {
            metadata: (),
            royalty_puzzle_hash: puzzle_hash,
            royalty_percentage: 100,
            owner_puzzle_hash: puzzle_hash,
            synthetic_key: pk,
            owner_did: Some(OwnerDid {
                did_id: did_info.launcher_id,
                did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
            }),
        };

        let mint_1 = IntermediateLauncher::new(did_info.coin.coin_id(), 0, 2)
            .create(ctx)?
            .mint_standard_nft(ctx, mint.clone())?
            .0;

        let mint_2 = IntermediateLauncher::new(did_info.coin.coin_id(), 1, 2)
            .create(ctx)?
            .mint_standard_nft(ctx, mint.clone())?
            .0;

        let _did_info = ctx.spend_standard_did(
            &did_info,
            pk,
            Conditions::new().extend(mint_1).extend(mint_2),
        )?;

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_nonstandard_intermediate_mint() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 3).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (create_did, did_info) =
            Launcher::new(coin.coin_id(), 1).create_standard_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let intermediate_coin = Coin::new(did_info.coin.coin_id(), puzzle_hash, 0);

        let (create_launcher, launcher) = Launcher::create_early(intermediate_coin.coin_id(), 1);

        let mint = StandardMint {
            metadata: (),
            royalty_puzzle_hash: puzzle_hash,
            royalty_percentage: 100,
            owner_puzzle_hash: puzzle_hash,
            synthetic_key: pk,
            owner_did: Some(OwnerDid {
                did_id: did_info.launcher_id,
                did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
            }),
        };

        let (mint_nft, _nft_info) = launcher.mint_standard_nft(ctx, mint)?;
        let _did_info =
            ctx.spend_standard_did(&did_info, pk, mint_nft.create_coin(puzzle_hash, 0))?;
        ctx.spend_p2_coin(intermediate_coin, pk, create_launcher)?;

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn test_nonstandard_intermediate_mint_recreated_did() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 3).await;

        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let (create_did, did_info) =
            Launcher::new(coin.coin_id(), 1).create_standard_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let intermediate_coin = Coin::new(did_info.coin.coin_id(), puzzle_hash, 0);

        let (create_launcher, launcher) = Launcher::create_early(intermediate_coin.coin_id(), 1);

        let mint = StandardMint {
            metadata: (),
            royalty_puzzle_hash: puzzle_hash,
            royalty_percentage: 100,
            owner_puzzle_hash: puzzle_hash,
            synthetic_key: pk,
            owner_did: Some(OwnerDid {
                did_id: did_info.launcher_id,
                did_inner_puzzle_hash: did_info.did_inner_puzzle_hash,
            }),
        };

        let (mint_nft, _nft_info) = launcher.mint_standard_nft(ctx, mint)?;

        let did_info =
            ctx.spend_standard_did(&did_info, pk, Conditions::new().create_coin(puzzle_hash, 0))?;
        let _did_info = ctx.spend_standard_did(&did_info, pk, mint_nft)?;
        ctx.spend_p2_coin(intermediate_coin, pk, create_launcher)?;

        test_transaction(
            &peer,
            ctx.take_spends(),
            &[sk],
            sim.config().genesis_challenge,
        )
        .await;

        Ok(())
    }
}
