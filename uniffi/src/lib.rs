#![allow(unsafe_code)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::inherent_to_string)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::result_large_err)]

use std::sync::Arc;

use bindy::{FromRust, UniffiContext};

uniffi::setup_scaffolding!("chiawalletsdk");

#[cfg(not(feature = "blocking"))]
bindy_macro::bindy_uniffi!("bindings.json");

#[cfg(feature = "blocking")]
bindy_macro::bindy_uniffi_blocking!("bindings.json");

#[uniffi::export]
impl Clvm {
    /// Allocates a value as a CLVM program.
    pub fn alloc(&self, value: ClvmType) -> Result<Arc<Program>, ChiaError> {
        let clvm = <Self as bindy::UniffiObject>::read(self).clone();
        let program = alloc(&clvm, value)?;
        Ok(FromRust::from_rust(program, &UniffiContext)?)
    }
}

fn alloc(
    clvm: &chia_sdk_bindings::Clvm,
    value: ClvmType,
) -> bindy::Result<chia_sdk_bindings::Program> {
    let value = match value.into_value() {
        Ok(value) => value,
        Err(ClvmType::Nil) => return clvm.nil(),
        Err(ClvmType::Bool { value }) => return clvm.bool(value),
        Err(ClvmType::Int { value }) => return clvm.int(value.parse()?),
        Err(ClvmType::Atom { value }) => return clvm.atom(value.into()),
        Err(ClvmType::Text { value }) => return clvm.string(value),
        Err(_) => unreachable!("all class variants are converted by `into_value`"),
    };

    Ok(match value {
        ClvmValue::Program(value) => value,
        ClvmValue::Pair(value) => clvm.pair(value.first, value.rest)?,
        ClvmValue::CurriedProgram(value) => value.program.curry(value.args.clone())?,
        ClvmValue::PublicKey(value) => clvm.atom(value.to_bytes().to_vec().into())?,
        ClvmValue::Signature(value) => clvm.atom(value.to_bytes().to_vec().into())?,
        ClvmValue::K1PublicKey(value) => clvm.atom(value.to_bytes()?.into())?,
        ClvmValue::K1Signature(value) => clvm.atom(value.to_bytes()?.into())?,
        ClvmValue::R1PublicKey(value) => clvm.atom(value.to_bytes()?.into())?,
        ClvmValue::R1Signature(value) => clvm.atom(value.to_bytes()?.into())?,
        ClvmValue::Remark(value) => clvm.remark(value.rest)?,
        ClvmValue::AggSigParent(value) => clvm.agg_sig_parent(value.public_key, value.message)?,
        ClvmValue::AggSigPuzzle(value) => clvm.agg_sig_puzzle(value.public_key, value.message)?,
        ClvmValue::AggSigAmount(value) => clvm.agg_sig_amount(value.public_key, value.message)?,
        ClvmValue::AggSigPuzzleAmount(value) => {
            clvm.agg_sig_puzzle_amount(value.public_key, value.message)?
        }
        ClvmValue::AggSigParentAmount(value) => {
            clvm.agg_sig_parent_amount(value.public_key, value.message)?
        }
        ClvmValue::AggSigParentPuzzle(value) => {
            clvm.agg_sig_parent_puzzle(value.public_key, value.message)?
        }
        ClvmValue::AggSigUnsafe(value) => clvm.agg_sig_unsafe(value.public_key, value.message)?,
        ClvmValue::AggSigMe(value) => clvm.agg_sig_me(value.public_key, value.message)?,
        ClvmValue::CreateCoin(value) => {
            clvm.create_coin(value.puzzle_hash, value.amount, value.memos)?
        }
        ClvmValue::ReserveFee(value) => clvm.reserve_fee(value.amount)?,
        ClvmValue::CreateCoinAnnouncement(value) => clvm.create_coin_announcement(value.message)?,
        ClvmValue::CreatePuzzleAnnouncement(value) => {
            clvm.create_puzzle_announcement(value.message)?
        }
        ClvmValue::AssertCoinAnnouncement(value) => {
            clvm.assert_coin_announcement(value.announcement_id)?
        }
        ClvmValue::AssertPuzzleAnnouncement(value) => {
            clvm.assert_puzzle_announcement(value.announcement_id)?
        }
        ClvmValue::AssertConcurrentSpend(value) => clvm.assert_concurrent_spend(value.coin_id)?,
        ClvmValue::AssertConcurrentPuzzle(value) => {
            clvm.assert_concurrent_puzzle(value.puzzle_hash)?
        }
        ClvmValue::AssertSecondsRelative(value) => clvm.assert_seconds_relative(value.seconds)?,
        ClvmValue::AssertSecondsAbsolute(value) => clvm.assert_seconds_absolute(value.seconds)?,
        ClvmValue::AssertHeightRelative(value) => clvm.assert_height_relative(value.height)?,
        ClvmValue::AssertHeightAbsolute(value) => clvm.assert_height_absolute(value.height)?,
        ClvmValue::AssertBeforeSecondsRelative(value) => {
            clvm.assert_before_seconds_relative(value.seconds)?
        }
        ClvmValue::AssertBeforeSecondsAbsolute(value) => {
            clvm.assert_before_seconds_absolute(value.seconds)?
        }
        ClvmValue::AssertBeforeHeightRelative(value) => {
            clvm.assert_before_height_relative(value.height)?
        }
        ClvmValue::AssertBeforeHeightAbsolute(value) => {
            clvm.assert_before_height_absolute(value.height)?
        }
        ClvmValue::AssertMyCoinId(value) => clvm.assert_my_coin_id(value.coin_id)?,
        ClvmValue::AssertMyParentId(value) => clvm.assert_my_parent_id(value.parent_id)?,
        ClvmValue::AssertMyPuzzleHash(value) => clvm.assert_my_puzzle_hash(value.puzzle_hash)?,
        ClvmValue::AssertMyAmount(value) => clvm.assert_my_amount(value.amount)?,
        ClvmValue::AssertMyBirthSeconds(value) => clvm.assert_my_birth_seconds(value.seconds)?,
        ClvmValue::AssertMyBirthHeight(value) => clvm.assert_my_birth_height(value.height)?,
        ClvmValue::AssertEphemeral(_value) => clvm.assert_ephemeral()?,
        ClvmValue::SendMessage(value) => {
            clvm.send_message(value.mode, value.message, value.data)?
        }
        ClvmValue::ReceiveMessage(value) => {
            clvm.receive_message(value.mode, value.message, value.data)?
        }
        ClvmValue::Softfork(value) => clvm.softfork(value.cost, value.rest)?,
        ClvmValue::MeltSingleton(_value) => clvm.melt_singleton()?,
        ClvmValue::TransferNft(value) => clvm.transfer_nft(
            value.launcher_id,
            value.trade_prices.clone(),
            value.singleton_inner_puzzle_hash,
        )?,
        ClvmValue::RunCatTail(value) => {
            clvm.run_cat_tail(value.program.clone(), value.solution.clone())?
        }
        ClvmValue::UpdateNftMetadata(value) => clvm.update_nft_metadata(
            value.updater_puzzle_reveal.clone(),
            value.updater_solution.clone(),
        )?,
        ClvmValue::UpdateDatastoreMerkleRoot(value) => {
            clvm.update_datastore_merkle_root(value.new_merkle_root, value.memos.clone())?
        }
        ClvmValue::NftMetadata(value) => clvm.nft_metadata(value.clone())?,
        ClvmValue::HandleNftMetadata(value) => clvm.handle_nft_metadata(value.clone())?,
        ClvmValue::MipsMemo(value) => clvm.mips_memo(value.clone())?,
        ClvmValue::InnerPuzzleMemo(value) => clvm.inner_puzzle_memo(value.clone())?,
        ClvmValue::RestrictionMemo(value) => clvm.restriction_memo(value.clone())?,
        ClvmValue::WrapperMemo(value) => clvm.wrapper_memo(value.clone())?,
        ClvmValue::Force1of2RestrictedVariableMemo(value) => {
            clvm.force_1_of_2_restricted_variable_memo(value.clone())?
        }
        ClvmValue::MemoKind(value) => clvm.memo_kind(value.clone())?,
        ClvmValue::MemberMemo(value) => clvm.member_memo(value.clone())?,
        ClvmValue::MofNMemo(value) => clvm.m_of_n_memo(value.clone())?,
        ClvmValue::OptionMetadata(value) => clvm.option_metadata(value)?,
        ClvmValue::NotarizedPayment(value) => clvm.notarized_payment(value.clone())?,
        ClvmValue::Payment(value) => clvm.payment(value.clone())?,
    })
}
