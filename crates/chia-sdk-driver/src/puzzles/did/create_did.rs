use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::{
    did::{DidArgs, DID_INNER_PUZZLE_HASH},
    singleton::SingletonStruct,
    standard::StandardArgs,
    EveProof, Proof,
};
use chia_sdk_types::puzzles::DidInfo;
use clvm_traits::ToClvm;
use clvm_utils::{tree_hash_atom, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{Conditions, Launcher, SpendContext, SpendError};

impl Launcher {
    pub fn create_eve_did<M>(
        self,
        ctx: &mut SpendContext<'_>,
        p2_puzzle_hash: Bytes32,
        recovery_did_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
    ) -> Result<(Conditions, DidInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>,
    {
        let metadata_ptr = ctx.alloc(&metadata)?;
        let metadata_hash = ctx.tree_hash(metadata_ptr);

        let did_inner_puzzle_hash = CurriedProgram {
            program: DID_INNER_PUZZLE_HASH,
            args: DidArgs {
                inner_puzzle: TreeHash::from(p2_puzzle_hash),
                recovery_did_list_hash,
                num_verifications_required,
                metadata: metadata_hash,
                singleton_struct: SingletonStruct::new(self.coin().coin_id()),
            },
        }
        .tree_hash()
        .into();

        let launcher_coin = self.coin();
        let (launch_singleton, eve_coin) = self.spend(ctx, did_inner_puzzle_hash, ())?;

        let proof = Proof::Eve(EveProof {
            parent_coin_info: launcher_coin.parent_coin_info,
            amount: launcher_coin.amount,
        });

        let did_info = DidInfo {
            launcher_id: launcher_coin.coin_id(),
            coin: eve_coin,
            did_inner_puzzle_hash,
            p2_puzzle_hash,
            proof,
            recovery_did_list_hash,
            num_verifications_required,
            metadata,
        };

        Ok((launch_singleton, did_info))
    }

    pub fn create_did<M>(
        self,
        ctx: &mut SpendContext<'_>,
        recovery_did_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
        synthetic_key: PublicKey,
    ) -> Result<(Conditions, DidInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr> + Clone,
        Self: Sized,
    {
        let inner_puzzle_hash = StandardArgs::curry_tree_hash(synthetic_key).into();

        let (create_did, did_info) = self.create_eve_did(
            ctx,
            inner_puzzle_hash,
            recovery_did_list_hash,
            num_verifications_required,
            metadata,
        )?;

        let did_info = ctx.spend_standard_did(&did_info, synthetic_key, Conditions::new())?;

        Ok((create_did, did_info))
    }

    pub fn create_simple_did(
        self,
        ctx: &mut SpendContext<'_>,
        synthetic_key: PublicKey,
    ) -> Result<(Conditions, DidInfo<()>), SpendError>
    where
        Self: Sized,
    {
        self.create_did(ctx, tree_hash_atom(&[]).into(), 1, (), synthetic_key)
    }
}
