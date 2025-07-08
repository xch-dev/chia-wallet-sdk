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

    #[napi]
    pub fn bound_checked_number(&self, env: Env, value: f64) -> Result<Program> {
        Ok(Program::from_rust(
            self.0.f64(value)?,
            &NapiReturnContext(env),
        )?)
    }
}

#[napi]
impl Program {
    #[napi]
    pub fn to_bound_checked_number(&self) -> Result<Option<f64>> {
        Ok(self.0.to_small_int()?)
    }
}

pub type Value<'a> = Either26<
    f64,
    BigInt,
    bool,
    String,
    Uint8Array,
    Array<'a>,
    Null,
    ClassInstance<'a, Program>,
    ClassInstance<'a, PublicKey>,
    ClassInstance<'a, Signature>,
    ClassInstance<'a, K1PublicKey>,
    ClassInstance<'a, K1Signature>,
    ClassInstance<'a, R1PublicKey>,
    ClassInstance<'a, R1Signature>,
    ClassInstance<'a, Remark>,
    ClassInstance<'a, AggSigParent>,
    ClassInstance<'a, AggSigPuzzle>,
    ClassInstance<'a, AggSigAmount>,
    ClassInstance<'a, AggSigPuzzleAmount>,
    ClassInstance<'a, AggSigParentAmount>,
    ClassInstance<'a, AggSigParentPuzzle>,
    ClassInstance<'a, AggSigUnsafe>,
    ClassInstance<'a, AggSigMe>,
    ClassInstance<'a, CreateCoin>,
    ClassInstance<'a, ReserveFee>,
    Value2<'a>,
>;

type Value2<'a> = Either26<
    ClassInstance<'a, CreateCoinAnnouncement>,
    ClassInstance<'a, CreatePuzzleAnnouncement>,
    ClassInstance<'a, AssertCoinAnnouncement>,
    ClassInstance<'a, AssertPuzzleAnnouncement>,
    ClassInstance<'a, AssertConcurrentSpend>,
    ClassInstance<'a, AssertConcurrentPuzzle>,
    ClassInstance<'a, AssertSecondsRelative>,
    ClassInstance<'a, AssertSecondsAbsolute>,
    ClassInstance<'a, AssertHeightRelative>,
    ClassInstance<'a, AssertHeightAbsolute>,
    ClassInstance<'a, AssertBeforeSecondsRelative>,
    ClassInstance<'a, AssertBeforeSecondsAbsolute>,
    ClassInstance<'a, AssertBeforeHeightRelative>,
    ClassInstance<'a, AssertBeforeHeightAbsolute>,
    ClassInstance<'a, AssertMyCoinId>,
    ClassInstance<'a, AssertMyParentId>,
    ClassInstance<'a, AssertMyPuzzleHash>,
    ClassInstance<'a, AssertMyAmount>,
    ClassInstance<'a, AssertMyBirthSeconds>,
    ClassInstance<'a, AssertMyBirthHeight>,
    ClassInstance<'a, AssertEphemeral>,
    ClassInstance<'a, SendMessage>,
    ClassInstance<'a, ReceiveMessage>,
    ClassInstance<'a, Softfork>,
    ClassInstance<'a, Pair>,
    Value3<'a>,
>;

type Value3<'a> = Either15<
    ClassInstance<'a, NftMetadata>,
    ClassInstance<'a, CurriedProgram>,
    ClassInstance<'a, MipsMemo>,
    ClassInstance<'a, InnerPuzzleMemo>,
    ClassInstance<'a, RestrictionMemo>,
    ClassInstance<'a, WrapperMemo>,
    ClassInstance<'a, Force1of2RestrictedVariableMemo>,
    ClassInstance<'a, MemoKind>,
    ClassInstance<'a, MemberMemo>,
    ClassInstance<'a, MofNMemo>,
    ClassInstance<'a, MeltSingleton>,
    ClassInstance<'a, TransferNft>,
    ClassInstance<'a, RunCatTail>,
    ClassInstance<'a, UpdateNftMetadata>,
    ClassInstance<'a, UpdateDataStoreMerkleRoot>,
>;

