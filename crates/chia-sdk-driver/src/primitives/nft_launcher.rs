use chia_protocol::Bytes32;
use chia_puzzles::{singleton::SingletonStruct, EveProof, Proof};
use chia_sdk_types::{Condition, NewNftOwner};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{did_puzzle_assertion, Conditions, DriverError, Launcher, Spend, SpendContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NftMint<M> {
    pub metadata: M,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Bytes32,
    pub owner: NewNftOwner,
}

impl Launcher {
    pub fn mint_eve_nft<M>(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
        metadata: M,
        royalty_puzzle_hash: Bytes32,
        royalty_ten_thousandths: u16,
    ) -> Result<(Conditions, Nft<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone + ToTreeHash,
    {
        let launcher_coin = self.coin();
        let nft_info = NftInfo::new(
            SingletonStruct::new(launcher_coin.coin_id()),
            metadata,
            None,
            royalty_puzzle_hash,
            royalty_ten_thousandths,
            p2_puzzle_hash,
        );

        let (launch_singleton, eve_coin) = self
            .spend(ctx, nft_info.inner_puzzle_hash().into(), ())
            .map_err(DriverError::Spend)?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((
            launch_singleton.create_puzzle_announcement(launcher_coin.coin_id().to_vec().into()),
            Nft::new(eve_coin, proof, nft_info),
        ))
    }

    pub fn mint_nft<M>(
        self,
        ctx: &mut SpendContext,
        mint: NftMint<M>,
    ) -> Result<(Conditions, Nft<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone + ToTreeHash,
    {
        let mut conditions =
            Conditions::new().create_hinted_coin(mint.p2_puzzle_hash, 1, mint.p2_puzzle_hash);

        if mint.owner != NewNftOwner::default() {
            conditions = conditions.condition(Condition::Other(ctx.alloc(&mint.owner)?));
        }

        let inner_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        let p2_puzzle_hash = ctx.tree_hash(inner_puzzle).into();
        let inner_spend = Spend::new(inner_puzzle, NodePtr::NIL);

        let (mint_eve_nft, eve_nft) = self.mint_eve_nft(
            ctx,
            p2_puzzle_hash,
            mint.metadata,
            mint.royalty_puzzle_hash,
            mint.royalty_ten_thousandths,
        )?;

        let eve_spend = eve_nft.spend(ctx, inner_spend)?;
        ctx.insert_coin_spend(eve_spend.clone());

        let mut did_conditions = Conditions::new();

        if mint.owner != NewNftOwner::default() {
            did_conditions = did_conditions.assert_raw_puzzle_announcement(did_puzzle_assertion(
                eve_nft.coin.puzzle_hash,
                &mint.owner,
            ));
        }

        Ok((
            mint_eve_nft.extend(did_conditions),
            eve_nft.create_child(mint.p2_puzzle_hash, Some(mint.owner.did_id)),
        ))
    }
}

#[cfg(test)]
pub use tests::nft_mint;

use super::{Nft, NftInfo};

#[cfg(test)]
mod tests {
    use crate::{Did, IntermediateLauncher, Launcher};

    use super::*;

    use chia_consensus::gen::{
        conditions::EmptyVisitor, run_block_generator::run_block_generator,
        solution_generator::solution_generator,
    };
    use chia_protocol::Coin;
    use chia_puzzles::{nft::NftMetadata, standard::StandardArgs};
    use chia_sdk_test::{secret_key, test_transaction, Simulator};
    use chia_sdk_types::MAINNET_CONSTANTS;

    pub fn nft_mint(p2_puzzle_hash: Bytes32, did: Option<&Did<()>>) -> NftMint<NftMetadata> {
        NftMint {
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
            royalty_ten_thousandths: 300,
            p2_puzzle_hash,
            owner: NewNftOwner {
                did_id: did.map(|did| did.info.singleton_struct.launcher_id),
                trade_prices: Vec::new(),
                did_inner_puzzle_hash: did.map(|did| did.info.inner_puzzle_hash().into()),
            },
        }
    }

    #[test]
    fn test_nft_mint_cost() -> anyhow::Result<()>
    where
        NftMetadata: ToClvm<Allocator> + FromClvm<Allocator> + Clone + ToTreeHash,
    {
        let sk = secret_key()?;
        let pk = sk.public_key();
        let mut owned_ctx = SpendContext::new();
        let ctx = &mut owned_ctx;

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = Coin::new(Bytes32::new([0; 32]), puzzle_hash, 1);

        let (create_did, did) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;
        ctx.spend_p2_coin(coin, pk, create_did)?;

        // We don't want to count the DID creation.
        ctx.take_spends();

        let coin = Coin::new(Bytes32::new([1; 32]), puzzle_hash, 1);
        let (mint_nft, _nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(ctx, nft_mint(puzzle_hash, None))?;
        let _did = ctx.spend_standard_did(
            &did,
            pk,
            mint_nft.create_coin_announcement(b"$".to_vec().into()),
        )?;
        ctx.spend_p2_coin(
            coin,
            pk,
            Conditions::new().assert_coin_announcement(did.coin.coin_id(), "$"),
        )?;

        let coin_spends = ctx.take_spends();

        let generator = solution_generator(
            coin_spends
                .iter()
                .map(|cs| (cs.coin, cs.puzzle_reveal.clone(), cs.solution.clone())),
        )?;
        let conds = run_block_generator::<Vec<u8>, EmptyVisitor, _>(
            &mut owned_ctx.into(),
            &generator,
            [],
            11_000_000_000,
            0,
            &MAINNET_CONSTANTS,
        )?;

        assert_eq!(conds.cost, 122_646_589);

        Ok(())
    }

    #[tokio::test]
    async fn test_bulk_mint() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 3).await;

        let (create_did, did) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let mint_1 = IntermediateLauncher::new(did.coin.coin_id(), 0, 2)
            .create(ctx)?
            .mint_nft(ctx, nft_mint(puzzle_hash, Some(&did)))?
            .0;

        let mint_2 = IntermediateLauncher::new(did.coin.coin_id(), 1, 2)
            .create(ctx)?
            .mint_nft(ctx, nft_mint(puzzle_hash, Some(&did)))?
            .0;

        let _did =
            ctx.spend_standard_did(&did, pk, Conditions::new().extend(mint_1).extend(mint_2))?;

        test_transaction(&peer, ctx.take_spends(), &[sk], &sim.config().constants).await;

        Ok(())
    }

    #[tokio::test]
    async fn test_nonstandard_intermediate_mint() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 3).await;

        let (create_did, did) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let intermediate_coin = Coin::new(did.coin.coin_id(), puzzle_hash, 0);

        let (create_launcher, launcher) = Launcher::create_early(intermediate_coin.coin_id(), 1);

        let (mint_nft, _nft) = launcher.mint_nft(ctx, nft_mint(puzzle_hash, Some(&did)))?;

        let _did_info = ctx.spend_standard_did(&did, pk, mint_nft.create_coin(puzzle_hash, 0))?;

        ctx.spend_p2_coin(intermediate_coin, pk, create_launcher)?;

        test_transaction(&peer, ctx.take_spends(), &[sk], &sim.config().constants).await;

        Ok(())
    }

    #[tokio::test]
    async fn test_nonstandard_intermediate_mint_recreated_did() -> anyhow::Result<()> {
        let sim = Simulator::new().await?;
        let peer = sim.connect().await?;
        let ctx = &mut SpendContext::new();

        let sk = secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.mint_coin(puzzle_hash, 3).await;

        let (create_did, did) = Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_p2_coin(coin, pk, create_did)?;

        let intermediate_coin = Coin::new(did.coin.coin_id(), puzzle_hash, 0);

        let (create_launcher, launcher) = Launcher::create_early(intermediate_coin.coin_id(), 1);

        let (mint_nft, _nft_info) = launcher.mint_nft(ctx, nft_mint(puzzle_hash, Some(&did)))?;

        let did =
            ctx.spend_standard_did(&did, pk, Conditions::new().create_coin(puzzle_hash, 0))?;
        let _did = ctx.spend_standard_did(&did, pk, mint_nft)?;
        ctx.spend_p2_coin(intermediate_coin, pk, create_launcher)?;

        test_transaction(&peer, ctx.take_spends(), &[sk], &sim.config().constants).await;

        Ok(())
    }
}
