#![allow(clippy::wildcard_imports)]
#![allow(clippy::too_many_arguments)]

use bindy::{FromRust, IntoRust, NapiParamContext, NapiReturnContext};
use napi::bindgen_prelude::*;
use napi_derive::napi;

bindy_macro::bindy_napi!("bindings.json");

#[napi]
impl Clvm {
    #[napi]
    pub fn alloc(&self, env: Env, value: Value<'_>) -> Result<Program> {
        Ok(Program::from_rust(
            alloc(env, &self.0, value)?,
            &NapiReturnContext(env),
        )?)
    }
}

pub type Value<'a> =
    Either9<f64, BigInt, bool, String, Uint8Array, Array<'a>, Null, Undefined, Value1<'a>>;

fn alloc<'a>(
    env: Env,
    clvm: &chia_sdk_bindings::Clvm,
    value: Value<'a>,
) -> bindy::Result<chia_sdk_bindings::Program> {
    match value {
        Value::A(value) => clvm.bound_checked_number(value),
        Value::B(value) => clvm.int(value.into_rust(&NapiParamContext)?),
        Value::C(value) => clvm.bool(value),
        Value::D(value) => clvm.string(value),
        Value::E(value) => clvm.atom(value.to_vec().into()),
        Value::F(value) => {
            let mut list = Vec::new();

            for index in 0..value.len() {
                let item = value.get::<Value<'a>>(index)?.unwrap();
                list.push(alloc(env, clvm, item)?);
            }

            Ok(clvm.list(list)?)
        }
        Value::G(..) | Value::H(..) => clvm.nil(),
        Value::I(value) => Ok(match extract_clvm_type(value) {
            ClvmType::Program(value) => value.0,
            ClvmType::Pair(value) => clvm.pair(value.0.first, value.0.rest)?,
            ClvmType::CurriedProgram(value) => value.0.program.curry(value.0.args.clone())?,
            ClvmType::PublicKey(value) => clvm.atom(value.to_bytes(env)?.to_vec().into())?,
            ClvmType::Signature(value) => clvm.atom(value.to_bytes(env)?.to_vec().into())?,
            ClvmType::K1PublicKey(value) => clvm.atom(value.to_bytes(env)?.to_vec().into())?,
            ClvmType::K1Signature(value) => clvm.atom(value.to_bytes(env)?.to_vec().into())?,
            ClvmType::R1PublicKey(value) => clvm.atom(value.to_bytes(env)?.to_vec().into())?,
            ClvmType::R1Signature(value) => clvm.atom(value.to_bytes(env)?.to_vec().into())?,
            ClvmType::Remark(value) => clvm.remark(value.0.rest)?,
            ClvmType::AggSigParent(value) => {
                clvm.agg_sig_parent(value.0.public_key, value.0.message)?
            }
            ClvmType::AggSigPuzzle(value) => {
                clvm.agg_sig_puzzle(value.0.public_key, value.0.message)?
            }
            ClvmType::AggSigAmount(value) => {
                clvm.agg_sig_amount(value.0.public_key, value.0.message)?
            }
            ClvmType::AggSigPuzzleAmount(value) => {
                clvm.agg_sig_puzzle_amount(value.0.public_key, value.0.message)?
            }
            ClvmType::AggSigParentAmount(value) => {
                clvm.agg_sig_parent_amount(value.0.public_key, value.0.message)?
            }
            ClvmType::AggSigParentPuzzle(value) => {
                clvm.agg_sig_parent_puzzle(value.0.public_key, value.0.message)?
            }
            ClvmType::AggSigUnsafe(value) => {
                clvm.agg_sig_unsafe(value.0.public_key, value.0.message)?
            }
            ClvmType::AggSigMe(value) => clvm.agg_sig_me(value.0.public_key, value.0.message)?,
            ClvmType::CreateCoin(value) => {
                clvm.create_coin(value.0.puzzle_hash, value.0.amount, value.0.memos)?
            }
            ClvmType::ReserveFee(value) => clvm.reserve_fee(value.0.amount)?,
            ClvmType::CreateCoinAnnouncement(value) => {
                clvm.create_coin_announcement(value.0.message)?
            }
            ClvmType::CreatePuzzleAnnouncement(value) => {
                clvm.create_puzzle_announcement(value.0.message)?
            }
            ClvmType::AssertCoinAnnouncement(value) => {
                clvm.assert_coin_announcement(value.0.announcement_id)?
            }
            ClvmType::AssertPuzzleAnnouncement(value) => {
                clvm.assert_puzzle_announcement(value.0.announcement_id)?
            }
            ClvmType::AssertConcurrentSpend(value) => {
                clvm.assert_concurrent_spend(value.0.coin_id)?
            }
            ClvmType::AssertConcurrentPuzzle(value) => {
                clvm.assert_concurrent_puzzle(value.0.puzzle_hash)?
            }
            ClvmType::AssertSecondsRelative(value) => {
                clvm.assert_seconds_relative(value.0.seconds)?
            }
            ClvmType::AssertSecondsAbsolute(value) => {
                clvm.assert_seconds_absolute(value.0.seconds)?
            }
            ClvmType::AssertHeightRelative(value) => clvm.assert_height_relative(value.0.height)?,
            ClvmType::AssertHeightAbsolute(value) => clvm.assert_height_absolute(value.0.height)?,
            ClvmType::AssertBeforeSecondsRelative(value) => {
                clvm.assert_before_seconds_relative(value.0.seconds)?
            }
            ClvmType::AssertBeforeSecondsAbsolute(value) => {
                clvm.assert_before_seconds_absolute(value.0.seconds)?
            }
            ClvmType::AssertBeforeHeightRelative(value) => {
                clvm.assert_before_height_relative(value.0.height)?
            }
            ClvmType::AssertBeforeHeightAbsolute(value) => {
                clvm.assert_before_height_absolute(value.0.height)?
            }
            ClvmType::AssertMyCoinId(value) => clvm.assert_my_coin_id(value.0.coin_id)?,
            ClvmType::AssertMyParentId(value) => clvm.assert_my_parent_id(value.0.parent_id)?,
            ClvmType::AssertMyPuzzleHash(value) => {
                clvm.assert_my_puzzle_hash(value.0.puzzle_hash)?
            }
            ClvmType::AssertMyAmount(value) => clvm.assert_my_amount(value.0.amount)?,
            ClvmType::AssertMyBirthSeconds(value) => {
                clvm.assert_my_birth_seconds(value.0.seconds)?
            }
            ClvmType::AssertMyBirthHeight(value) => clvm.assert_my_birth_height(value.0.height)?,
            ClvmType::AssertEphemeral(_value) => clvm.assert_ephemeral()?,
            ClvmType::SendMessage(value) => {
                clvm.send_message(value.0.mode, value.0.message, value.0.data)?
            }
            ClvmType::ReceiveMessage(value) => {
                clvm.receive_message(value.0.mode, value.0.message, value.0.data)?
            }
            ClvmType::Softfork(value) => clvm.softfork(value.0.cost, value.0.rest)?,
            ClvmType::MeltSingleton(_value) => clvm.melt_singleton()?,
            ClvmType::TransferNft(value) => clvm.transfer_nft(
                value.0.launcher_id,
                value.0.trade_prices.clone(),
                value.0.singleton_inner_puzzle_hash,
            )?,
            ClvmType::RunCatTail(value) => {
                clvm.run_cat_tail(value.0.program.clone(), value.0.solution.clone())?
            }
            ClvmType::UpdateNftMetadata(value) => clvm.update_nft_metadata(
                value.0.updater_puzzle_reveal.clone(),
                value.0.updater_solution.clone(),
            )?,
            ClvmType::UpdateDataStoreMerkleRoot(value) => {
                clvm.update_data_store_merkle_root(value.0.new_merkle_root, value.0.memos.clone())?
            }
            ClvmType::NftMetadata(value) => clvm.nft_metadata(value.0.clone())?,
            ClvmType::MipsMemo(value) => clvm.mips_memo(value.0.clone())?,
            ClvmType::InnerPuzzleMemo(value) => clvm.inner_puzzle_memo(value.0.clone())?,
            ClvmType::RestrictionMemo(value) => clvm.restriction_memo(value.0.clone())?,
            ClvmType::WrapperMemo(value) => clvm.wrapper_memo(value.0.clone())?,
            ClvmType::Force1of2RestrictedVariableMemo(value) => {
                clvm.force_1_of_2_restricted_variable_memo(value.0.clone())?
            }
            ClvmType::MemoKind(value) => clvm.memo_kind(value.0.clone())?,
            ClvmType::MemberMemo(value) => clvm.member_memo(value.0.clone())?,
            ClvmType::MofNMemo(value) => clvm.m_of_n_memo(value.0.clone())?,
        }),
    }
}
