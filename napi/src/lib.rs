#![allow(clippy::wildcard_imports)]
#![allow(clippy::too_many_arguments)]

use bindy::{FromRust, IntoRust, NapiParamContext, NapiReturnContext};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
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
            ClvmType::OptionMetadata(value) => clvm.option_metadata(value.0)?,
            ClvmType::NotarizedPayment(value) => clvm.notarized_payment(value.0.clone())?,
            ClvmType::Payment(value) => clvm.payment(value.0.clone())?,
        }),
    }
}

#[napi(object)]
#[derive(Debug)]
pub struct FullNodeSimulatorEventPayload {
    pub r#type: String,
    pub height: Option<u32>,
    pub header_hash: Option<String>,
    pub previous_header_hash: Option<String>,
    pub additions: Vec<String>,
    pub removals: Vec<String>,
    pub fork_height: Option<u32>,
    pub old_peak_hash: Option<String>,
    pub new_peak_hash: Option<String>,
    pub reverted_header_hashes: Vec<String>,
    pub new_header_hashes: Vec<String>,
}

#[napi]
pub struct FullNodeSimulator {
    inner: chia_sdk_bindings::FullNodeSimulator,
    event_callback: Option<ThreadsafeFunction<FullNodeSimulatorEventPayload>>,
}

#[napi]
#[derive(Debug)]
pub struct FullNodeSimulatorServer {
    inner: chia_sdk_bindings::FullNodeSimulatorServer,
}

impl std::fmt::Debug for FullNodeSimulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FullNodeSimulator").finish_non_exhaustive()
    }
}

#[napi]
impl FullNodeSimulator {
    #[napi(constructor)]
    pub fn new(secret_key: Option<ClassInstance<'_, SecretKey>>) -> Result<Self> {
        let inner = match secret_key {
            Some(secret_key) => chia_sdk_bindings::FullNodeSimulator::with_secret_key(
                secret_key.into_rust(&NapiParamContext)?,
            )?,
            None => chia_sdk_bindings::FullNodeSimulator::new()?,
        };
        Ok(Self {
            inner,
            event_callback: None,
        })
    }

    #[napi(factory)]
    pub fn with_seed(seed: BigInt) -> Result<Self> {
        Ok(Self {
            inner: chia_sdk_bindings::FullNodeSimulator::with_seed(
                seed.into_rust(&NapiParamContext)?,
            )?,
            event_callback: None,
        })
    }

    #[napi(factory)]
    pub fn with_secret_key(secret_key: ClassInstance<'_, SecretKey>) -> Result<Self> {
        Ok(Self {
            inner: chia_sdk_bindings::FullNodeSimulator::with_secret_key(
                secret_key.into_rust(&NapiParamContext)?,
            )?,
            event_callback: None,
        })
    }

    #[napi]
    pub fn on_event(
        &mut self,
        callback: ThreadsafeFunction<FullNodeSimulatorEventPayload>,
    ) -> Result<()> {
        self.event_callback = Some(callback);
        Ok(())
    }

    #[napi]
    pub fn drain_events(&self) -> Result<Vec<FullNodeSimulatorEventPayload>> {
        Ok(self
            .inner
            .drain_events()?
            .into_iter()
            .map(event_payload)
            .collect())
    }

    #[napi]
    pub async fn start_server(&self) -> Result<FullNodeSimulatorServer> {
        Ok(FullNodeSimulatorServer {
            inner: self.inner.start_server().await?,
        })
    }

    #[napi]
    pub fn height(&self) -> Result<u32> {
        Ok(self.inner.height()?)
    }

