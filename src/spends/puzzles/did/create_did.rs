use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_wallet::{
    did::DID_INNER_PUZZLE_HASH,
    singleton::{SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
    standard::standard_puzzle_hash,
    EveProof, Proof,
};
use clvm_traits::ToClvm;
use clvm_utils::{curry_tree_hash, tree_hash_atom, tree_hash_pair};
use clvmr::NodePtr;

use crate::{
    u64_to_bytes, ChainedSpend, DidInfo, SpendContext, SpendError, SpendableLauncher,
    StandardDidSpend,
};

pub trait CreateDid {
    fn create_eve_did<M>(
        self,
        ctx: &mut SpendContext,
        inner_puzzle_hash: Bytes32,
        recovery_did_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
    ) -> Result<(ChainedSpend, DidInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>;

    fn create_custom_standard_did<M>(
        self,
        ctx: &mut SpendContext,
        recovery_did_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
        synthetic_key: PublicKey,
    ) -> Result<(ChainedSpend, DidInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>,
        Self: Sized,
    {
        let inner_puzzle_hash = standard_puzzle_hash(&synthetic_key).into();

        let (create_did, did_info) = self.create_eve_did(
            ctx,
            inner_puzzle_hash,
            recovery_did_list_hash,
            num_verifications_required,
            metadata,
        )?;

        let did_info = StandardDidSpend::new()
            .recreate()
            .finish(ctx, synthetic_key, did_info)?;

        Ok((create_did, did_info))
    }

    fn create_standard_did(
        self,
        ctx: &mut SpendContext,
        synthetic_key: PublicKey,
    ) -> Result<(ChainedSpend, DidInfo<()>), SpendError>
    where
        Self: Sized,
    {
        self.create_custom_standard_did(ctx, tree_hash_atom(&[]).into(), 1, (), synthetic_key)
    }
}

impl CreateDid for SpendableLauncher {
    fn create_eve_did<M>(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
        recovery_did_list_hash: Bytes32,
        num_verifications_required: u64,
        metadata: M,
    ) -> Result<(ChainedSpend, DidInfo<M>), SpendError>
    where
        M: ToClvm<NodePtr>,
    {
        let metadata_ptr = ctx.alloc(&metadata)?;
        let metadata_hash = ctx.tree_hash(metadata_ptr);

        let did_inner_puzzle_hash = did_inner_puzzle_hash(
            p2_puzzle_hash,
            recovery_did_list_hash,
            num_verifications_required,
            self.coin().coin_id(),
            metadata_hash,
        );

        let launcher_coin = self.coin().clone();
        let (chained_spend, eve_coin) = self.spend(ctx, did_inner_puzzle_hash, ())?;

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

        Ok((chained_spend, did_info))
    }
}

pub fn did_inner_puzzle_hash(
    inner_puzzle_hash: Bytes32,
    recovery_did_list_hash: Bytes32,
    num_verifications_required: u64,
    launcher_id: Bytes32,
    metadata_hash: Bytes32,
) -> Bytes32 {
    let recovery_hash = tree_hash_atom(&recovery_did_list_hash);
    let num_verifications_hash = tree_hash_atom(&u64_to_bytes(num_verifications_required));

    let singleton_hash = tree_hash_atom(&SINGLETON_TOP_LAYER_PUZZLE_HASH);
    let launcher_id_hash = tree_hash_atom(&launcher_id);
    let launcher_puzzle_hash = tree_hash_atom(&SINGLETON_LAUNCHER_PUZZLE_HASH);

    let pair = tree_hash_pair(launcher_id_hash, launcher_puzzle_hash);
    let singleton_struct_hash = tree_hash_pair(singleton_hash, pair);

    curry_tree_hash(
        DID_INNER_PUZZLE_HASH,
        &[
            inner_puzzle_hash.into(),
            recovery_hash,
            num_verifications_hash,
            singleton_struct_hash,
            metadata_hash.into(),
        ],
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_wallet::{
        did::DidArgs,
        singleton::{
            SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH,
        },
    };
    use clvm_utils::CurriedProgram;
    use clvmr::Allocator;

    #[test]
    fn test_puzzle_hash() {
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let inner_puzzle = ctx.alloc([1, 2, 3]).unwrap();
        let inner_puzzle_hash = ctx.tree_hash(inner_puzzle);

        let metadata = ctx.alloc([4, 5, 6]).unwrap();
        let metadata_hash = ctx.tree_hash(metadata);

        let launcher_id = Bytes32::new([34; 32]);
        let recovery_did_list_hash = Bytes32::new([42; 32]);
        let num_verifications_required = 2;

        let did_inner_puzzle = ctx.did_inner_puzzle();

        let puzzle = ctx
            .alloc(CurriedProgram {
                program: did_inner_puzzle,
                args: DidArgs {
                    inner_puzzle,
                    recovery_did_list_hash,
                    num_verifications_required,
                    singleton_struct: SingletonStruct {
                        mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                        launcher_id,
                        launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
                    },
                    metadata,
                },
            })
            .unwrap();
        let allocated_puzzle_hash = ctx.tree_hash(puzzle);

        let puzzle_hash = did_inner_puzzle_hash(
            inner_puzzle_hash,
            recovery_did_list_hash,
            num_verifications_required,
            launcher_id,
            metadata_hash,
        );

        assert_eq!(hex::encode(allocated_puzzle_hash), hex::encode(puzzle_hash));
    }
}
