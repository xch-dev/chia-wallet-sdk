use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_bls::{PublicKey, Signature};
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, Program as SerializedProgram, SpendBundle};
use chia_puzzle_types::{offer::SettlementPaymentsSolution, LineageProof};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_driver::{
    create_security_coin, launch_reward_distributor, spend_security_coin, spend_settlement_nft,
    Cat, HashedPtr, Launcher, Layer, MedievalVault as SdkMedievalVault, MedievalVaultInfo, Offer,
    OptionMetadata, RewardDistributor as SdkRewardDistributor, RewardDistributorConstants,
    RewardDistributorState, SettlementLayer, SpendContext, StandardLayer, StreamedAsset,
};
use chia_sdk_types::{Condition, Conditions, MAINNET_CONSTANTS, TESTNET11_CONSTANTS};
use clvm_tools_rs::classic::clvm_tools::binutils::assemble;
use clvm_traits::{clvm_quote, ToClvm};
use clvm_utils::TreeHash;
use clvmr::{
    serde::{node_from_bytes, node_from_bytes_backrefs},
    NodePtr,
};
use num_bigint::BigInt;

use crate::{
    AsProgram, AsPtr, CatSpend, CreatedDid, Did, Force1of2RestrictedVariableMemo, InnerPuzzleMemo,
    MedievalVault, MemberMemo, MemoKind, MintedNfts, MipsMemo, MipsSpend, MofNMemo, Nft,
    NftMetadata, NftMint, NotarizedPayment, OfferSecurityCoinDetails, OptionContract, Payment,
    Program, RestrictionMemo, RewardDistributor, RewardDistributorInfoFromEveCoin,
    RewardDistributorLaunchResult, RewardSlot, SettlementNftSpendResult, Spend,
    StreamedAssetParsingResult, VaultMint, WrapperMemo,
};

pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
pub const MIN_SAFE_INTEGER: f64 = -MAX_SAFE_INTEGER;

// This is sort of an implementation detail of the CLVM runtime.
pub const MAX_CLVM_SMALL_INTEGER: i64 = 67_108_863;

// We use an Arc because we need to be able to share the SpendContext with the Program class
// And we use a Mutex because we need to retain mutability even while Program instances exist
#[derive(Default, Clone)]
pub struct Clvm(pub(crate) Arc<Mutex<SpendContext>>);

impl Clvm {
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn add_coin_spend(&self, coin_spend: CoinSpend) -> Result<()> {
        self.0.lock().unwrap().insert(coin_spend);
        Ok(())
    }