    #[napi]
    pub fn header_hash(&self, env: Env) -> Result<Buffer> {
        Ok(FromRust::from_rust(
            self.inner.header_hash()?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn header_hash_of(&self, env: Env, height: u32) -> Result<Option<Buffer>> {
        Ok(FromRust::from_rust(
            self.inner.header_hash_of(height)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn insert_coin(&mut self, coin: ClassInstance<'_, Coin>) -> Result<()> {
        self.inner.insert_coin(coin.into_rust(&NapiParamContext)?)?;
        self.emit_events();
        Ok(())
    }

    #[napi]
    pub fn new_coin(&mut self, env: Env, puzzle_hash: Uint8Array, amount: BigInt) -> Result<Coin> {
        let coin = self.inner.new_coin(
            puzzle_hash.into_rust(&NapiParamContext)?,
            amount.into_rust(&NapiParamContext)?,
        )?;
        self.emit_events();
        Ok(FromRust::from_rust(coin, &NapiReturnContext(env))?)
    }

    #[napi]
    pub fn get_farming_ph(&self, env: Env) -> Result<Buffer> {
        Ok(FromRust::from_rust(
            self.inner.get_farming_ph()?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_master_secret_key(&self, env: Env) -> Result<SecretKey> {
        Ok(FromRust::from_rust(
            self.inner.get_master_secret_key()?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_prefarm_puzzle_hash(&self, env: Env) -> Result<Buffer> {
        Ok(FromRust::from_rust(
            self.inner.get_prefarm_puzzle_hash()?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn set_farming_ph(&mut self, puzzle_hash: Uint8Array) -> Result<()> {
        self.inner
            .set_farming_ph(puzzle_hash.into_rust(&NapiParamContext)?)?;
        Ok(())
    }

    #[napi]
    pub fn get_blockchain_state(&self, env: Env) -> Result<BlockchainStateResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_blockchain_state()?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_network_info(&self, env: Env) -> Result<GetNetworkInfoResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_network_info()?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_aggsig_additional_data(&self, env: Env) -> Result<Buffer> {
        Ok(FromRust::from_rust(
            self.inner.get_aggsig_additional_data()?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_block_record(
        &self,
        env: Env,
        header_hash: Uint8Array,
    ) -> Result<GetBlockRecordResponse> {
        Ok(FromRust::from_rust(
            self.inner
                .get_block_record(header_hash.into_rust(&NapiParamContext)?)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_block_record_by_height(
        &self,
        env: Env,
        height: u32,
    ) -> Result<GetBlockRecordResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_block_record_by_height(height)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_block_records(
        &self,
        env: Env,
        start: u32,
        end: u32,
    ) -> Result<GetBlockRecordsResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_block_records(start, end)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_additions_and_removals(
        &self,
        env: Env,
        header_hash: Uint8Array,
    ) -> Result<AdditionsAndRemovalsResponse> {
        Ok(FromRust::from_rust(
            self.inner
                .get_additions_and_removals(header_hash.into_rust(&NapiParamContext)?)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_block_spends(
        &self,
        env: Env,
        header_hash: Uint8Array,
    ) -> Result<GetBlockSpendsResponse> {
        Ok(FromRust::from_rust(
            self.inner
                .get_block_spends(header_hash.into_rust(&NapiParamContext)?)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_coin_record_by_name(
        &self,
        env: Env,
        name: Uint8Array,
    ) -> Result<GetCoinRecordResponse> {
        Ok(FromRust::from_rust(
            self.inner
                .get_coin_record_by_name(name.into_rust(&NapiParamContext)?)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_coin_records_by_names(
        &self,
        env: Env,
        names: Vec<Uint8Array>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_coin_records_by_names(
                names.into_rust(&NapiParamContext)?,
                start_height,
                end_height,
                include_spent_coins,
            )?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_coin_records_by_hint(
        &self,
        env: Env,
        hint: Uint8Array,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_coin_records_by_hint(
                hint.into_rust(&NapiParamContext)?,
                start_height,
                end_height,
                include_spent_coins,
            )?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_coin_records_by_hints(
        &self,
        env: Env,
        hints: Vec<Uint8Array>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_coin_records_by_hints(
                hints.into_rust(&NapiParamContext)?,
                start_height,
                end_height,
                include_spent_coins,
            )?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_coin_records_by_parent_ids(
        &self,
        env: Env,
        parent_ids: Vec<Uint8Array>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_coin_records_by_parent_ids(
                parent_ids.into_rust(&NapiParamContext)?,
                start_height,
                end_height,
                include_spent_coins,
            )?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_coin_records_by_puzzle_hash(
        &self,
        env: Env,
        puzzle_hash: Uint8Array,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_coin_records_by_puzzle_hash(
                puzzle_hash.into_rust(&NapiParamContext)?,
                start_height,
                end_height,
                include_spent_coins,
            )?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_coin_records_by_puzzle_hashes(
        &self,
        env: Env,
        puzzle_hashes: Vec<Uint8Array>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> Result<GetCoinRecordsResponse> {
        Ok(FromRust::from_rust(
            self.inner.get_coin_records_by_puzzle_hashes(
                puzzle_hashes.into_rust(&NapiParamContext)?,
                start_height,
                end_height,
                include_spent_coins,
            )?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_puzzle_and_solution(
        &self,
        env: Env,
        coin_id: Uint8Array,
        height: Option<u32>,
    ) -> Result<GetPuzzleAndSolutionResponse> {
        Ok(FromRust::from_rust(
            self.inner
                .get_puzzle_and_solution(coin_id.into_rust(&NapiParamContext)?, height)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn push_tx(
        &mut self,
        env: Env,
        spend_bundle: ClassInstance<'_, SpendBundle>,
    ) -> Result<PushTxResponse> {
        let response = self
            .inner
            .push_tx(spend_bundle.into_rust(&NapiParamContext)?)?;
        self.emit_events();
        Ok(FromRust::from_rust(response, &NapiReturnContext(env))?)
    }

    #[napi]
    pub fn get_mempool_item_by_tx_id(
        &self,
        env: Env,
        tx_id: Uint8Array,
    ) -> Result<GetMempoolItemResponse> {
        Ok(FromRust::from_rust(
            self.inner
                .get_mempool_item_by_tx_id(tx_id.into_rust(&NapiParamContext)?)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn get_mempool_items_by_coin_name(
        &self,
        env: Env,
        coin_name: Uint8Array,
    ) -> Result<GetMempoolItemsResponse> {
        Ok(FromRust::from_rust(
            self.inner
                .get_mempool_items_by_coin_name(coin_name.into_rust(&NapiParamContext)?)?,
            &NapiReturnContext(env),
        )?)
    }

    #[napi]
    pub fn farm_block(&mut self, env: Env, blocks: u32) -> Result<Vec<BlockRecord>> {
        let records = self.inner.farm_block(blocks)?;
        self.emit_events();
        Ok(FromRust::from_rust(records, &NapiReturnContext(env))?)
    }

    #[napi]
    pub fn revert_blocks(&mut self, env: Env, blocks: u32) -> Result<Vec<Buffer>> {
        let reverted = self.inner.revert_blocks(blocks)?;
        self.emit_events();
        Ok(FromRust::from_rust(reverted, &NapiReturnContext(env))?)
    }

    #[napi]
    pub fn reorg_blocks(
        &mut self,
        env: Env,
        num_of_blocks_to_rev: u32,
        num_of_new_blocks: u32,
    ) -> Result<Vec<BlockRecord>> {
        let records = self
            .inner
            .reorg_blocks(num_of_blocks_to_rev, num_of_new_blocks)?;
        self.emit_events();
        Ok(FromRust::from_rust(records, &NapiReturnContext(env))?)
    }
}

#[napi]
impl FullNodeSimulatorServer {
    #[napi(getter)]
    pub fn url(&self) -> Result<String> {
        Ok(self.inner.url()?)
    }

    #[napi]
    pub fn close(&mut self) -> Result<()> {
        Ok(self.inner.close()?)
    }
}

impl FullNodeSimulator {
    fn emit_events(&mut self) {
        let Some(callback) = &self.event_callback else {
            return;
        };
        let Ok(events) = self.inner.drain_events() else {
            return;
        };
        for event in events {
            callback.call(
                Ok(event_payload(event)),
                ThreadsafeFunctionCallMode::NonBlocking,
            );
        }
    }
}

fn event_payload(
    event: chia_sdk_bindings::FullNodeSimulatorEvent,
) -> FullNodeSimulatorEventPayload {
    match event {
        chia_sdk_bindings::FullNodeSimulatorEvent::Block {
            height,
            header_hash,
            previous_header_hash,
            additions,
            removals,
        } => FullNodeSimulatorEventPayload {
            r#type: "block".to_string(),
            height: Some(height),
            header_hash: Some(hex32(header_hash)),
            previous_header_hash: Some(hex32(previous_header_hash)),
            additions: additions
                .into_iter()
                .map(|record| hex32(record.coin.coin_id()))
                .collect(),
            removals: removals
                .into_iter()
                .map(|record| hex32(record.coin.coin_id()))
                .collect(),
            fork_height: None,
            old_peak_hash: None,
            new_peak_hash: None,
            reverted_header_hashes: Vec::new(),
            new_header_hashes: Vec::new(),
        },
        chia_sdk_bindings::FullNodeSimulatorEvent::Reorg {
            fork_height,
            old_peak_hash,
            new_peak_hash,
            reverted_header_hashes,
            new_header_hashes,
        } => FullNodeSimulatorEventPayload {
            r#type: "reorg".to_string(),
            height: None,
            header_hash: None,
            previous_header_hash: None,
            additions: Vec::new(),
            removals: Vec::new(),
            fork_height: Some(fork_height),
            old_peak_hash: Some(hex32(old_peak_hash)),
            new_peak_hash: Some(hex32(new_peak_hash)),
            reverted_header_hashes: reverted_header_hashes.into_iter().map(hex32).collect(),
            new_header_hashes: new_header_hashes.into_iter().map(hex32).collect(),
        },
    }
}

fn hex32(bytes: chia_sdk_bindings::Bytes32) -> String {
    format!("0x{}", hex::encode(bytes.to_bytes()))
}
