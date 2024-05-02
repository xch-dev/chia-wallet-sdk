use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, Program};
use chia_wallet::{
    did::{DidArgs, DidSolution},
    singleton::{
        LauncherSolution, SingletonArgs, SingletonSolution, SingletonStruct,
        SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH,
    },
    standard::StandardArgs,
    EveProof, Proof,
};
use clvm_traits::{clvm_list, ToClvm};
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;
use sha2::{digest::FixedOutput, Digest, Sha256};

use crate::{
    create_launcher, standard_solution, AssertCoinAnnouncement, CreateCoinWithMemos, SpendContext,
    SpendError,
};

/// The output of a DID mint.
pub struct DidCreation {
    /// The conditions that must be output from the parent to make this DID creation valid.
    pub parent_conditions: Vec<NodePtr>,
    /// The coin spends required to fulfill the DID creation.
    pub coin_spends: Vec<CoinSpend>,
    /// The launcher id of the newly created DID.
    pub did_id: Bytes32,
    /// The inner puzzle hash of the DID.
    pub did_inner_puzzle_hash: Bytes32,
    /// The DID coin.
    pub coin: Coin,
    /// The DID puzzle reveal.
    pub puzzle_reveal: Program,
}

/// Creates a new DID singleton.
pub fn create_did(
    ctx: &mut SpendContext,
    parent_coin_id: Bytes32,
    synthetic_key: PublicKey,
    owner_puzzle_hash: Bytes32,
) -> Result<DidCreation, SpendError> {
    let standard_puzzle = ctx.standard_puzzle();
    let launcher_puzzle = ctx.singleton_launcher();
    let singleton_puzzle = ctx.singleton_top_layer();
    let did_puzzle = ctx.did_inner_puzzle();

    let mut coin_spends = Vec::new();

    let launcher = create_launcher(ctx, parent_coin_id)?;
    let launcher_id = launcher.coin.coin_id();
    let mut parent_conditions = launcher.parent_conditions;

    let singleton_struct = SingletonStruct {
        mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
        launcher_id,
        launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
    };

    let p2 = CurriedProgram {
        program: standard_puzzle,
        args: StandardArgs { synthetic_key },
    };

    let did = ctx.alloc(CurriedProgram {
        program: did_puzzle,
        args: DidArgs {
            inner_puzzle: p2,
            recovery_did_list_hash: ctx.tree_hash(NodePtr::NIL),
            num_verifications_required: 1,
            singleton_struct: singleton_struct.clone(),
            metadata: (),
        },
    })?;

    let did_inner_puzzle_hash = ctx.tree_hash(did);

    let singleton = ctx.alloc(CurriedProgram {
        program: singleton_puzzle,
        args: SingletonArgs {
            singleton_struct,
            inner_puzzle: did,
        },
    })?;

    let eve_puzzle_hash = ctx.tree_hash(singleton);

    let eve_message = ctx.alloc(clvm_list!(eve_puzzle_hash, 1, ()))?;
    let eve_message_hash = ctx.tree_hash(eve_message);

    let mut announcement_id = Sha256::new();
    announcement_id.update(launcher_id);
    announcement_id.update(eve_message_hash);

    parent_conditions.push(ctx.alloc(AssertCoinAnnouncement {
        announcement_id: Bytes32::new(announcement_id.finalize_fixed().into()),
    })?);

    // Spend the launcher coin.
    let launcher_puzzle_reveal = ctx.serialize(launcher_puzzle)?;
    let launcher_solution = ctx.serialize(LauncherSolution {
        singleton_puzzle_hash: eve_puzzle_hash,
        amount: 1,
        key_value_list: (),
    })?;

    coin_spends.push(CoinSpend::new(
        launcher.coin,
        launcher_puzzle_reveal,
        launcher_solution,
    ));

    // Spend the eve coin.
    let eve_coin = Coin::new(launcher_id, eve_puzzle_hash, 1);

    let eve_proof = Proof::Eve(EveProof {
        parent_coin_info: parent_coin_id,
        amount: 1,
    });

    let eve_puzzle_reveal = ctx.serialize(singleton)?;

    let eve_coin_spend = spend_did(
        ctx,
        eve_coin.clone(),
        eve_puzzle_reveal.clone(),
        eve_proof,
        clvm_list!(CreateCoinWithMemos {
            puzzle_hash: did_inner_puzzle_hash,
            amount: 1,
            memos: vec![Bytes::new(owner_puzzle_hash.to_vec())],
        },),
    )?;

    coin_spends.push(eve_coin_spend);

    Ok(DidCreation {
        parent_conditions,
        coin_spends,
        did_id: launcher_id,
        did_inner_puzzle_hash,
        coin: Coin::new(eve_coin.coin_id(), eve_puzzle_hash, 1),
        puzzle_reveal: eve_puzzle_reveal,
    })
}

/// Spend a standard DID coin (a DID singleton with the standard transaction inner puzzle).
pub fn spend_did<T>(
    ctx: &mut SpendContext,
    coin: Coin,
    puzzle_reveal: Program,
    proof: Proof,
    conditions: T,
) -> Result<CoinSpend, SpendError>
where
    T: ToClvm<NodePtr>,
{
    let p2_solution = standard_solution(conditions);
    let did_solution = DidSolution::InnerSpend(p2_solution);

    let solution = ctx.serialize(SingletonSolution {
        proof,
        amount: coin.amount,
        inner_solution: did_solution,
    })?;

    Ok(CoinSpend::new(coin, puzzle_reveal, solution))
}

#[cfg(test)]
mod tests {
    use chia_bls::{sign, Signature};
    use chia_protocol::SpendBundle;
    use chia_wallet::{
        standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
        DeriveSynthetic,
    };
    use clvmr::Allocator;

    use crate::{spend_standard_coin, testing::SECRET_KEY, RequiredSignature, WalletSimulator};

    use super::*;

    #[tokio::test]
    async fn test_create_did() {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let sk = SECRET_KEY.derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
        let pk = sk.public_key();
        let puzzle_hash = standard_puzzle_hash(&pk);

        let parent = sim.generate_coin(puzzle_hash.into(), 1).await;

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let did_creation = create_did(
            &mut ctx,
            parent.coin.coin_id(),
            pk.clone(),
            puzzle_hash.into(),
        )
        .unwrap();

        let mut coin_spends = did_creation.coin_spends;

        coin_spends.push(
            spend_standard_coin(&mut ctx, parent.coin, pk, did_creation.parent_conditions).unwrap(),
        );

        let mut spend_bundle = SpendBundle::new(coin_spends, Signature::default());

        let required_signatures = RequiredSignature::from_coin_spends(
            &mut allocator,
            &spend_bundle.coin_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )
        .unwrap();

        for required in required_signatures {
            spend_bundle.aggregated_signature += &sign(&sk, required.final_message());
        }

        let ack = peer.send_transaction(spend_bundle).await.unwrap();
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        // Make sure the DID was created.
        let found_coins = peer
            .register_for_ph_updates(vec![puzzle_hash.into()], 0)
            .await
            .unwrap();
        assert_eq!(found_coins.len(), 2);
    }
}