    pub fn spend_coin(&self, coin: Coin, spend: Spend) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        let puzzle_reveal = ctx.serialize(&spend.puzzle.1)?;
        let solution = ctx.serialize(&spend.solution.1)?;
        ctx.insert(chia_protocol::CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    pub fn coin_spends(&self) -> Result<Vec<CoinSpend>> {
        Ok(self.0.lock().unwrap().take())
    }

    pub fn delegated_spend(&self, conditions: Vec<Program>) -> Result<Spend> {
        let delegated_puzzle = self.0.lock().unwrap().alloc(&clvm_quote!(conditions
            .into_iter()
            .map(|p| p.1)
            .collect::<Vec<_>>()))?;
        Ok(Spend {
            puzzle: Program(self.0.clone(), delegated_puzzle),
            solution: Program(self.0.clone(), NodePtr::NIL),
        })
    }

    pub fn standard_spend(&self, synthetic_key: PublicKey, spend: Spend) -> Result<Spend> {
        let mut ctx = self.0.lock().unwrap();
        let spend =
            StandardLayer::new(synthetic_key).delegated_inner_spend(&mut ctx, spend.into())?;
        Ok(Spend {
            puzzle: Program(self.0.clone(), spend.puzzle),
            solution: Program(self.0.clone(), spend.solution),
        })
    }

    pub fn spend_standard_coin(
        &self,
        coin: Coin,
        synthetic_key: PublicKey,
        spend: Spend,
    ) -> Result<()> {
        let spend = self.standard_spend(synthetic_key, spend)?;
        let mut ctx = self.0.lock().unwrap();
        let puzzle_reveal = ctx.serialize(&spend.puzzle.1)?;
        let solution = ctx.serialize(&spend.solution.1)?;
        ctx.insert(chia_protocol::CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    pub fn settlement_spend(&self, notarized_payments: Vec<NotarizedPayment>) -> Result<Spend> {
        let mut ctx = self.0.lock().unwrap();

        let notarized_payments = notarized_payments
            .into_iter()
            .map(|p| p.as_ptr(&ctx))
            .collect::<Vec<_>>();

        let spend = SettlementLayer.construct_spend(
            &mut ctx,
            SettlementPaymentsSolution::new(notarized_payments),
        )?;

        Ok(Spend {
            puzzle: Program(self.0.clone(), spend.puzzle),
            solution: Program(self.0.clone(), spend.solution),
        })
    }

    pub fn spend_settlement_coin(
        &self,
        coin: Coin,
        notarized_payments: Vec<NotarizedPayment>,
    ) -> Result<()> {
        let spend = self.settlement_spend(notarized_payments)?;
        let mut ctx = self.0.lock().unwrap();
        let puzzle_reveal = ctx.serialize(&spend.puzzle.1)?;
        let solution = ctx.serialize(&spend.solution.1)?;
        ctx.insert(chia_protocol::CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    pub fn spend_cats(&self, cat_spends: Vec<CatSpend>) -> Result<Vec<Cat>> {
        let mut ctx = self.0.lock().unwrap();

        Ok(Cat::spend_all(
            &mut ctx,
            &cat_spends.into_iter().map(Into::into).collect::<Vec<_>>(),
        )?)
    }

    pub fn mint_nfts(
        &self,
        parent_coin_id: Bytes32,
        nft_mints: Vec<NftMint>,
    ) -> Result<MintedNfts> {
        let mut ctx = self.0.lock().unwrap();
        let mut nfts = Vec::new();
        let mut parent_conditions = Vec::new();

        for (i, nft_mint) in nft_mints.into_iter().enumerate() {
            let nft_mint = nft_mint.as_ptr(&ctx);

            let (conditions, nft) = Launcher::new(parent_coin_id, i as u64 * 2)
                .with_singleton_amount(1)
                .mint_nft(&mut ctx, &nft_mint)?;

            nfts.push(nft.as_program(&self.0));

            for condition in conditions {
                let condition = condition.to_clvm(&mut ctx)?;
                parent_conditions.push(Program(self.0.clone(), condition));
            }
        }

        Ok(MintedNfts {
            nfts,
            parent_conditions,
        })
    }

    pub fn spend_nft(&self, nft: Nft, inner_spend: Spend) -> Result<Nft> {
        let mut ctx = self.0.lock().unwrap();

        Ok(nft
            .as_ptr(&ctx)
            .spend(
                &mut ctx,
                chia_sdk_driver::Spend::new(inner_spend.puzzle.1, inner_spend.solution.1),
            )?
            .as_program(&self.0))
    }

    pub fn create_eve_did(
        &self,
        parent_coin_id: Bytes32,
        p2_puzzle_hash: Bytes32,
    ) -> Result<CreatedDid> {
        let mut ctx = self.0.lock().unwrap();

        let (conditions, did) = Launcher::new(parent_coin_id, 1).create_eve_did(
            &mut ctx,
            p2_puzzle_hash,
            None,
            1,
            HashedPtr::NIL,
        )?;

        let mut parent_conditions = Vec::new();

        for condition in conditions {
            let condition = condition.to_clvm(&mut ctx)?;
            parent_conditions.push(Program(self.0.clone(), condition));
        }

        Ok(CreatedDid {
            did: did.as_program(&self.0),
            parent_conditions,
        })
    }

    pub fn spend_did(&self, did: Did, inner_spend: Spend) -> Result<Option<Did>> {
        let mut ctx = self.0.lock().unwrap();

        Ok(did
            .as_ptr(&ctx)
            .spend(
                &mut ctx,
                chia_sdk_driver::Spend::new(inner_spend.puzzle.1, inner_spend.solution.1),
            )?
            .map(|did| did.as_program(&self.0)))
    }

    pub fn spend_option(
        &self,
        option: OptionContract,
        inner_spend: Spend,
    ) -> Result<Option<OptionContract>> {
        let mut ctx = self.0.lock().unwrap();

        let option = chia_sdk_driver::OptionContract::from(option);

        Ok(option
            .spend(
                &mut ctx,
                chia_sdk_driver::Spend::new(inner_spend.puzzle.1, inner_spend.solution.1),
            )?
            .map(Into::into))
    }

    pub fn spend_streamed_asset(
        &self,
        streamed_asset: StreamedAsset,
        payment_time: u64,
        clawback: bool,
    ) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        streamed_asset.spend(&mut ctx, payment_time, clawback)?;
        Ok(())
    }

    pub fn mint_vault(
        &self,
        parent_coin_id: Bytes32,
        custody_hash: TreeHash,
        memos: Program,
    ) -> Result<VaultMint> {
        let mut ctx = self.0.lock().unwrap();

        let (parent_conditions, vault) =
            Launcher::new(parent_coin_id, 1).mint_vault(&mut ctx, custody_hash, memos.1)?;

        let parent_conditions = parent_conditions
            .into_iter()
            .map(|program| Ok(Program(self.0.clone(), program.to_clvm(&mut ctx)?)))
            .collect::<Result<Vec<_>>>()?;

        Ok(VaultMint {
            parent_conditions,
            vault: vault.into(),
        })
    }

    pub fn mips_spend(&self, coin: Coin, delegated_spend: Spend) -> Result<MipsSpend> {
        Ok(MipsSpend {
            clvm: self.0.clone(),
            spend: Arc::new(Mutex::new(chia_sdk_driver::MipsSpend::new(
                chia_sdk_driver::Spend::new(delegated_spend.puzzle.1, delegated_spend.solution.1),
            ))),
            coin,
        })
    }

    pub fn parse(&self, program: String) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = assemble(&mut ctx, &program)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn deserialize(&self, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = node_from_bytes(&mut ctx, &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn deserialize_with_backrefs(&self, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = node_from_bytes_backrefs(&mut ctx, &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn cache(&self, mod_hash: Bytes32, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.puzzle(mod_hash.into(), &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn pair(&self, first: Program, second: Program) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.new_pair(first.1, second.1)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn nil(&self) -> Result<Program> {
        Ok(Program(self.0.clone(), NodePtr::NIL))
    }

    pub fn int(&self, value: BigInt) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.lock().unwrap().new_number(value)?,
        ))
    }

    pub fn bound_checked_number(&self, value: f64) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();

        if value.is_infinite() {
            return Err(Error::Infinite);
        }

        if value.is_nan() {
            return Err(Error::NaN);
        }

        if value.fract() != 0.0 {
            return Err(Error::Fractional);
        }

        // If the value is larger, it can't be safely encoded as a JavaScript number.
        if value > MAX_SAFE_INTEGER {
            return Err(Error::TooLarge);
        }

        // If the value is smaller, it can't be safely encoded as a JavaScript number.
        if value < MIN_SAFE_INTEGER {
            return Err(Error::TooSmall);
        }

        #[allow(clippy::cast_possible_truncation)]
        let value = value as i64;

        if (0..=MAX_CLVM_SMALL_INTEGER).contains(&value) {
            Ok(Program(
                self.0.clone(),
                ctx.new_small_number(value.try_into().unwrap())?,
            ))
        } else {
            Ok(Program(self.0.clone(), ctx.new_number(value.into())?))
        }
    }

    pub fn string(&self, value: String) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.lock().unwrap().new_atom(value.as_bytes())?,
        ))
    }

    pub fn bool(&self, value: bool) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.lock().unwrap().new_small_number(value as u32)?,
        ))
    }

