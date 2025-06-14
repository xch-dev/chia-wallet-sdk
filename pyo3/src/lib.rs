#![allow(clippy::too_many_arguments)]

use bindy::{FromRust, Pyo3Context};
use num_bigint::BigInt;
use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{PyList, PyNone, PyTuple},
};

bindy_macro::bindy_pyo3!("bindings.json");

#[pymethods]
impl Clvm {
    pub fn alloc(&self, value: Bound<'_, PyAny>) -> PyResult<Program> {
        Ok(Program::from_rust(alloc(&self.0, value)?, &Pyo3Context)?)
    }
}

pub fn alloc(
    clvm: &chia_sdk_bindings::Clvm,
    value: Bound<'_, PyAny>,
) -> PyResult<chia_sdk_bindings::Program> {
    if let Ok(_value) = value.downcast::<PyNone>() {
        Ok(clvm.nil()?)
    } else if let Ok(value) = value.extract::<BigInt>() {
        Ok(clvm.int(value)?)
    } else if let Ok(value) = value.extract::<bool>() {
        Ok(clvm.bool(value)?)
    } else if let Ok(value) = value.extract::<String>() {
        Ok(clvm.string(value)?)
    } else if let Ok(value) = value.extract::<Vec<u8>>() {
        Ok(clvm.atom(value.into())?)
    } else if let Ok(value) = value.extract::<Program>() {
        Ok(value.0)
    } else if let Ok(value) = value.extract::<PublicKey>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<Signature>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<K1PublicKey>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<K1Signature>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<R1PublicKey>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<R1Signature>() {
        Ok(clvm.atom(value.to_bytes()?.to_vec().into())?)
    } else if let Ok(value) = value.extract::<CurriedProgram>() {
        Ok(value.0.program.curry(value.0.args.clone())?)
    } else if let Ok(value) = value.downcast::<PyTuple>() {
        if value.len() != 2 {
            return PyResult::Err(PyErr::new::<PyTypeError, _>(
                "Expected a tuple with 2 items",
            ));
        }

        let first = alloc(clvm, value.get_item(0)?)?;
        let rest = alloc(clvm, value.get_item(1)?)?;

        Ok(clvm.pair(first, rest)?)
    } else if let Ok(value) = value.extract::<Pair>() {
        Ok(clvm.pair(value.0.first, value.0.rest)?)
    } else if let Ok(value) = value.downcast::<PyList>() {
        let mut list = Vec::new();

        for item in value.iter() {
            list.push(alloc(clvm, item)?);
        }

        Ok(clvm.list(list)?)
    } else if let Ok(value) = value.extract::<Remark>() {
        Ok(clvm.remark(value.0.rest)?)
    } else if let Ok(value) = value.extract::<AggSigParent>() {
        Ok(clvm.agg_sig_parent(value.0.public_key, value.0.message)?)
    } else if let Ok(value) = value.extract::<AggSigPuzzle>() {
        Ok(clvm.agg_sig_puzzle(value.0.public_key, value.0.message)?)
    } else if let Ok(value) = value.extract::<AggSigAmount>() {
        Ok(clvm.agg_sig_amount(value.0.public_key, value.0.message)?)
    } else if let Ok(value) = value.extract::<AggSigPuzzleAmount>() {
        Ok(clvm.agg_sig_puzzle_amount(value.0.public_key, value.0.message)?)
    } else if let Ok(value) = value.extract::<AggSigParentAmount>() {
        Ok(clvm.agg_sig_parent_amount(value.0.public_key, value.0.message)?)
    } else if let Ok(value) = value.extract::<AggSigParentPuzzle>() {
        Ok(clvm.agg_sig_parent_puzzle(value.0.public_key, value.0.message)?)
    } else if let Ok(value) = value.extract::<AggSigUnsafe>() {
        Ok(clvm.agg_sig_unsafe(value.0.public_key, value.0.message)?)
    } else if let Ok(value) = value.extract::<AggSigMe>() {
        Ok(clvm.agg_sig_me(value.0.public_key, value.0.message)?)
    } else if let Ok(value) = value.extract::<CreateCoin>() {
        Ok(clvm.create_coin(value.0.puzzle_hash, value.0.amount, value.0.memos)?)
    } else if let Ok(value) = value.extract::<ReserveFee>() {
        Ok(clvm.reserve_fee(value.0.amount)?)
    } else if let Ok(value) = value.extract::<CreateCoinAnnouncement>() {
        Ok(clvm.create_coin_announcement(value.0.message)?)
    } else if let Ok(value) = value.extract::<CreatePuzzleAnnouncement>() {
        Ok(clvm.create_puzzle_announcement(value.0.message)?)
    } else if let Ok(value) = value.extract::<AssertCoinAnnouncement>() {
        Ok(clvm.assert_coin_announcement(value.0.announcement_id)?)
    } else if let Ok(value) = value.extract::<AssertPuzzleAnnouncement>() {
        Ok(clvm.assert_puzzle_announcement(value.0.announcement_id)?)
    } else if let Ok(value) = value.extract::<AssertConcurrentSpend>() {
        Ok(clvm.assert_concurrent_spend(value.0.coin_id)?)
    } else if let Ok(value) = value.extract::<AssertConcurrentPuzzle>() {
        Ok(clvm.assert_concurrent_puzzle(value.0.puzzle_hash)?)
    } else if let Ok(value) = value.extract::<AssertSecondsRelative>() {
        Ok(clvm.assert_seconds_relative(value.0.seconds)?)
    } else if let Ok(value) = value.extract::<AssertSecondsAbsolute>() {
        Ok(clvm.assert_seconds_absolute(value.0.seconds)?)
    } else if let Ok(value) = value.extract::<AssertHeightRelative>() {
        Ok(clvm.assert_height_relative(value.0.height)?)
    } else if let Ok(value) = value.extract::<AssertHeightAbsolute>() {
        Ok(clvm.assert_height_absolute(value.0.height)?)
    } else if let Ok(value) = value.extract::<AssertBeforeSecondsRelative>() {
        Ok(clvm.assert_before_seconds_relative(value.0.seconds)?)
    } else if let Ok(value) = value.extract::<AssertBeforeSecondsAbsolute>() {
        Ok(clvm.assert_before_seconds_absolute(value.0.seconds)?)
    } else if let Ok(value) = value.extract::<AssertBeforeHeightRelative>() {
        Ok(clvm.assert_before_height_relative(value.0.height)?)
    } else if let Ok(value) = value.extract::<AssertBeforeHeightAbsolute>() {
        Ok(clvm.assert_before_height_absolute(value.0.height)?)
    } else if let Ok(value) = value.extract::<AssertMyCoinId>() {
        Ok(clvm.assert_my_coin_id(value.0.coin_id)?)
    } else if let Ok(value) = value.extract::<AssertMyParentId>() {
        Ok(clvm.assert_my_parent_id(value.0.parent_id)?)
    } else if let Ok(value) = value.extract::<AssertMyPuzzleHash>() {
        Ok(clvm.assert_my_puzzle_hash(value.0.puzzle_hash)?)
    } else if let Ok(value) = value.extract::<AssertMyAmount>() {
        Ok(clvm.assert_my_amount(value.0.amount)?)
    } else if let Ok(value) = value.extract::<AssertMyBirthSeconds>() {
        Ok(clvm.assert_my_birth_seconds(value.0.seconds)?)
    } else if let Ok(value) = value.extract::<AssertMyBirthHeight>() {
        Ok(clvm.assert_my_birth_height(value.0.height)?)
    } else if let Ok(_value) = value.extract::<AssertEphemeral>() {
        Ok(clvm.assert_ephemeral()?)
    } else if let Ok(value) = value.extract::<SendMessage>() {
        Ok(clvm.send_message(value.0.mode, value.0.message, value.0.data)?)
    } else if let Ok(value) = value.extract::<ReceiveMessage>() {
        Ok(clvm.receive_message(value.0.mode, value.0.message, value.0.data)?)
    } else if let Ok(value) = value.extract::<Softfork>() {
        Ok(clvm.softfork(value.0.cost, value.0.rest)?)
    } else if let Ok(_value) = value.extract::<MeltSingleton>() {
        Ok(clvm.melt_singleton()?)
    } else if let Ok(value) = value.extract::<TransferNft>() {
        Ok(clvm.transfer_nft(
            value.0.launcher_id,
            value.0.trade_prices.clone(),
            value.0.singleton_inner_puzzle_hash,
        )?)
    } else if let Ok(value) = value.extract::<RunCatTail>() {
        Ok(clvm.run_cat_tail(value.0.program.clone(), value.0.solution.clone())?)
    } else if let Ok(value) = value.extract::<UpdateNftMetadata>() {
        Ok(clvm.update_nft_metadata(
            value.0.updater_puzzle_reveal.clone(),
            value.0.updater_solution.clone(),
        )?)
    } else if let Ok(value) = value.extract::<UpdateDataStoreMerkleRoot>() {
        Ok(clvm.update_data_store_merkle_root(value.0.new_merkle_root, value.0.memos.clone())?)
    } else if let Ok(value) = value.extract::<NftMetadata>() {
        Ok(clvm.nft_metadata(value.0.clone())?)
    } else if let Ok(value) = value.extract::<MipsMemo>() {
        Ok(clvm.mips_memo(value.0.clone())?)
    } else if let Ok(value) = value.extract::<InnerPuzzleMemo>() {
        Ok(clvm.inner_puzzle_memo(value.0.clone())?)
    } else if let Ok(value) = value.extract::<RestrictionMemo>() {
        Ok(clvm.restriction_memo(value.0.clone())?)
    } else if let Ok(value) = value.extract::<WrapperMemo>() {
        Ok(clvm.wrapper_memo(value.0.clone())?)
    } else if let Ok(value) = value.extract::<Force1of2RestrictedVariableMemo>() {
        Ok(clvm.force_1_of_2_restricted_variable_memo(value.0.clone())?)
    } else if let Ok(value) = value.extract::<MemoKind>() {
        Ok(clvm.memo_kind(value.0.clone())?)
    } else if let Ok(value) = value.extract::<MemberMemo>() {
        Ok(clvm.member_memo(value.0.clone())?)
    } else if let Ok(value) = value.extract::<MofNMemo>() {
        Ok(clvm.m_of_n_memo(value.0.clone())?)
    } else {
        PyResult::Err(PyErr::new::<PyTypeError, _>("Unsupported CLVM value type"))
    }
}
