#![allow(clippy::too_many_arguments)]
// UniFFI's setup_scaffolding! macro emits raw FFI extern blocks which require unsafe.
#![allow(unsafe_code)]

use std::sync::Arc;

// UniFFI scaffolding — must match the [lib] name in Cargo.toml
uniffi::setup_scaffolding!("chia_wallet_sdk");

// Generate all bindings from the JSON schemas
bindy_macro::bindy_uniffi!("bindings.json");

// Hand-written alloc method for Clvm.
// The JSON schema marks alloc as stub_only because the argument type (ClvmType)
// requires manual dispatch. Here we implement it using the macro-generated ClvmType enum.
#[uniffi::export]
impl Clvm {
    // Unlike the Python backend which accepts dynamic types (None, int, bool, str, bytes, list, tuple),
    // the UniFFI backend uses a statically-typed ClvmType enum. To allocate nil/int/bool/string/bytes
    // use the Clvm helper methods directly: nil(), int(), bool_(), string(), atom(), pair(), list().
    pub fn alloc(&self, value: ClvmType) -> Result<Arc<Program>, ChiaError> {
        let result = match value {
            ClvmType::Program { value } => value.0.clone(),
            ClvmType::Pair { value } => self.0.pair(value.0.first.clone(), value.0.rest.clone())?,
            ClvmType::CurriedProgram { value } => {
                value.0.program.clone().curry(value.0.args.clone())?
            }
            ClvmType::PublicKey { value } => {
                let bytes = value.0.to_bytes();
                self.0.atom(bytes.to_vec().into())?
            }
            ClvmType::Signature { value } => {
                let bytes = value.0.to_bytes();
                self.0.atom(bytes.to_vec().into())?
            }
            ClvmType::K1PublicKey { value } => {
                let bytes = value.0.to_bytes()?;
                self.0.atom(bytes.into())?
            }
            ClvmType::K1Signature { value } => {
                let bytes = value.0.to_bytes()?;
                self.0.atom(bytes.into())?
            }
            ClvmType::R1PublicKey { value } => {
                let bytes = value.0.to_bytes()?;
                self.0.atom(bytes.into())?
            }
            ClvmType::R1Signature { value } => {
                let bytes = value.0.to_bytes()?;
                self.0.atom(bytes.into())?
            }
            ClvmType::Remark { value } => self.0.remark(value.0.rest.clone())?,
            ClvmType::AggSigParent { value } => self
                .0
                .agg_sig_parent(value.0.public_key, value.0.message.clone())?,
            ClvmType::AggSigPuzzle { value } => self
                .0
                .agg_sig_puzzle(value.0.public_key, value.0.message.clone())?,
            ClvmType::AggSigAmount { value } => self
                .0
                .agg_sig_amount(value.0.public_key, value.0.message.clone())?,
            ClvmType::AggSigPuzzleAmount { value } => self
                .0
                .agg_sig_puzzle_amount(value.0.public_key, value.0.message.clone())?,
            ClvmType::AggSigParentAmount { value } => self
                .0
                .agg_sig_parent_amount(value.0.public_key, value.0.message.clone())?,
            ClvmType::AggSigParentPuzzle { value } => self
                .0
                .agg_sig_parent_puzzle(value.0.public_key, value.0.message.clone())?,
            ClvmType::AggSigUnsafe { value } => self
                .0
                .agg_sig_unsafe(value.0.public_key, value.0.message.clone())?,
            ClvmType::AggSigMe { value } => self
                .0
                .agg_sig_me(value.0.public_key, value.0.message.clone())?,
            ClvmType::CreateCoin { value } => {
                self.0
                    .create_coin(value.0.puzzle_hash, value.0.amount, value.0.memos.clone())?
            }
            ClvmType::ReserveFee { value } => self.0.reserve_fee(value.0.amount)?,
            ClvmType::CreateCoinAnnouncement { value } => {
                self.0.create_coin_announcement(value.0.message.clone())?
            }
            ClvmType::CreatePuzzleAnnouncement { value } => {
                self.0.create_puzzle_announcement(value.0.message.clone())?
            }
            ClvmType::AssertCoinAnnouncement { value } => {
                self.0.assert_coin_announcement(value.0.announcement_id)?
            }
            ClvmType::AssertPuzzleAnnouncement { value } => {
                self.0.assert_puzzle_announcement(value.0.announcement_id)?
            }
            ClvmType::AssertConcurrentSpend { value } => {
                self.0.assert_concurrent_spend(value.0.coin_id)?
            }
            ClvmType::AssertConcurrentPuzzle { value } => {
                self.0.assert_concurrent_puzzle(value.0.puzzle_hash)?
            }
            ClvmType::AssertSecondsRelative { value } => {
                self.0.assert_seconds_relative(value.0.seconds)?
            }
            ClvmType::AssertSecondsAbsolute { value } => {
                self.0.assert_seconds_absolute(value.0.seconds)?
            }
            ClvmType::AssertHeightRelative { value } => {
                self.0.assert_height_relative(value.0.height)?
            }
            ClvmType::AssertHeightAbsolute { value } => {
                self.0.assert_height_absolute(value.0.height)?
            }
            ClvmType::AssertBeforeSecondsRelative { value } => {
                self.0.assert_before_seconds_relative(value.0.seconds)?
            }
            ClvmType::AssertBeforeSecondsAbsolute { value } => {
                self.0.assert_before_seconds_absolute(value.0.seconds)?
            }
            ClvmType::AssertBeforeHeightRelative { value } => {
                self.0.assert_before_height_relative(value.0.height)?
            }
            ClvmType::AssertBeforeHeightAbsolute { value } => {
                self.0.assert_before_height_absolute(value.0.height)?
            }
            ClvmType::AssertMyCoinId { value } => self.0.assert_my_coin_id(value.0.coin_id)?,
            ClvmType::AssertMyParentId { value } => {
                self.0.assert_my_parent_id(value.0.parent_id)?
            }
            ClvmType::AssertMyPuzzleHash { value } => {
                self.0.assert_my_puzzle_hash(value.0.puzzle_hash)?
            }
            ClvmType::AssertMyAmount { value } => self.0.assert_my_amount(value.0.amount)?,
            ClvmType::AssertMyBirthSeconds { value } => {
                self.0.assert_my_birth_seconds(value.0.seconds)?
            }
            ClvmType::AssertMyBirthHeight { value } => {
                self.0.assert_my_birth_height(value.0.height)?
            }
            ClvmType::AssertEphemeral { .. } => self.0.assert_ephemeral()?,
            ClvmType::SendMessage { value } => {
                self.0
                    .send_message(value.0.mode, value.0.message.clone(), value.0.data.clone())?
            }
            ClvmType::ReceiveMessage { value } => self.0.receive_message(
                value.0.mode,
                value.0.message.clone(),
                value.0.data.clone(),
            )?,
            ClvmType::Softfork { value } => self.0.softfork(value.0.cost, value.0.rest.clone())?,
            ClvmType::MeltSingleton { .. } => self.0.melt_singleton()?,
            ClvmType::TransferNft { value } => self.0.transfer_nft(
                value.0.launcher_id,
                value.0.trade_prices.clone(),
                value.0.singleton_inner_puzzle_hash,
            )?,
            ClvmType::RunCatTail { value } => self
                .0
                .run_cat_tail(value.0.program.clone(), value.0.solution.clone())?,
            ClvmType::UpdateNftMetadata { value } => self.0.update_nft_metadata(
                value.0.updater_puzzle_reveal.clone(),
                value.0.updater_solution.clone(),
            )?,
            ClvmType::UpdateDataStoreMerkleRoot { value } => self
                .0
                .update_data_store_merkle_root(value.0.new_merkle_root, value.0.memos.clone())?,
            ClvmType::NftMetadata { value } => self.0.nft_metadata(value.0.clone())?,
            ClvmType::MipsMemo { value } => self.0.mips_memo(value.0.clone())?,
            ClvmType::InnerPuzzleMemo { value } => self.0.inner_puzzle_memo(value.0.clone())?,
            ClvmType::RestrictionMemo { value } => self.0.restriction_memo(value.0.clone())?,
            ClvmType::WrapperMemo { value } => self.0.wrapper_memo(value.0.clone())?,
            ClvmType::Force1of2RestrictedVariableMemo { value } => self
                .0
                .force_1_of_2_restricted_variable_memo(value.0.clone())?,
            ClvmType::MemoKind { value } => self.0.memo_kind(value.0.clone())?,
            ClvmType::MemberMemo { value } => self.0.member_memo(value.0.clone())?,
            ClvmType::MofNMemo { value } => self.0.m_of_n_memo(value.0.clone())?,
            ClvmType::OptionMetadata { value } => self.0.option_metadata(value.0)?,
            ClvmType::NotarizedPayment { value } => self.0.notarized_payment(value.0.clone())?,
            ClvmType::Payment { value } => self.0.payment(value.0.clone())?,
        };
        Ok(Arc::new(Program(result)))
    }
}
