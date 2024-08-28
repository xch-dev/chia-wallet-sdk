use chia_protocol::Bytes32;
use chia_puzzles::{EveProof, Proof};
use chia_sdk_types::Conditions;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash_atom;
use clvmr::Allocator;

use crate::{DriverError, Launcher, SpendContext, SpendWithConditions};

use super::{Did, DidInfo};

impl Launcher {
    pub fn create_eve_did<M>(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
        recovery_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
    ) -> Result<(Conditions, Did<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator>,
    {
        let launcher_coin = self.coin();

        let did_info = DidInfo::new(
            launcher_coin.coin_id(),
            recovery_list_hash,
            num_verifications_required,
            metadata,
            p2_puzzle_hash,
        );

        let inner_puzzle_hash = did_info.inner_puzzle_hash(&mut ctx.allocator)?;
        let (launch_singleton, eve_coin) = self.spend(ctx, inner_puzzle_hash.into(), ())?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((launch_singleton, Did::new(eve_coin, proof, did_info)))
    }

    pub fn create_did<M, I>(
        self,
        ctx: &mut SpendContext,
        recovery_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
        inner: &I,
    ) -> Result<(Conditions, Did<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
        I: SpendWithConditions,
        Self: Sized,
    {
        let (create_eve, eve) = self.create_eve_did(
            ctx,
            inner.puzzle_hash().into(),
            recovery_list_hash,
            num_verifications_required,
            metadata,
        )?;

        let did = eve.update(ctx, inner, Conditions::new())?;

        Ok((create_eve, did))
    }

    pub fn create_simple_did<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
    ) -> Result<(Conditions, Did<()>), DriverError>
    where
        I: SpendWithConditions,
        Self: Sized,
    {
        self.create_did(ctx, tree_hash_atom(&[]).into(), 1, (), inner)
    }
}