    pub fn atom(&self, value: Bytes) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.lock().unwrap().new_atom(&value)?,
        ))
    }

    pub fn list(&self, value: Vec<Program>) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let mut result = NodePtr::NIL;

        for item in value.into_iter().rev() {
            result = ctx.new_pair(item.1, result)?;
        }

        Ok(Program(self.0.clone(), result))
    }

    pub fn nft_metadata(&self, nft_metadata: NftMetadata) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&nft_metadata)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn mips_memo(&self, value: MipsMemo) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&chia_sdk_driver::MipsMemo::from(value))?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn inner_puzzle_memo(&self, value: InnerPuzzleMemo) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&chia_sdk_driver::InnerPuzzleMemo::from(value))?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn restriction_memo(&self, value: RestrictionMemo) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&chia_sdk_driver::RestrictionMemo::from(value))?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn wrapper_memo(&self, value: WrapperMemo) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&chia_sdk_driver::WrapperMemo::from(value))?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn force_1_of_2_restricted_variable_memo(
        &self,
        value: Force1of2RestrictedVariableMemo,
    ) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&chia_sdk_driver::Force1of2RestrictedVariableMemo::from(
            value,
        ))?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn memo_kind(&self, value: MemoKind) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&chia_sdk_driver::MemoKind::from(value))?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn member_memo(&self, value: MemberMemo) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&chia_sdk_driver::MemberMemo::from(value))?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn m_of_n_memo(&self, value: MofNMemo) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&chia_sdk_driver::MofNMemo::from(value))?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn option_metadata(&self, value: OptionMetadata) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.alloc(&value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn payment(&self, value: Payment) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = value.as_ptr(&ctx);
        let ptr = ctx.alloc(&ptr)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn notarized_payment(&self, value: NotarizedPayment) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = value.as_ptr(&ctx);
        let ptr = ctx.alloc(&ptr)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn parse_child_streamed_asset(
        &self,
        parent_spend: CoinSpend,
    ) -> Result<StreamedAssetParsingResult> {
        let mut ctx = self.0.lock().unwrap();

        let (streamed_asset, clawback, last_payment_amount) =
            chia_sdk_driver::StreamedAsset::from_parent_spend(&mut ctx, &parent_spend)?;

        Ok(StreamedAssetParsingResult {
            streamed_asset,
            last_spend_was_clawback: clawback,
            last_payment_amount_if_clawback: last_payment_amount,
        })
    }

    pub fn parse_child_medieval_vault(
        &self,
        parent_spend: CoinSpend,
    ) -> Result<Option<MedievalVault>> {
        let mut ctx = self.0.lock().unwrap();

        let result = if parent_spend.coin.puzzle_hash == SINGLETON_LAUNCHER_HASH.into() {
            SdkMedievalVault::from_launcher_spend(&mut ctx, &parent_spend)?
        } else {
            SdkMedievalVault::from_parent_spend(&mut ctx, &parent_spend)?
        };

        Ok(result.map(|sdk_vault| {
            MedievalVault::new(sdk_vault.coin, sdk_vault.proof.into(), sdk_vault.info)
        }))
    }

    pub fn spend_medieval_vault(
        &self,
        medieval_vault: MedievalVault,
        used_pubkeys: Vec<PublicKey>,
        conditions: Vec<Program>,
        genesis_challenge: Bytes32,
    ) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();

        let mut actual_conditions = Conditions::new();
        for condition in conditions {
            actual_conditions.push(ctx.extract::<Condition<NodePtr>>(condition.1)?);
        }

        Ok(medieval_vault.to_sdk().spend(
            &mut ctx,
            &used_pubkeys,
            actual_conditions,
            genesis_challenge,
        )?)
    }

    pub fn spend_medieval_vault_unsafe(
        &self,
        medieval_vault: MedievalVault,
        used_pubkeys: Vec<PublicKey>,
        delegated_spend: Spend,
    ) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();

        Ok(medieval_vault.to_sdk().spend_sunsafe(
            &mut ctx,
            &used_pubkeys,
            delegated_spend.puzzle.1,
            delegated_spend.solution.1,
        )?)
    }

    pub fn medieval_vault_rekey_delegated_puzzle(
        &self,
        launcher_id: Bytes32,
        new_m: usize,
        new_pubkeys: Vec<PublicKey>,
        coin_id: Bytes32,
        genesis_challenge: Bytes32,
    ) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();

        Ok(Program(
            self.0.clone(),
            SdkMedievalVault::delegated_puzzle_for_rekey(
                &mut ctx,
                launcher_id,
                new_m,
                new_pubkeys,
                coin_id,
                genesis_challenge,
            )?,
        ))
    }

    pub fn medieval_vault_send_message_delegated_puzzle(
        &self,
        message: Bytes,
        receiver_launcher_id: Bytes32,
        my_coin: Coin,
        my_info: MedievalVaultInfo,
        genesis_challenge: Bytes32,
    ) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();

        Ok(Program(
            self.0.clone(),
            SdkMedievalVault::delegated_puzzle_for_flexible_send_message(
                &mut ctx,
                message,
                receiver_launcher_id,
                my_coin,
                &my_info,
                genesis_challenge,
            )?,
        ))
    }

    pub fn reward_distributor_from_spend(
        &self,
        spend: CoinSpend,
        reserve_lineage_proof: Option<LineageProof>,
        constants: RewardDistributorConstants,
    ) -> Result<Option<RewardDistributor>> {
        let mut ctx = self.0.lock().unwrap();

        let result =
            SdkRewardDistributor::from_spend(&mut ctx, &spend, reserve_lineage_proof, constants)?;

        Ok(result.map(|reward_distributor| RewardDistributor {
            clvm: self.0.clone(),
            distributor: Arc::new(Mutex::new(reward_distributor)),
        }))
    }

    pub fn reward_distributor_from_parent_spend(
        &self,
        parent_spend: CoinSpend,
        constants: RewardDistributorConstants,
    ) -> Result<Option<RewardDistributor>> {
        let mut ctx = self.0.lock().unwrap();

        let result = SdkRewardDistributor::from_parent_spend(&mut ctx, &parent_spend, constants)?;

        Ok(result.map(|reward_distributor| RewardDistributor {
            clvm: self.0.clone(),
            distributor: Arc::new(Mutex::new(reward_distributor)),
        }))
    }

    pub fn reward_distributor_from_eve_coin_spend(
        &self,
        constants: RewardDistributorConstants,
        initial_state: RewardDistributorState,
        eve_coin_spend: CoinSpend,
        reserve_parent_id: Bytes32,
        reserve_lineage_proof: LineageProof,
    ) -> Result<Option<RewardDistributorInfoFromEveCoin>> {
        let mut ctx = self.0.lock().unwrap();

        let result = SdkRewardDistributor::from_eve_coin_spend(
            &mut ctx,
            constants,
            initial_state,
            &eve_coin_spend,
            reserve_parent_id,
            reserve_lineage_proof,
        )?;

        Ok(result.map(
            |(reward_distributor, reward_slot)| RewardDistributorInfoFromEveCoin {
                distributor: RewardDistributor {
                    clvm: self.0.clone(),
                    distributor: Arc::new(Mutex::new(reward_distributor)),
                },
                first_reward_slot: RewardSlot::from_slot(reward_slot),
            },
        ))
    }

    pub fn launch_reward_distributor(
        &self,
        offer: SpendBundle,
        first_epoch_start: u64,
        cat_refund_puzzle_hash: Bytes32,
        constants: RewardDistributorConstants,
        mainnet: bool,
        comment: String,
    ) -> Result<RewardDistributorLaunchResult> {
        let mut ctx = self.0.lock().unwrap();

        let offer = Offer::from_spend_bundle(&mut ctx, &offer)?;
        let (
            security_signature,
            security_secret_key,
            sdk_distributor,
            first_epoch_slot,
            refunded_cat,
        ) = launch_reward_distributor(
            &mut ctx,
            &offer,
            first_epoch_start,
            cat_refund_puzzle_hash,
            constants,
            if mainnet {
                &MAINNET_CONSTANTS
            } else {
                &TESTNET11_CONSTANTS
            },
            &comment,
        )?;

        Ok(RewardDistributorLaunchResult {
            security_signature,
            security_secret_key,
            reward_distributor: RewardDistributor {
                clvm: self.0.clone(),
                distributor: Arc::new(Mutex::new(sdk_distributor)),
            },
            first_epoch_slot: RewardSlot::from_slot(first_epoch_slot),
            refunded_cat,
        })
    }

    pub fn create_offer_security_coin(
        &self,
        offer: SpendBundle,
    ) -> Result<OfferSecurityCoinDetails> {
        let mut ctx = self.0.lock().unwrap();

        let offer = Offer::from_spend_bundle(&mut ctx, &offer)?;

        let (security_coin_sk, security_coin) =
            create_security_coin(&mut ctx, offer.offered_coins().xch[0])?;

        Ok(OfferSecurityCoinDetails {
            security_coin,
            security_coin_sk,
        })
    }

    pub fn spend_offer_security_coin(
        &self,
        security_coin_details: OfferSecurityCoinDetails,
        conditions: Vec<Program>,
        mainnet: bool,
    ) -> Result<Signature> {
        let mut ctx = self.0.lock().unwrap();

        let mut sdk_conditions = Conditions::new();
        for condition in conditions {
            sdk_conditions.push(ctx.extract::<Condition<NodePtr>>(condition.1)?);
        }

        Ok(spend_security_coin(
            &mut ctx,
            security_coin_details.security_coin,
            sdk_conditions,
            &security_coin_details.security_coin_sk,
            if mainnet {
                &MAINNET_CONSTANTS
            } else {
                &TESTNET11_CONSTANTS
            },
        )?)
    }

    pub fn spend_settlement_nft(
        &self,
        offer: SpendBundle,
        nft_launcher_id: Bytes32,
        nonce: Bytes32,
        destination_puzzle_hash: Bytes32,
    ) -> Result<SettlementNftSpendResult> {
        let mut ctx = self.0.lock().unwrap();

        let offer = Offer::from_spend_bundle(&mut ctx, &offer)?;

        let (new_nft, security_conditions) = spend_settlement_nft(
            &mut ctx,
            &offer,
            nft_launcher_id,
            nonce,
            destination_puzzle_hash,
        )?;

        Ok(SettlementNftSpendResult {
            new_nft: new_nft.as_program(&self.0),
            security_conditions: security_conditions
                .into_iter()
                .map(|c| Program(self.0.clone(), ctx.alloc(&c).unwrap()))
                .collect(),
        })
    }

    pub fn offer_settlement_cats(&self, offer: SpendBundle, asset_id: Bytes32) -> Result<Vec<Cat>> {
        let mut ctx = self.0.lock().unwrap();

        let offer = Offer::from_spend_bundle(&mut ctx, &offer)?;

        Ok(offer
            .offered_coins()
            .cats
            .get(&asset_id)
            .cloned()
            .unwrap_or_default())
    }

    pub fn offer_settlement_nft(
        &self,
        offer: SpendBundle,
        nft_launcher_id: Bytes32,
    ) -> Result<Option<Nft>> {
        let mut ctx = self.0.lock().unwrap();

        let offer = Offer::from_spend_bundle(&mut ctx, &offer)?;

        Ok(offer
            .offered_coins()
            .nfts
            .get(&nft_launcher_id)
            .copied()
            .map(|n| n.as_program(&self.0)))
    }
}
