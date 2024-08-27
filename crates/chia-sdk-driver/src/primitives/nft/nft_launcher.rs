use chia_protocol::Bytes32;
use chia_puzzles::{EveProof, Proof};
use chia_sdk_types::{Conditions, TransferNft};
use clvm_traits::{clvm_quote, FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{did_puzzle_assertion, DriverError, Launcher, Spend, SpendContext};

use super::{Nft, NftInfo, NftMint};

impl Launcher {
    pub fn mint_eve_nft<M>(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
        metadata: M,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_ten_thousandths: u16,
    ) -> Result<(Conditions, Nft<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let launcher_coin = self.coin();
        let metadata_ptr = ctx.alloc(&metadata)?;

        let nft_info = NftInfo::new(
            launcher_coin.coin_id(),
            ctx.tree_hash(metadata_ptr),
            metadata_updater_puzzle_hash,
            None,
            royalty_puzzle_hash,
            royalty_ten_thousandths,
            p2_puzzle_hash,
        );

        let (launch_singleton, eve_coin) =
            self.spend(ctx, nft_info.inner_puzzle_hash().into(), ())?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((
            launch_singleton.create_puzzle_announcement(launcher_coin.coin_id().to_vec().into()),
            Nft::new(eve_coin, proof, nft_info.with_metadata(metadata)),
        ))
    }

    pub fn mint_nft<M>(
        self,
        ctx: &mut SpendContext,
        mint: NftMint<M>,
    ) -> Result<(Conditions, Nft<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
    {
        let transfer_condition = mint.owner.map(|owner| {
            TransferNft::new(
                Some(owner.did_id),
                Vec::new(),
                Some(owner.inner_puzzle_hash),
            )
        });

        let conditions = Conditions::new()
            .create_coin(mint.p2_puzzle_hash, 1, vec![mint.p2_puzzle_hash.into()])
            .extend(transfer_condition.clone());

        let inner_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        let p2_puzzle_hash = ctx.tree_hash(inner_puzzle).into();
        let inner_spend = Spend::new(inner_puzzle, NodePtr::NIL);

        let (mint_eve_nft, eve_nft) = self.mint_eve_nft(
            ctx,
            p2_puzzle_hash,
            mint.metadata,
            mint.metadata_updater_puzzle_hash,
            mint.royalty_puzzle_hash,
            mint.royalty_ten_thousandths,
        )?;

        let eve_spend = eve_nft.spend(ctx, inner_spend)?;
        ctx.insert(eve_spend.clone());

        let mut did_conditions = Conditions::new();

        if let Some(transfer_condition) = transfer_condition {
            did_conditions = did_conditions.assert_puzzle_announcement(did_puzzle_assertion(
                eve_nft.coin.puzzle_hash,
                &transfer_condition,
            ));
        }

        let hashed_eve = eve_nft.with_hashed_metadata(&mut ctx.allocator)?;
        let child = hashed_eve.create_child(
            mint.p2_puzzle_hash,
            Some(mint.owner.map(|owner| owner.did_id)),
        );

        Ok((
            mint_eve_nft.extend(did_conditions),
            child.with_metadata(eve_nft.info.metadata),
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{DidOwner, IntermediateLauncher, Launcher, StandardLayer};

    use super::*;

    use chia_consensus::gen::{
        conditions::EmptyVisitor, run_block_generator::run_block_generator,
        solution_generator::solution_generator,
    };
    use chia_protocol::Coin;
    use chia_puzzles::{nft::NftMetadata, standard::StandardArgs};
    use chia_sdk_test::{test_secret_key, Simulator};
    use chia_sdk_types::{announcement_id, TESTNET11_CONSTANTS};

    #[test]
    fn test_nft_mint_cost() -> anyhow::Result<()> {
        let sk = test_secret_key()?;
        let pk = sk.public_key();
        let mut owned_ctx = SpendContext::new();
        let ctx = &mut owned_ctx;

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = Coin::new(Bytes32::new([0; 32]), puzzle_hash, 1);

        let (create_did, did) =
            Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, &StandardLayer::new(pk))?;
        ctx.spend_standard_coin(coin, pk, create_did)?;

        // We don't want to count the DID creation.
        ctx.take();

        let coin = Coin::new(Bytes32::new([1; 32]), puzzle_hash, 1);
        let (mint_nft, _nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
            .create(ctx)?
            .mint_nft(
                ctx,
                NftMint::new(NftMetadata::default(), puzzle_hash, 300, None),
            )?;

        let _ = did.update_with(
            ctx,
            &StandardLayer::new(pk),
            mint_nft.create_coin_announcement(b"$".to_vec().into()),
        )?;

        ctx.spend_standard_coin(
            coin,
            pk,
            Conditions::new().assert_coin_announcement(announcement_id(did.coin.coin_id(), "$")),
        )?;

        let coin_spends = ctx.take();

        let generator = solution_generator(
            coin_spends
                .iter()
                .map(|cs| (cs.coin, cs.puzzle_reveal.clone(), cs.solution.clone())),
        )?;
        let conds = run_block_generator::<Vec<u8>, EmptyVisitor, _>(
            &mut owned_ctx.allocator,
            &generator,
            [],
            11_000_000_000,
            0,
            &TESTNET11_CONSTANTS,
        )?;

        assert_eq!(conds.cost, 119_997_509);

        Ok(())
    }

    #[test]
    fn test_bulk_mint() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let sk = test_secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.new_coin(puzzle_hash, 3);

        let (create_did, did) =
            Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, &StandardLayer::new(pk))?;

        ctx.spend_standard_coin(coin, pk, create_did)?;

        let mint = NftMint::new(
            NftMetadata::default(),
            puzzle_hash,
            300,
            Some(DidOwner::from_did_info(&did.info)),
        );

        let mint_1 = IntermediateLauncher::new(did.coin.coin_id(), 0, 2)
            .create(ctx)?
            .mint_nft(ctx, mint.clone())?
            .0;

        let mint_2 = IntermediateLauncher::new(did.coin.coin_id(), 1, 2)
            .create(ctx)?
            .mint_nft(ctx, mint)?
            .0;

        let _ = did.update_with(ctx, &StandardLayer::new(pk), mint_1.extend(mint_2))?;

        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }

    #[test]
    fn test_nonstandard_intermediate_mint() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let sk = test_secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.new_coin(puzzle_hash, 3);

        let (create_did, did) =
            Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, &StandardLayer::new(pk))?;

        ctx.spend_standard_coin(coin, pk, create_did)?;

        let intermediate_coin = Coin::new(did.coin.coin_id(), puzzle_hash, 0);

        let (create_launcher, launcher) = Launcher::create_early(intermediate_coin.coin_id(), 1);

        let mint = NftMint::new(
            NftMetadata::default(),
            puzzle_hash,
            300,
            Some(DidOwner::from_did_info(&did.info)),
        );

        let (mint_nft, _nft) = launcher.mint_nft(ctx, mint)?;

        let _ = did.update_with(
            ctx,
            &StandardLayer::new(pk),
            mint_nft.create_coin(puzzle_hash, 0, Vec::new()),
        )?;

        ctx.spend_standard_coin(intermediate_coin, pk, create_launcher)?;

        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }

    #[test]
    fn test_nonstandard_intermediate_mint_recreated_did() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let sk = test_secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.new_coin(puzzle_hash, 3);

        let (create_did, did) =
            Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, &StandardLayer::new(pk))?;

        ctx.spend_standard_coin(coin, pk, create_did)?;

        let intermediate_coin = Coin::new(did.coin.coin_id(), puzzle_hash, 0);

        let (create_launcher, launcher) = Launcher::create_early(intermediate_coin.coin_id(), 1);

        let mint = NftMint::new(
            NftMetadata::default(),
            puzzle_hash,
            300,
            Some(DidOwner::from_did_info(&did.info)),
        );

        let (mint_nft, _nft_info) = launcher.mint_nft(ctx, mint)?;

        let did = did.update_with(
            ctx,
            &StandardLayer::new(pk),
            Conditions::new().create_coin(puzzle_hash, 0, Vec::new()),
        )?;

        let _ = did.update_with(ctx, &StandardLayer::new(pk), mint_nft)?;

        ctx.spend_standard_coin(intermediate_coin, pk, create_launcher)?;
        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }
}