fn alloc<'a>(
    env: Env,
    clvm: &chia_sdk_bindings::Clvm,
    value: Value<'a>,
) -> bindy::Result<chia_sdk_bindings::Program> {
    match value {
        Value::A(value) => clvm.f64(value),
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
        Value::G(_) => clvm.nil(),
        Value::H(value) => Ok(value.0.clone()),
        Value::I(value) => clvm.atom(value.to_bytes(env)?.to_vec().into()),
        Value::J(value) => clvm.atom(value.to_bytes(env)?.to_vec().into()),
        Value::K(value) => clvm.atom(value.to_bytes(env)?.to_vec().into()),
        Value::L(value) => clvm.atom(value.to_bytes(env)?.to_vec().into()),
        Value::M(value) => clvm.atom(value.to_bytes(env)?.to_vec().into()),
        Value::N(value) => clvm.atom(value.to_bytes(env)?.to_vec().into()),
        Value::O(value) => clvm.remark(value.0.rest.clone()),
        Value::P(value) => clvm.agg_sig_parent(value.0.public_key, value.0.message.clone()),
        Value::Q(value) => clvm.agg_sig_puzzle(value.0.public_key, value.0.message.clone()),
        Value::R(value) => clvm.agg_sig_amount(value.0.public_key, value.0.message.clone()),
        Value::S(value) => clvm.agg_sig_puzzle_amount(value.0.public_key, value.0.message.clone()),
        Value::T(value) => clvm.agg_sig_parent_amount(value.0.public_key, value.0.message.clone()),
        Value::U(value) => clvm.agg_sig_parent_puzzle(value.0.public_key, value.0.message.clone()),
        Value::V(value) => clvm.agg_sig_unsafe(value.0.public_key, value.0.message.clone()),
        Value::W(value) => clvm.agg_sig_me(value.0.public_key, value.0.message.clone()),
        Value::X(value) => {
            clvm.create_coin(value.0.puzzle_hash, value.0.amount, value.0.memos.clone())
        }
        Value::Y(value) => clvm.reserve_fee(value.0.amount),
        Value::Z(value) => match value {
            Value2::A(value) => clvm.create_coin_announcement(value.0.message.clone()),
            Value2::B(value) => clvm.create_puzzle_announcement(value.0.message.clone()),
            Value2::C(value) => clvm.assert_coin_announcement(value.0.announcement_id),
            Value2::D(value) => clvm.assert_puzzle_announcement(value.0.announcement_id),
            Value2::E(value) => clvm.assert_concurrent_spend(value.0.coin_id),
            Value2::F(value) => clvm.assert_concurrent_puzzle(value.0.puzzle_hash),
            Value2::G(value) => clvm.assert_seconds_relative(value.0.seconds),
            Value2::H(value) => clvm.assert_seconds_absolute(value.0.seconds),
            Value2::I(value) => clvm.assert_height_relative(value.0.height),
            Value2::J(value) => clvm.assert_height_absolute(value.0.height),
            Value2::K(value) => clvm.assert_before_seconds_relative(value.0.seconds),
            Value2::L(value) => clvm.assert_before_seconds_absolute(value.0.seconds),
            Value2::M(value) => clvm.assert_before_height_relative(value.0.height),
            Value2::N(value) => clvm.assert_before_height_absolute(value.0.height),
            Value2::O(value) => clvm.assert_my_coin_id(value.0.coin_id),
            Value2::P(value) => clvm.assert_my_parent_id(value.0.parent_id),
            Value2::Q(value) => clvm.assert_my_puzzle_hash(value.0.puzzle_hash),
            Value2::R(value) => clvm.assert_my_amount(value.0.amount),
            Value2::S(value) => clvm.assert_my_birth_seconds(value.0.seconds),
            Value2::T(value) => clvm.assert_my_birth_height(value.0.height),
            Value2::U(_value) => clvm.assert_ephemeral(),
            Value2::V(value) => {
                clvm.send_message(value.0.mode, value.0.message.clone(), value.0.data.clone())
            }
            Value2::W(value) => {
                clvm.receive_message(value.0.mode, value.0.message.clone(), value.0.data.clone())
            }
            Value2::X(value) => clvm.softfork(value.0.cost, value.0.rest.clone()),
            Value2::Y(value) => clvm.pair(value.0.first.clone(), value.0.rest.clone()),
            Value2::Z(value) => match value {
                Value3::A(value) => clvm.nft_metadata(value.0.clone()),
                Value3::B(value) => value.0.program.curry(value.0.args.clone()),
                Value3::C(value) => clvm.mips_memo(value.0.clone()),
                Value3::D(value) => clvm.inner_puzzle_memo(value.0.clone()),
                Value3::E(value) => clvm.restriction_memo(value.0.clone()),
                Value3::F(value) => clvm.wrapper_memo(value.0.clone()),
                Value3::G(value) => clvm.force_1_of_2_restricted_variable_memo(value.0.clone()),
                Value3::H(value) => clvm.memo_kind(value.0.clone()),
                Value3::I(value) => clvm.member_memo(value.0.clone()),
                Value3::J(value) => clvm.m_of_n_memo(value.0.clone()),
                Value3::K(_value) => clvm.melt_singleton(),
                Value3::L(value) => clvm.transfer_nft(
                    value.0.launcher_id,
                    value.0.trade_prices.clone(),
                    value.0.singleton_inner_puzzle_hash,
                ),
                Value3::M(value) => {
                    clvm.run_cat_tail(value.0.program.clone(), value.0.solution.clone())
                }
                Value3::N(value) => clvm.update_nft_metadata(
                    value.0.updater_puzzle_reveal.clone(),
                    value.0.updater_solution.clone(),
                ),
                Value3::O(value) => clvm
                    .update_data_store_merkle_root(value.0.new_merkle_root, value.0.memos.clone()),
            },
        },
    }
}
