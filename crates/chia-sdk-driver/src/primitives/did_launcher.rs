use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::{standard::StandardArgs, EveProof, Proof};
use chia_sdk_types::Conditions;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::tree_hash_atom;
use clvmr::Allocator;

use crate::{DriverError, Launcher, SpendContext};

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
        let metadata_ptr = ctx.alloc(&metadata)?;

        let did_info = DidInfo::new(
            launcher_coin.coin_id(),
            recovery_list_hash,
            num_verifications_required,
            ctx.tree_hash(metadata_ptr),
            p2_puzzle_hash,
        );

        let (launch_singleton, eve_coin) =
            self.spend(ctx, did_info.inner_puzzle_hash().into(), ())?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((
            launch_singleton,
            Did::new(eve_coin, proof, did_info.with_metadata(metadata)),
        ))
    }

    pub fn create_did<M>(
        self,
        ctx: &mut SpendContext,
        recovery_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
        synthetic_key: PublicKey,
    ) -> Result<(Conditions, Did<M>), DriverError>
    where
        M: ToClvm<Allocator> + FromClvm<Allocator> + Clone,
        Self: Sized,
    {
        let p2_puzzle_hash = StandardArgs::curry_tree_hash(synthetic_key).into();

        let (create_did, did) = self.create_eve_did(
            ctx,
            p2_puzzle_hash,
            recovery_list_hash,
            num_verifications_required,
            metadata,
        )?;

        let new_did = ctx.spend_standard_did(did, synthetic_key, Conditions::new())?;

        Ok((create_did, new_did))
    }

    pub fn create_simple_did(
        self,
        ctx: &mut SpendContext,
        synthetic_key: PublicKey,
    ) -> Result<(Conditions, Did<()>), DriverError>
    where
        Self: Sized,
    {
        self.create_did(ctx, tree_hash_atom(&[]).into(), 1, (), synthetic_key)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Launcher, SpendContext};

    use chia_puzzles::standard::StandardArgs;
    use chia_sdk_test::{test_secret_key, Simulator};

    #[test]
    fn test_create_did() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let sk = test_secret_key()?;
        let pk = sk.public_key();

        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let coin = sim.new_coin(puzzle_hash, 1);

        let (launch_singleton, did) =
            Launcher::new(coin.coin_id(), 1).create_simple_did(ctx, pk)?;

        ctx.spend_standard_coin(coin, pk, launch_singleton)?;
        sim.spend_coins(ctx.take(), &[sk])?;

        // Make sure the DID was created.
        let coin_state = sim
            .coin_state(did.coin.coin_id())
            .expect("expected did coin");
        assert_eq!(coin_state.coin, did.coin);

        Ok(())
    }
}
