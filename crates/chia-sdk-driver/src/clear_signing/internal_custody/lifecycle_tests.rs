use super::*;
use crate::*;
use chia_puzzle_types::{DeriveSynthetic, Proof};
use chia_sdk_types::{Mod, puzzles::*};
use serde_json::Value;
use std::sync::OnceLock;

mod adversarial;
mod consensus;
mod payments;
mod reference;
mod rekeys;

pub(super) fn data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("tests/lifecycle.json")).unwrap())
}
fn unhex(v: &Value) -> Vec<u8> {
    hex::decode(v.as_str().unwrap().trim_start_matches("0x")).unwrap()
}
pub(super) fn bytes(v: &Value) -> Bytes32 {
    unhex(v).try_into().unwrap()
}
fn ctx() -> SpendContext {
    clvmr::Allocator::new_limited(64 * 1024 * 1024).into()
}
pub(super) fn policy(which: &str) -> InternalCustodyPolicy {
    InternalCustodyPolicy::from_legacy_bytes(&unhex(&data()[which])).unwrap()
}
fn coin(v: &Value) -> Coin {
    Coin::new(
        bytes(&v["parent_coin_info"]),
        bytes(&v["puzzle_hash"]),
        v["amount"].as_u64().unwrap(),
    )
}
pub(super) fn spends(v: &Value) -> Vec<CoinSpend> {
    v["spends"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            CoinSpend::new(
                coin(&v["coin"]),
                unhex(&data()["programs"][v["puzzle_reveal_ref"].as_str().unwrap()]).into(),
                unhex(&v["solution"]).into(),
            )
        })
        .collect()
}
pub(super) fn keys(v: &Value) -> Vec<PublicKey> {
    v["signers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| PublicKey::from_bytes(&unhex(k).try_into().unwrap()).unwrap())
        .collect()
}
fn seed(i: u8) -> SecretKey {
    SecretKey::from_seed(&[i; 32])
}
fn proof(ctx: &mut SpendContext, spend: &CoinSpend) -> Proof {
    let node = canonical_program(ctx, spend.solution.as_ref()).unwrap();
    SingletonSolution::<InternalCustodySpendSolution<NodePtr, NodePtr, NodePtr, NodePtr>>::from_clvm(&**ctx, node).unwrap().lineage_proof
}
fn request<'a>(
    v: &Value,
    old: &InternalCustodyPolicy,
    new: &'a InternalCustodyPolicy,
    next: &'a InternalCustodyPolicy,
    signers: &'a [PublicKey],
) -> InternalCustodyOperationRequest<'a> {
    match v["kind"].as_str().unwrap() {
        "clawback" => InternalCustodyOperationRequest::PaymentClawback {
            destination: bytes(&data()["destination"]),
            signers,
        },
        "payment-complete" => InternalCustodyOperationRequest::PaymentCompletion {
            destination: bytes(&data()["destination"]),
        },
        "rekey" => InternalCustodyOperationRequest::Rekey {
            new_policy: new,
            signers,
        },
        "rekey-cancel" => InternalCustodyOperationRequest::RekeyClawback {
            state: rekey_state(
                old,
                v.get("new_root").map_or(new.info().puzzle_root, bytes),
                u8::try_from(v["multiple"].as_u64().unwrap()).unwrap(),
            ),
            signers,
        },
        "rekey-complete" => InternalCustodyOperationRequest::RekeyCompletion { new_policy: new },
        "lock-complete" => InternalCustodyOperationRequest::RekeyCompletion { new_policy: next },
        "lock" => InternalCustodyOperationRequest::IncreaseSecurity {
            new_policy: next,
            signers,
        },
        _ => panic!("not a lifecycle operation"),
    }
}
fn signature(v: &Value) -> Signature {
    if v["contexts"].as_array().unwrap().is_empty() {
        return Signature::default();
    }
    let required = &v["contexts"][0];
    let message = unhex(&required["message"]);
    let final_key =
        PublicKey::from_bytes(&unhex(&required["public_key"]).try_into().unwrap()).unwrap();
    let selected = keys(v);
    let mut secret = (1..=6)
        .map(seed)
        .filter(|sk| selected.contains(&sk.public_key()))
        .reduce(|a, b| a + &b)
        .unwrap();
    if secret.public_key() != final_key {
        secret = secret.derive_synthetic();
    }
    assert_eq!(secret.public_key(), final_key);
    chia_bls::sign(&secret, message)
}
pub(super) fn operation(name: &str) -> &'static Value {
    data()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == name)
        .unwrap()
}
pub(super) fn constants() -> AggSigConstants {
    AggSigConstants::new(bytes(&data()["domain"]))
}
fn sign_required(required: &RequiredBlsSignature, selected: &[PublicKey]) -> Signature {
    let mut secret = (1..=6)
        .map(seed)
        .filter(|sk| selected.contains(&sk.public_key()))
        .reduce(|a, b| a + &b)
        .unwrap();
    if secret.public_key() != required.public_key {
        secret = secret.derive_synthetic();
    }
    assert_eq!(secret.public_key(), required.public_key);
    chia_bls::sign(&secret, required.message())
}
fn sign_payment(
    ctx: &mut SpendContext,
    p: &InternalCustodyPolicy,
    input: &[CoinSpend],
    keys: &[PublicKey],
) -> anyhow::Result<Signature> {
    let verified = parse_internal_custody_payment(ctx, p, input, keys, &constants(), 500_000_000)?;
    Ok(sign_required(verified.required_signature(), keys))
}
fn child_proof(
    ctx: &mut SpendContext,
    p: &InternalCustodyPolicy,
    parent: Coin,
) -> anyhow::Result<Proof> {
    let inner = p.inner_layer(ctx)?.construct_puzzle(ctx)?;
    Ok(Proof::Lineage(chia_puzzle_types::LineageProof {
        parent_parent_coin_info: parent.parent_coin_info,
        parent_inner_puzzle_hash: ctx.tree_hash(inner).into(),
        parent_amount: parent.amount,
    }))
}

fn rekey_state(
    old: &InternalCustodyPolicy,
    new_root: Bytes32,
    timelock_multiple: u8,
) -> InternalCustodyRekeyState {
    InternalCustodyRekeyState {
        old_root: old.info().puzzle_root,
        new_root,
        timelock_multiple,
    }
}
