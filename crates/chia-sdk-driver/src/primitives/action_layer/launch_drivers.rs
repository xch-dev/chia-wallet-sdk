use bip39::Mnemonic;
use chia_bls::{sign, SecretKey, Signature};
use chia_consensus::consensus_constants::ConsensusConstants;
use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
    singleton::{SingletonArgs, SingletonSolution, SingletonStruct},
    standard::{StandardArgs, StandardSolution},
    EveProof, LineageProof, Memos, Proof,
};
use chia_sdk_signer::{AggSigConstants, RequiredBlsSignature};
use chia_sdk_types::{
    announcement_id,
    conditions::{AggSig, AggSigKind},
    puzzles::{
        CatalogSlotValue, DefaultCatMakerArgs, P2DelegatedBySingletonLayerArgs,
        RewardDistributorRewardSlotValue, RewardDistributorSlotNonce, SettlementPayment, SlotInfo,
        XchandlesHandleSlotValue, XchandlesSlotNonce,
    },
    Condition, Conditions, Mod,
};
use clvm_traits::{clvm_list, clvm_quote, clvm_tuple, FromClvm, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{
    Cat, CatSpend, CatalogRegistry, CatalogRegistryConstants, CatalogRegistryInfo,
    CatalogRegistryState, DriverError, Launcher, Layer, Nft, Offer, Reserve, RewardDistributor,
    RewardDistributorConstants, RewardDistributorInfo, RewardDistributorState, Slot, Spend,
    SpendContext, StandardLayer, XchandlesConstants, XchandlesRegistry, XchandlesRegistryInfo,
    XchandlesRegistryState,
};

#[allow(clippy::needless_pass_by_value)]
fn custom_err<T>(e: T) -> DriverError
where
    T: ToString,
{
    DriverError::Custom(e.to_string())
}

pub fn new_sk() -> Result<SecretKey, DriverError> {
    // we need the security coin puzzle hash to spend the offer coin after finding it
    let mut entropy = [0u8; 32];
    getrandom::getrandom(&mut entropy).map_err(custom_err)?;
    let mnemonic = Mnemonic::from_entropy(&entropy).map_err(custom_err)?;
    let seed = mnemonic.to_seed("");
    let sk = SecretKey::from_seed(&seed);
    Ok(sk)
}

pub fn spend_security_coin(
    ctx: &mut SpendContext,
    security_coin: Coin,
    conditions: Conditions<NodePtr>,
    sk: &SecretKey,
    consensus_constants: &ConsensusConstants,
) -> Result<Signature, DriverError> {
    let pk = sk.public_key();

    let layer = StandardLayer::new(pk);
    let puzzle_reveal_ptr = layer.construct_puzzle(ctx)?;

    let quoted_conditions_ptr = ctx.alloc(&clvm_quote!(conditions))?;
    let solution_ptr = layer.construct_solution(
        ctx,
        StandardSolution {
            original_public_key: None,
            delegated_puzzle: quoted_conditions_ptr,
            solution: NodePtr::NIL,
        },
    )?;

    let spend = Spend::new(puzzle_reveal_ptr, solution_ptr);
    ctx.spend(security_coin, spend)?;

    sign_standard_transaction(ctx, security_coin, spend, sk, consensus_constants)
}

pub fn sign_standard_transaction(
    ctx: &mut SpendContext,
    coin: Coin,
    spend: Spend,
    sk: &SecretKey,
    consensus_constants: &ConsensusConstants,
) -> Result<Signature, DriverError> {
    let output = ctx.run(spend.puzzle, spend.solution)?;
    let output = Vec::<Condition<NodePtr>>::from_clvm(ctx, output)?;
    let Some(agg_sig_me) = output.iter().find_map(|cond| {
        if let Condition::AggSigMe(agg_sig_me) = cond {
            return Some(agg_sig_me);
        }

        None
    }) else {
        return Err(DriverError::Custom(
            "Missing agg_sig_me from security coin".to_string(),
        ));
    };

    let required_signature = RequiredBlsSignature::from_condition(
        &coin,
        AggSig::new(
            AggSigKind::Me,
            agg_sig_me.public_key,
            agg_sig_me.message.clone(),
        ),
        &AggSigConstants::new(consensus_constants.agg_sig_me_additional_data),
    );

    Ok(sign(sk, required_signature.message()))
}

pub fn eve_singleton_inner_puzzle<S>(
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    slot_nonce: u64,
    left_slot_value: S,
    right_slot_value: S,
    memos_after_hint: NodePtr,
    target_inner_puzzle_hash: Bytes32,
) -> Result<NodePtr, DriverError>
where
    S: ToTreeHash,
{
    let left_slot_info = SlotInfo::from_value(launcher_id, slot_nonce, left_slot_value);
    let left_slot_puzzle_hash = Slot::<S>::puzzle_hash(&left_slot_info);

    let right_slot_info = SlotInfo::from_value(launcher_id, slot_nonce, right_slot_value);
    let right_slot_puzzle_hash = Slot::<S>::puzzle_hash(&right_slot_info);

    let slot_hint = Slot::<()>::first_curry_hash(launcher_id, slot_nonce).into();
    let slot_memos = ctx.hint(slot_hint)?;
    let launcher_id_ptr = ctx.alloc(&launcher_id)?;
    let launcher_memos = ctx.memos(&clvm_tuple!(launcher_id_ptr, memos_after_hint))?;

    clvm_quote!(Conditions::new()
        .create_coin(left_slot_puzzle_hash.into(), 0, slot_memos)
        .create_coin(right_slot_puzzle_hash.into(), 0, slot_memos)
        .create_coin(target_inner_puzzle_hash, 1, launcher_memos))
    .to_clvm(ctx)
    .map_err(DriverError::ToClvm)
}

// Spends the eve signleton, whose only job is to create the
//   slot 'premine' (leftmost and rightmost slots) and
//   transition to the actual registry puzzle
#[allow(clippy::type_complexity)]
#[allow(clippy::too_many_arguments)]
fn spend_eve_coin_and_create_registry<S, M, KV>(
    ctx: &mut SpendContext,
    launcher: Launcher,
    target_inner_puzzle_hash: Bytes32,
    slot_nonce: u64,
    left_slot_value: S,
    right_slot_value: S,
    memos_after_hint: M,
    launcher_kv_list: KV,
) -> Result<(Conditions, Coin, Proof, [Slot<S>; 2]), DriverError>
where
    S: Clone + ToTreeHash,
    M: ToClvm<Allocator>,
    KV: ToClvm<Allocator>,
{
    let launcher_coin = launcher.coin();
    let launcher_id = launcher_coin.coin_id();

    let memos_after_hint = ctx.alloc(&memos_after_hint)?;
    let eve_singleton_inner_puzzle = eve_singleton_inner_puzzle(
        ctx,
        launcher_id,
        slot_nonce,
        left_slot_value.clone(),
        right_slot_value.clone(),
        memos_after_hint,
        target_inner_puzzle_hash,
    )?;

    let eve_singleton_inner_puzzle_hash = ctx.tree_hash(eve_singleton_inner_puzzle);
    let eve_singleton_proof = Proof::Eve(EveProof {
        parent_parent_coin_info: launcher_coin.parent_coin_info,
        parent_amount: launcher_coin.amount,
    });

    let (security_coin_conditions, eve_coin) = launcher.with_singleton_amount(1).spend(
        ctx,
        eve_singleton_inner_puzzle_hash.into(),
        launcher_kv_list,
    )?;

    let eve_coin_solution = SingletonSolution {
        lineage_proof: eve_singleton_proof,
        amount: 1,
        inner_solution: NodePtr::NIL,
    }
    .to_clvm(ctx)?;

    let eve_singleton_puzzle =
        ctx.curry(SingletonArgs::new(launcher_id, eve_singleton_inner_puzzle))?;
    let eve_singleton_spend = Spend::new(eve_singleton_puzzle, eve_coin_solution);
    ctx.spend(eve_coin, eve_singleton_spend)?;

    let new_registry_coin = Coin::new(
        eve_coin.coin_id(),
        SingletonArgs::curry_tree_hash(launcher_id, target_inner_puzzle_hash.into()).into(),
        1,
    );
    let new_proof = Proof::Lineage(LineageProof {
        parent_parent_coin_info: eve_coin.parent_coin_info,
        parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
        parent_amount: 1,
    });

    let slot_proof = LineageProof {
        parent_parent_coin_info: eve_coin.parent_coin_info,
        parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
        parent_amount: 1,
    };
    let left_slot = Slot::new(
        slot_proof,
        SlotInfo::from_value(launcher_id, slot_nonce, left_slot_value),
    );
    let right_slot = Slot::new(
        slot_proof,
        SlotInfo::from_value(launcher_id, slot_nonce, right_slot_value),
    );

    Ok((
        security_coin_conditions.assert_concurrent_spend(eve_coin.coin_id()),
        new_registry_coin,
        new_proof,
        [left_slot, right_slot],
    ))
}

pub fn create_security_coin(
    ctx: &mut SpendContext,
    xch_settlement_coin: Coin,
) -> Result<(SecretKey, Coin), DriverError> {
    let security_coin_sk = new_sk()?;
    let security_coin_puzzle_hash =
        StandardArgs::curry_tree_hash(security_coin_sk.public_key()).into();

    let notarized_payment = NotarizedPayment {
        nonce: xch_settlement_coin.coin_id(),
        payments: vec![Payment::new(
            security_coin_puzzle_hash,
            xch_settlement_coin.amount,
            Memos::None,
        )],
    };
    let settlement_puzzle = ctx.alloc_mod::<SettlementPayment>()?;
    let settlement_solution = ctx.alloc(&SettlementPaymentsSolution {
        notarized_payments: vec![notarized_payment],
    })?;
    ctx.spend(
        xch_settlement_coin,
        Spend::new(settlement_puzzle, settlement_solution),
    )?;

    let security_coin = Coin::new(
        xch_settlement_coin.coin_id(),
        security_coin_puzzle_hash,
        xch_settlement_coin.amount,
    );

    Ok((security_coin_sk, security_coin))
}

#[allow(clippy::type_complexity)]
pub fn launch_catalog_registry<V>(
    ctx: &mut SpendContext,
    offer: &Offer,
    initial_registration_price: u64,
    // (registry launcher id, security coin, additional_args) -> (additional conditions, registry constants, initial_registration_asset_id)
    get_additional_info: fn(
        ctx: &mut SpendContext,
        Bytes32,
        Coin,
        V,
    ) -> Result<
        (Conditions<NodePtr>, CatalogRegistryConstants, Bytes32),
        DriverError,
    >,
    consensus_constants: &ConsensusConstants,
    additional_args: V,
) -> Result<
    (
        Signature,
        SecretKey,
        CatalogRegistry,
        [Slot<CatalogSlotValue>; 2],
        Coin, // security coin
    ),
    DriverError,
> {
    let (security_coin_sk, security_coin) =
        create_security_coin(ctx, offer.offered_coins().xch[0])?;
    offer
        .spend_bundle()
        .coin_spends
        .iter()
        .for_each(|cs| ctx.insert(cs.clone()));

    let security_coin_id = security_coin.coin_id();
    let mut security_coin_conditions = Conditions::new();

    // Create CATalog registry launcher
    let registry_launcher = Launcher::new(security_coin_id, 1);
    let registry_launcher_coin = registry_launcher.coin();
    let registry_launcher_id = registry_launcher_coin.coin_id();

    let (additional_security_coin_conditions, catalog_constants, initial_registration_asset_id) =
        get_additional_info(ctx, registry_launcher_id, security_coin, additional_args)?;

    let initial_state = CatalogRegistryState {
        registration_price: initial_registration_price,
        cat_maker_puzzle_hash: DefaultCatMakerArgs::new(
            initial_registration_asset_id.tree_hash().into(),
        )
        .curry_tree_hash()
        .into(),
    };
    let catalog_registry_info = CatalogRegistryInfo::new(
        initial_state,
        catalog_constants.with_launcher_id(registry_launcher_id),
    );
    let catalog_inner_puzzle_hash = catalog_registry_info.clone().inner_puzzle_hash();

    let (new_security_coin_conditions, new_catalog_registry_coin, catalog_proof, slots) =
        spend_eve_coin_and_create_registry(
            ctx,
            registry_launcher,
            catalog_inner_puzzle_hash.into(),
            0,
            CatalogSlotValue::initial_left_end(),
            CatalogSlotValue::initial_right_end(),
            clvm_tuple!(
                initial_registration_asset_id,
                clvm_tuple!(initial_state, ())
            ),
            (),
        )?;

    let catalog_registry = CatalogRegistry::new(
        new_catalog_registry_coin,
        catalog_proof,
        catalog_registry_info,
    );

    // this creates the CATalog registry & secures the spend
    security_coin_conditions = security_coin_conditions
        .extend(new_security_coin_conditions)
        .extend(additional_security_coin_conditions);

    // Spend security coin
    let security_coin_sig = spend_security_coin(
        ctx,
        security_coin,
        security_coin_conditions,
        &security_coin_sk,
        consensus_constants,
    )?;

    // Finally, return the data
    Ok((
        security_coin_sig + &offer.spend_bundle().aggregated_signature,
        security_coin_sk,
        catalog_registry,
        slots,
        security_coin,
    ))
}

#[allow(clippy::type_complexity)]
pub fn launch_xchandles_registry<V>(
    ctx: &mut SpendContext,
    offer: &Offer,
    initial_base_registration_price: u64,
    initial_registration_period: u64,
    // (registry launcher id, security coin, additional_args) -> (additional conditions, registry constants, initial_registration_asset_id)
    get_additional_info: fn(
        ctx: &mut SpendContext,
        Bytes32,
        Coin,
        V,
    ) -> Result<
        (Conditions<NodePtr>, XchandlesConstants, Bytes32),
        DriverError,
    >,
    consensus_constants: &ConsensusConstants,
    additional_args: V,
) -> Result<
    (
        Signature,
        SecretKey,
        XchandlesRegistry,
        [Slot<XchandlesHandleSlotValue>; 2],
        Coin, // security coin
    ),
    DriverError,
> {
    let (security_coin_sk, security_coin) =
        create_security_coin(ctx, offer.offered_coins().xch[0])?;
    offer
        .spend_bundle()
        .coin_spends
        .iter()
        .for_each(|cs| ctx.insert(cs.clone()));

    let security_coin_id = security_coin.coin_id();

    let mut security_coin_conditions = Conditions::new();

    // Create registry coin launcher
    let registry_launcher = Launcher::new(security_coin_id, 1);
    let registry_launcher_coin = registry_launcher.coin();
    let registry_launcher_id = registry_launcher_coin.coin_id();

    let (additional_security_coin_conditions, xchandles_constants, initial_registration_asset_id) =
        get_additional_info(ctx, registry_launcher_id, security_coin, additional_args)?;

    // Spend intermediary coin and create registry
    let initial_state = XchandlesRegistryState::from(
        initial_registration_asset_id.tree_hash().into(),
        initial_base_registration_price,
        initial_registration_period,
    );
    let target_xchandles_info = XchandlesRegistryInfo::new(
        initial_state,
        xchandles_constants.with_launcher_id(registry_launcher_id),
    );

    let target_xchandles_inner_puzzle_hash = target_xchandles_info.clone().inner_puzzle_hash();
    let (new_security_coin_conditions, new_xchandles_coin, xchandles_proof, slots) =
        spend_eve_coin_and_create_registry(
            ctx,
            registry_launcher,
            target_xchandles_inner_puzzle_hash.into(),
            XchandlesSlotNonce::HANDLE.to_u64(),
            XchandlesHandleSlotValue::initial_left_end(),
            XchandlesHandleSlotValue::initial_right_end(),
            (),
            clvm_list!(
                initial_registration_asset_id,
                initial_base_registration_price,
                initial_registration_period,
                initial_state,
                target_xchandles_info.constants
            ),
        )?;

    // this creates the launcher & secures the spend
    security_coin_conditions = security_coin_conditions
        .extend(new_security_coin_conditions)
        .extend(additional_security_coin_conditions);

    let xchandles_registry =
        XchandlesRegistry::new(new_xchandles_coin, xchandles_proof, target_xchandles_info);

    // Spend security coin
    let security_coin_sig = spend_security_coin(
        ctx,
        security_coin,
        security_coin_conditions,
        &security_coin_sk,
        consensus_constants,
    )?;

    // Finally, return the data
    Ok((
        security_coin_sig + &offer.spend_bundle().aggregated_signature,
        security_coin_sk,
        xchandles_registry,
        slots,
        security_coin,
    ))
}

pub fn spend_settlement_cats(
    ctx: &mut SpendContext,
    offer: &Offer,
    asset_id: Bytes32,
    nonce: Bytes32,
    payments: &[(Bytes32, u64)],
) -> Result<(Vec<Cat>, Conditions), DriverError> {
    let settlement_cats = offer
        .offered_coins()
        .cats
        .get(&asset_id)
        .ok_or(DriverError::Custom(
            "Could not find required CAT in offer".to_string(),
        ))?;

    let mut pmnts = Vec::with_capacity(payments.len());
    for (puzzle_hash, amount) in payments {
        pmnts.push(Payment::new(*puzzle_hash, *amount, ctx.hint(*puzzle_hash)?));
    }
    let notarized_payment = NotarizedPayment {
        nonce,
        payments: pmnts,
    };

    let offer_ann_message = ctx.alloc(&notarized_payment)?;
    let offer_ann_message: Bytes32 = ctx.tree_hash(offer_ann_message).into();

    let first_settlement_inner_solution = ctx.alloc(&SettlementPaymentsSolution {
        notarized_payments: vec![notarized_payment],
    })?;
    let settlement_inner_puzzle = ctx.alloc_mod::<SettlementPayment>()?;

    let security_coin_conditions = Conditions::new().assert_puzzle_announcement(announcement_id(
        settlement_cats[0].coin.puzzle_hash,
        offer_ann_message,
    ));

    let mut cat_spends = Vec::with_capacity(settlement_cats.len());
    for (i, cat) in settlement_cats.iter().enumerate() {
        cat_spends.push(CatSpend {
            cat: *cat,
            spend: Spend::new(
                settlement_inner_puzzle,
                if i == 0 {
                    first_settlement_inner_solution
                } else {
                    NodePtr::NIL
                },
            ),
            hidden: false,
        });
    }
    let created_cats = Cat::spend_all(ctx, &cat_spends)?;

    Ok((created_cats, security_coin_conditions))
}

pub fn spend_settlement_nft(
    ctx: &mut SpendContext,
    offer: &Offer,
    nft_launcher_id: Bytes32,
    nonce: Bytes32,
    destination_puzzle_hash: Bytes32,
) -> Result<(Nft, Conditions), DriverError> {
    let settlement_nft =
        offer
            .offered_coins()
            .nfts
            .get(&nft_launcher_id)
            .ok_or(DriverError::Custom(
                "Could not find required NFT in offer".to_string(),
            ))?;

    let notarized_payment = NotarizedPayment {
        nonce,
        payments: vec![Payment::new(
            destination_puzzle_hash,
            1,
            ctx.hint(destination_puzzle_hash)?,
        )],
    };

    let offer_ann_message = ctx.alloc(&notarized_payment)?;
    let offer_ann_message: Bytes32 = ctx.tree_hash(offer_ann_message).into();

    let settlement_inner_solution = ctx.alloc(&SettlementPaymentsSolution {
        notarized_payments: vec![notarized_payment],
    })?;
    let settlement_inner_puzzle = ctx.alloc_mod::<SettlementPayment>()?;

    let security_coin_conditions = Conditions::new().assert_puzzle_announcement(announcement_id(
        settlement_nft.coin.puzzle_hash,
        offer_ann_message,
    ));

    let created_nft = settlement_nft.spend(
        ctx,
        Spend::new(settlement_inner_puzzle, settlement_inner_solution),
    )?;

    Ok((created_nft, security_coin_conditions))
}

#[allow(clippy::type_complexity)]
pub fn launch_reward_distributor(
    ctx: &mut SpendContext,
    offer: &Offer,
    first_epoch_start: u64,
    cat_refund_puzzle_hash: Bytes32,
    constants: RewardDistributorConstants,
    consensus_constants: &ConsensusConstants,
    comment: &str,
) -> Result<
    (
        Signature,
        SecretKey,
        RewardDistributor,
        Slot<RewardDistributorRewardSlotValue>,
        Cat,
    ),
    DriverError,
> {
    let (security_coin_sk, security_coin) =
        create_security_coin(ctx, offer.offered_coins().xch[0])?;
    offer
        .spend_bundle()
        .coin_spends
        .iter()
        .for_each(|cs| ctx.insert(cs.clone()));

    let reward_distributor_hint: Bytes32 = "Reward Distributor v1".tree_hash().into();
    let launcher_memos = ctx.memos(&(reward_distributor_hint, (comment, ())))?;
    let launcher = Launcher::with_memos(security_coin.coin_id(), 1, launcher_memos);
    let launcher_coin = launcher.coin();
    let launcher_id = launcher_coin.coin_id();

    let controller_singleton_struct_hash = SingletonStruct::new(launcher_id).tree_hash().into();
    let reserve_inner_ph =
        P2DelegatedBySingletonLayerArgs::curry_tree_hash(controller_singleton_struct_hash, 0)
            .into();

    let total_cat_amount = offer
        .offered_coins()
        .cats
        .get(&constants.reserve_asset_id)
        .map_or(1, |cs| cs.iter().map(|c| c.coin.amount).sum::<u64>());

    let interim_cat_puzzle = clvm_quote!(Conditions::new()
        .create_coin(reserve_inner_ph, 0, ctx.hint(reserve_inner_ph)?)
        .create_coin(
            cat_refund_puzzle_hash,
            total_cat_amount,
            ctx.hint(cat_refund_puzzle_hash)?
        ));
    let interim_cat_puzzle = ctx.alloc(&interim_cat_puzzle)?;
    let interim_cat_puzzle_hash = ctx.tree_hash(interim_cat_puzzle);

    let (created_cats, mut security_coin_conditions) = spend_settlement_cats(
        ctx,
        offer,
        constants.reserve_asset_id,
        constants.launcher_id,
        &[(interim_cat_puzzle_hash.into(), total_cat_amount)],
    )?;

    let interim_cat = created_cats[0];
    let created_cats = Cat::spend_all(
        ctx,
        &[CatSpend {
            cat: interim_cat,
            spend: Spend::new(interim_cat_puzzle, NodePtr::NIL),
            hidden: false,
        }],
    )?;

    // Spend intermediary coin and create registry
    let target_info = RewardDistributorInfo::new(
        RewardDistributorState::initial(first_epoch_start),
        constants.with_launcher_id(launcher_id),
    );

    let target_inner_puzzle_hash = target_info.clone().inner_puzzle_hash();

    let slot_value = RewardDistributorRewardSlotValue {
        epoch_start: first_epoch_start,
        next_epoch_initialized: false,
        rewards: 0,
    };
    let slot_info = SlotInfo::<RewardDistributorRewardSlotValue>::from_value(
        launcher_id,
        RewardDistributorSlotNonce::REWARD.to_u64(),
        slot_value,
    );
    let slot_puzzle_hash = Slot::<RewardDistributorRewardSlotValue>::puzzle_hash(&slot_info);

    let slot_hint = first_epoch_start.tree_hash().into();
    let slot_memos = ctx.hint(slot_hint)?;
    let launcher_memos = ctx.hint(launcher_id)?;
    let eve_singleton_inner_puzzle = clvm_quote!(Conditions::new()
        .create_coin(slot_puzzle_hash.into(), 0, slot_memos)
        .create_coin(target_inner_puzzle_hash.into(), 1, launcher_memos))
    .to_clvm(ctx)?;

    let eve_singleton_inner_puzzle_hash = ctx.tree_hash(eve_singleton_inner_puzzle);
    let eve_singleton_proof = Proof::Eve(EveProof {
        parent_parent_coin_info: launcher_coin.parent_coin_info,
        parent_amount: launcher_coin.amount,
    });

    let (launch_conditions, eve_coin) = launcher.with_singleton_amount(1).spend(
        ctx,
        eve_singleton_inner_puzzle_hash.into(),
        (first_epoch_start, target_info.constants),
    )?;
    security_coin_conditions = security_coin_conditions.extend(launch_conditions);

    let eve_coin_solution = SingletonSolution {
        lineage_proof: eve_singleton_proof,
        amount: 1,
        inner_solution: NodePtr::NIL,
    }
    .to_clvm(ctx)?;

    let eve_singleton_puzzle =
        ctx.curry(SingletonArgs::new(launcher_id, eve_singleton_inner_puzzle))?;
    let eve_singleton_spend = Spend::new(eve_singleton_puzzle, eve_coin_solution);
    ctx.spend(eve_coin, eve_singleton_spend)?;

    let new_registry_coin = Coin::new(
        eve_coin.coin_id(),
        SingletonArgs::curry_tree_hash(launcher_id, target_inner_puzzle_hash).into(),
        1,
    );
    let new_proof = LineageProof {
        parent_parent_coin_info: eve_coin.parent_coin_info,
        parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
        parent_amount: eve_coin.amount,
    };
    let slot = Slot::new(new_proof, slot_info);

    // this creates the launcher & secures the spend
    let security_coin_conditions =
        security_coin_conditions.assert_concurrent_spend(eve_coin.coin_id());

    // create reserve and registry
    let reserve_cat = created_cats[0];
    let reserve = Reserve::new(
        reserve_cat.coin.parent_coin_info,
        reserve_cat.lineage_proof.unwrap(),
        reserve_cat.info.asset_id,
        controller_singleton_struct_hash,
        0,
        reserve_cat.coin.amount,
    );
    let registry = RewardDistributor::new(
        new_registry_coin,
        Proof::Lineage(new_proof),
        target_info,
        reserve,
    );

    // Spend security coin
    let security_coin_sig = spend_security_coin(
        ctx,
        security_coin,
        security_coin_conditions,
        &security_coin_sk,
        consensus_constants,
    )?;

    // Finally, return the data
    Ok((
        security_coin_sig + &offer.spend_bundle().aggregated_signature,
        security_coin_sk,
        registry,
        slot,
        created_cats[1], // refund cat
    ))
}

#[cfg(test)]
mod tests {
    use std::slice;

    use chia_protocol::{CoinSpend, SpendBundle};

    use chia_puzzle_types::{cat::GenesisByCoinIdTailArgs, CoinProof};
    use chia_puzzles::{SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH};
    use chia_sdk_test::{Benchmark, BlsPairWithCoin, Simulator};
    use chia_sdk_types::{
        puzzles::{
            AnyMetadataUpdater, CatNftMetadata, CompactCoinProof, DelegatedStateActionSolution,
            IntermediaryCoinProof, NftLauncherProof, XchandlesFactorPricingPuzzleArgs,
            XchandlesPricingSolution, ANY_METADATA_UPDATER_HASH,
        },
        MerkleTree, TESTNET11_CONSTANTS,
    };
    use clvm_traits::clvm_list;
    use clvmr::Allocator;
    use hex_literal::hex;

    use crate::{
        Asset, CatalogPrecommitValue, CatalogRefundAction, CatalogRegisterAction, DataStore,
        DataStoreMetadata, DelegatedPuzzle, DelegatedStateAction, HashedPtr, MetadataWithRootHash,
        NftMint, OracleLayer, PrecommitCoin, RewardDistributorAddEntryAction,
        RewardDistributorAddIncentivesAction, RewardDistributorCommitIncentivesAction,
        RewardDistributorInitiatePayoutAction, RewardDistributorNewEpochAction,
        RewardDistributorRefreshAction, RewardDistributorRemoveEntryAction,
        RewardDistributorStakeAction, RewardDistributorSyncAction, RewardDistributorType,
        RewardDistributorUnstakeAction, RewardDistributorWithdrawIncentivesAction, SingleCatSpend,
        SingletonInfo, Slot, SpendWithConditions, XchandlesExecuteUpdateAction,
        XchandlesExpireAction, XchandlesExpirePricingPuzzle, XchandlesExtendAction,
        XchandlesInitiateUpdateAction, XchandlesOracleAction, XchandlesPrecommitValue,
        XchandlesRefundAction, XchandlesRegisterAction, XchandlesRegistryReceivedMessagePrefix,
    };

    use super::*;

    fn cat_nft_metadata_for_testing() -> CatNftMetadata {
        CatNftMetadata {
            ticker: "TDBX".to_string(),
            name: "Testnet dexie bucks".to_string(),
            description: "    Testnet version of dexie bucks".to_string(),
            precision: 3,
            hidden_puzzle_hash: None,
            image_uris: vec!["https://icons-testnet.dexie.space/d82dd03f8a9ad2f84353cd953c4de6b21dbaaf7de3ba3f4ddd9abe31ecba80ad.webp".to_string()],
            image_hash: Bytes32::from(
                hex!("c84607c0e4cb4a878cc34ba913c90504ed0aac0f4484c2078529b9e42387da99")
            ),
            metadata_uris: vec!["https://icons-testnet.dexie.space/test.json".to_string()],
            metadata_hash: Some(Bytes32::from([2; 32])),
            license_uris: vec!["https://icons-testnet.dexie.space/license.pdf".to_string()],
            license_hash: Some(Bytes32::from([3; 32])),
        }
    }

    // ensures conditions are met
    fn ensure_conditions_met(
        ctx: &mut SpendContext,
        sim: &mut Simulator,
        conditions: Conditions<NodePtr>,
        amount_to_mint: u64,
    ) -> Result<(), DriverError> {
        let checker_puzzle_ptr = clvm_quote!(conditions).to_clvm(ctx)?;
        let checker_coin = sim.new_coin(ctx.tree_hash(checker_puzzle_ptr).into(), amount_to_mint);
        ctx.spend(checker_coin, Spend::new(checker_puzzle_ptr, NodePtr::NIL))?;

        Ok(())
    }

    // Launches a test singleton with an innter puzzle of '1'
    // JUST FOR TESTING PURPOSES PLEASE DO NOT USE THIS THING IN PRODUCTION
    fn launch_test_singleton(
        ctx: &mut SpendContext,
        sim: &mut Simulator,
    ) -> Result<(Bytes32, Coin, Proof, NodePtr, Bytes32, NodePtr), DriverError> {
        let test_singleton_launcher_coin = sim.new_coin(SINGLETON_LAUNCHER_HASH.into(), 1);
        let test_singleton_launcher =
            Launcher::new(test_singleton_launcher_coin.parent_coin_info, 1);

        let test_singleton_launcher_id = test_singleton_launcher.coin().coin_id();

        let test_singleton_inner_puzzle = ctx.alloc(&1)?;
        let test_singleton_inner_puzzle_hash = ctx.tree_hash(test_singleton_inner_puzzle);
        let (_, test_singleton_coin) =
            test_singleton_launcher.spend(ctx, test_singleton_inner_puzzle_hash.into(), ())?;

        let test_singleton_puzzle = ctx.curry(SingletonArgs::new(
            test_singleton_launcher_id,
            test_singleton_inner_puzzle,
        ))?;
        let test_singleton_proof = Proof::Eve(EveProof {
            parent_parent_coin_info: test_singleton_launcher_coin.parent_coin_info,
            parent_amount: test_singleton_launcher_coin.amount,
        });

        Ok((
            test_singleton_launcher_id,
            test_singleton_coin,
            test_singleton_proof,
            test_singleton_inner_puzzle,
            test_singleton_inner_puzzle_hash.into(),
            test_singleton_puzzle,
        ))
    }

    // Spends the price singleton to update the price of a registry
    fn spend_price_singleton<S>(
        ctx: &mut SpendContext,
        price_singleton_coin: Coin,
        price_singleton_proof: Proof,
        price_singleton_puzzle: NodePtr,
        new_state: &S,
        receiver_puzzle_hash: Bytes32,
    ) -> Result<(Coin, Proof, DelegatedStateActionSolution<NodePtr>), DriverError>
    where
        S: ToTreeHash + ToClvm<Allocator>,
    {
        let price_singleton_inner_puzzle = ctx.alloc(&1)?;
        let price_singleton_inner_puzzle_hash = ctx.tree_hash(price_singleton_inner_puzzle);

        let price_singleton_inner_solution = Conditions::new()
            .send_message(
                18,
                XchandlesRegistryReceivedMessagePrefix::update_state(new_state.tree_hash()).into(),
                vec![ctx.alloc(&receiver_puzzle_hash)?],
            )
            .create_coin(price_singleton_inner_puzzle_hash.into(), 1, Memos::None);

        let price_singleton_inner_solution = price_singleton_inner_solution.to_clvm(ctx)?;
        let price_singleton_solution = SingletonSolution {
            lineage_proof: price_singleton_proof,
            amount: 1,
            inner_solution: price_singleton_inner_solution,
        }
        .to_clvm(ctx)?;

        let price_singleton_spend = Spend::new(price_singleton_puzzle, price_singleton_solution);
        ctx.spend(price_singleton_coin, price_singleton_spend)?;

        // compute price singleton info for next spend
        let next_price_singleton_proof = Proof::Lineage(LineageProof {
            parent_parent_coin_info: price_singleton_coin.parent_coin_info,
            parent_inner_puzzle_hash: price_singleton_inner_puzzle_hash.into(),
            parent_amount: price_singleton_coin.amount,
        });
        let next_price_singleton_coin = Coin::new(
            price_singleton_coin.coin_id(),
            price_singleton_coin.puzzle_hash,
            1,
        );

        Ok((
            next_price_singleton_coin,
            next_price_singleton_proof,
            DelegatedStateActionSolution {
                new_state: new_state.to_clvm(ctx)?,
                other_singleton_inner_puzzle_hash: price_singleton_inner_puzzle_hash.into(),
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::similar_names)]
    fn test_refund_for_catalog(
        ctx: &mut SpendContext,
        sim: &mut Simulator,
        benchmark: &mut Benchmark,
        benchmark_label: &str,
        reg_amount: u64,
        payment_cat: Cat,
        tail_puzzle_to_refund: Option<NodePtr>,
        catalog: CatalogRegistry,
        catalog_constants: &CatalogRegistryConstants,
        slots: &[Slot<CatalogSlotValue>],
        user_puzzle_hash: Bytes32,
        minter_p2: StandardLayer,
        minter_puzzle_hash: Bytes32,
        sks: &[SecretKey; 2],
    ) -> anyhow::Result<(CatalogRegistry, Cat)> {
        // create precommit coin
        let user_coin = sim.new_coin(user_puzzle_hash, reg_amount);
        // pretty much a random TAIL - we're not actually launching it
        let tail = if let Some(t) = tail_puzzle_to_refund {
            t
        } else {
            ctx.curry(GenesisByCoinIdTailArgs::new(user_coin.coin_id()))?
        };
        let tail_hash = ctx.tree_hash(tail);
        // doesn't matter - we're getting refudned anyway
        let eve_nft_inner_puzzle_hash = tail_hash;

        let value = CatalogPrecommitValue::with_default_cat_maker(
            payment_cat.info.asset_id.tree_hash(),
            eve_nft_inner_puzzle_hash.into(),
            tail,
        );

        let refund_puzzle = ctx.alloc(&1)?;
        let refund_puzzle_hash = ctx.tree_hash(refund_puzzle);
        let precommit_coin = PrecommitCoin::new(
            ctx,
            payment_cat.coin.coin_id(),
            payment_cat.child_lineage_proof(),
            payment_cat.info.asset_id,
            SingletonStruct::new(catalog.info.constants.launcher_id)
                .tree_hash()
                .into(),
            catalog_constants.relative_block_height,
            catalog_constants.precommit_payout_puzzle_hash,
            refund_puzzle_hash.into(),
            value,
            reg_amount,
        )?;

        let payment_cat_inner_spend = minter_p2.spend_with_conditions(
            ctx,
            Conditions::new()
                .create_coin(precommit_coin.inner_puzzle_hash, reg_amount, Memos::None)
                .create_coin(
                    minter_puzzle_hash,
                    payment_cat.coin.amount - reg_amount,
                    Memos::None,
                ),
        )?;
        Cat::spend_all(
            ctx,
            &[CatSpend {
                cat: payment_cat,
                spend: payment_cat_inner_spend,
                hidden: false,
            }],
        )?;

        let new_payment_cat =
            payment_cat.child(minter_puzzle_hash, payment_cat.coin.amount - reg_amount);

        // sim.spend_coins(ctx.take(), sks)?;
        let spends = ctx.take();
        benchmark.add_spends(ctx, sim, spends, "create_precommit", sks)?;

        let slot = slots
            .iter()
            .find(|s| s.info.value.asset_id == tail_hash.into());

        let mut catalog = catalog;
        let secure_cond = catalog.new_action::<CatalogRefundAction>().spend(
            ctx,
            &mut catalog,
            tail_hash.into(),
            slot.map(|s| s.info.value.neighbors),
            &precommit_coin,
            slot.cloned(),
        )?;

        // check refund action created/spent slots function
        let created_slots = catalog.pending_spend.created_slots.clone();
        let spent_slots = catalog.pending_spend.spent_slots.clone();
        if slot.is_some() {
            assert_eq!(created_slots.len(), 1);
            assert_eq!(created_slots[0], slot.unwrap().info.value);

            assert_eq!(spent_slots.len(), 1);
            assert_eq!(spent_slots[0], slot.unwrap().info.value);
        } else {
            assert_eq!(created_slots.len(), 0);
            assert_eq!(spent_slots.len(), 0);
        }

        let (new_catalog, _) = catalog.finish_spend(ctx)?;

        ensure_conditions_met(ctx, sim, secure_cond, 0)?;

        // sim.spend_coins(ctx.take(), sks)?;
        let spends = ctx.take();
        benchmark.add_spends(ctx, sim, spends, benchmark_label, sks)?;

        Ok((new_catalog, new_payment_cat))
    }

    #[allow(clippy::similar_names)]
    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn test_catalog() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut sim = Simulator::new();
        let mut benchmark = Benchmark::new("CATalog".to_string());

        // setup config

        let initial_registration_price = 2000;
        let test_price_schedule = [1000, 500, 250];

        let catalog_constants = CatalogRegistryConstants {
            launcher_id: Bytes32::from([1; 32]),
            royalty_address: Bytes32::from([7; 32]),
            royalty_basis_points: 100,
            precommit_payout_puzzle_hash: Bytes32::from([8; 32]),
            relative_block_height: 1,
            price_singleton_launcher_id: Bytes32::from(hex!(
                "0000000000000000000000000000000000000000000000000000000000000000"
            )),
        };

        // Create source offer
        let user_bls = sim.bls(0);

        let offer_amount = 1;
        let launcher_bls = sim.bls(offer_amount);

        let offer_src_coin = launcher_bls.coin;
        let offer_spend = StandardLayer::new(launcher_bls.pk).spend_with_conditions(
            ctx,
            Conditions::new().create_coin(
                SETTLEMENT_PAYMENT_HASH.into(),
                offer_amount,
                Memos::None,
            ),
        )?;

        let puzzle_reveal = ctx.serialize(&offer_spend.puzzle)?;
        let solution = ctx.serialize(&offer_spend.solution)?;
        let agg_sig = sign_standard_transaction(
            ctx,
            offer_src_coin,
            offer_spend,
            &launcher_bls.sk,
            &TESTNET11_CONSTANTS,
        )?;
        let offer = Offer::from_spend_bundle(
            ctx,
            &SpendBundle {
                coin_spends: vec![CoinSpend::new(offer_src_coin, puzzle_reveal, solution)],
                aggregated_signature: agg_sig,
            },
        )?;

        let (
            price_singleton_launcher_id,
            mut price_singleton_coin,
            mut price_singleton_proof,
            _price_singleton_inner_puzzle,
            _price_singleton_inner_puzzle_hash,
            price_singleton_puzzle,
        ) = launch_test_singleton(ctx, &mut sim)?;

        // Launch test CAT
        let mut payment_cat_amount = 10_000_000;
        let minter_bls = sim.bls(payment_cat_amount);
        let minter_p2 = StandardLayer::new(minter_bls.pk);

        let (issue_cat, payment_cat) = Cat::issue_with_coin(
            ctx,
            minter_bls.coin.coin_id(),
            payment_cat_amount,
            Conditions::new().create_coin(minter_bls.puzzle_hash, payment_cat_amount, Memos::None),
        )?;
        let mut payment_cat = payment_cat[0];
        minter_p2.spend(ctx, minter_bls.coin, issue_cat)?;

        sim.spend_coins(ctx.take(), slice::from_ref(&minter_bls.sk))?;

        // Launch catalog
        let (_, security_sk, mut catalog, slots, _security_coin) = launch_catalog_registry(
            ctx,
            &offer,
            initial_registration_price,
            |_ctx, _launcher_id, _coin, (catalog_constants, initial_registration_asset_id)| {
                Ok((
                    Conditions::new(),
                    catalog_constants,
                    initial_registration_asset_id,
                ))
            },
            &TESTNET11_CONSTANTS,
            (
                catalog_constants.with_price_singleton(price_singleton_launcher_id),
                payment_cat.info.asset_id,
            ),
        )?;

        // sim.spend_coins(ctx.take(), &[launcher_bls.sk, security_sk])?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "launch",
            &[launcher_bls.sk, security_sk],
        )?;

        // Register CAT

        let mut tail = NodePtr::NIL; // will be used for refund as well
        let mut slots: Vec<Slot<CatalogSlotValue>> = slots.into();
        for i in 0..7 {
            // create precommit coin
            let reg_amount = if i % 2 == 1 {
                test_price_schedule[i / 2]
            } else {
                catalog.info.state.registration_price
            };
            let user_coin = sim.new_coin(user_bls.puzzle_hash, reg_amount);
            // pretty much a random TAIL - we're not actually launching it
            tail = ctx.curry(GenesisByCoinIdTailArgs::new(user_coin.coin_id()))?;
            let tail_hash = ctx.tree_hash(tail);

            let eve_nft_inner_puzzle = clvm_quote!(Conditions::new().create_coin(
                Bytes32::new([4 + i as u8; 32]),
                1,
                Memos::None,
            ))
            .to_clvm(ctx)?;
            let eve_nft_inner_puzzle_hash = ctx.tree_hash(eve_nft_inner_puzzle);

            let value = CatalogPrecommitValue::with_default_cat_maker(
                payment_cat.info.asset_id.tree_hash(),
                eve_nft_inner_puzzle_hash.into(),
                tail,
            );

            let refund_puzzle = ctx.alloc(&1)?;
            let refund_puzzle_hash = ctx.tree_hash(refund_puzzle);
            let precommit_coin = PrecommitCoin::new(
                ctx,
                payment_cat.coin.coin_id(),
                payment_cat.child_lineage_proof(),
                payment_cat.info.asset_id,
                SingletonStruct::new(catalog.info.constants.launcher_id)
                    .tree_hash()
                    .into(),
                catalog_constants.relative_block_height,
                catalog_constants.precommit_payout_puzzle_hash,
                refund_puzzle_hash.into(),
                value,
                reg_amount,
            )?;

            let payment_cat_inner_spend = minter_p2.spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(precommit_coin.inner_puzzle_hash, reg_amount, Memos::None)
                    .create_coin(
                        minter_bls.puzzle_hash,
                        payment_cat_amount - reg_amount,
                        Memos::None,
                    ),
            )?;
            let new_cats = Cat::spend_all(
                ctx,
                &[CatSpend {
                    cat: payment_cat,
                    spend: payment_cat_inner_spend,
                    hidden: false,
                }],
            )?;

            payment_cat_amount -= reg_amount;
            payment_cat = new_cats[1];

            // sim.spend_coins(ctx.take(), &[user_bls.sk.clone(), minter_bls.sk.clone()])?;
            let spends = ctx.take();
            println!("before adding and executing spends"); // todo: debug
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "create_precommit",
                &[user_bls.sk.clone(), minter_bls.sk.clone()],
            )?;
            println!("after adding and executing spends"); // todo: debug

            // call the 'register' action on CATalog
            slots.sort_unstable_by(|a, b| a.info.value.cmp(&b.info.value));

            let slot_value_to_insert =
                CatalogSlotValue::new(tail_hash.into(), Bytes32::default(), Bytes32::default());

            let mut left_slot: Option<Slot<CatalogSlotValue>> = None;
            let mut right_slot: Option<Slot<CatalogSlotValue>> = None;
            for slot in &slots {
                let slot_value = slot.info.value;

                if slot_value < slot_value_to_insert {
                    // slot belongs to the left
                    if let Some(left_slot_ref) = &left_slot {
                        if slot_value > left_slot_ref.info.value {
                            left_slot = Some(slot.clone());
                        }
                    } else {
                        left_slot = Some(slot.clone());
                    }
                } else {
                    // slot belongs to the right
                    if let Some(right_slot_ref) = &right_slot {
                        if slot_value < right_slot_ref.info.value {
                            right_slot = Some(slot.clone());
                        }
                    } else {
                        right_slot = Some(slot.clone());
                    }
                }
            }

            let (left_slot, right_slot) = (left_slot.unwrap(), right_slot.unwrap());

            if i % 2 == 1 {
                let new_price = reg_amount;
                assert_ne!(new_price, catalog.info.state.registration_price);

                let new_state = CatalogRegistryState {
                    cat_maker_puzzle_hash: DefaultCatMakerArgs::new(
                        payment_cat.info.asset_id.tree_hash().into(),
                    )
                    .curry_tree_hash()
                    .into(),
                    registration_price: new_price,
                };

                let (
                    new_price_singleton_coin,
                    new_price_singleton_proof,
                    delegated_state_action_solution,
                ) = spend_price_singleton(
                    ctx,
                    price_singleton_coin,
                    price_singleton_proof,
                    price_singleton_puzzle,
                    &new_state,
                    catalog.coin.puzzle_hash,
                )?;

                price_singleton_coin = new_price_singleton_coin;
                price_singleton_proof = new_price_singleton_proof;

                let (_conds, action_spend) = catalog.new_action::<DelegatedStateAction>().spend(
                    ctx,
                    catalog.coin,
                    new_state,
                    delegated_state_action_solution.other_singleton_inner_puzzle_hash,
                )?;

                catalog.insert_action_spend(ctx, action_spend)?;
                catalog = catalog.finish_spend(ctx)?.0;
                // sim.spend_coins(ctx.take(), slice::from_ref(&user_bls.sk))?;
                let spends = ctx.take();
                benchmark.add_spends(
                    ctx,
                    &mut sim,
                    spends,
                    "update_price",
                    slice::from_ref(&user_bls.sk),
                )?;
            }

            println!("before creating register action"); // todo: debug
            let secure_cond = catalog.new_action::<CatalogRegisterAction>().spend(
                ctx,
                &mut catalog,
                tail_hash.into(),
                left_slot.clone(),
                right_slot.clone(),
                &precommit_coin,
                Spend {
                    puzzle: eve_nft_inner_puzzle,
                    solution: NodePtr::NIL,
                },
            )?;
            println!("after creating register action"); // todo: debug

            // check register action created/spent slots function
            let created_slots = catalog
                .pending_spend
                .created_slots
                .iter()
                .map(|s| catalog.created_slot_value_to_slot(*s))
                .collect::<Vec<_>>();
            let spent_slots = catalog.pending_spend.spent_slots.clone();
            assert_eq!(spent_slots.len(), 2);
            assert_eq!(spent_slots[0], left_slot.info.value);
            assert_eq!(spent_slots[1], right_slot.info.value);

            println!("before finishing spend"); // todo: debug
            catalog = catalog.finish_spend(ctx)?.0;
            println!("after finishing spend"); // todo: debug

            ensure_conditions_met(ctx, &mut sim, secure_cond.clone(), 1)?;

            // sim.spend_coins(ctx.take(), slice::from_ref(&user_bls.sk))?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "register",
                slice::from_ref(&user_bls.sk),
            )?;

            slots.retain(|s| {
                s.info.value_hash != left_slot.info.value_hash
                    && s.info.value_hash != right_slot.info.value_hash
            });
            slots.extend(created_slots.clone());

            for s in created_slots {
                assert!(sim
                    .coin_state(s.coin.coin_id())
                    .map(|c| c.spent_height)
                    .is_some());
            }
        }

        assert_eq!(
            catalog.info.state.registration_price,
            test_price_schedule[2], // 1, 3, 5 updated the price
        );

        // Test refunds

        // b - the amount is wrong (by one)
        let (catalog, payment_cat) = test_refund_for_catalog(
            ctx,
            &mut sim,
            &mut benchmark,
            "refund_amount_wrong",
            catalog.info.state.registration_price + 1,
            payment_cat,
            None,
            catalog,
            &catalog_constants,
            &slots,
            user_bls.puzzle_hash,
            minter_p2,
            minter_bls.puzzle_hash,
            &[user_bls.sk.clone(), minter_bls.sk.clone()],
        )?;

        // a - the CAT maker puzzle has changed
        // i.e., use different payment CAT
        let alternative_payment_cat_amount = 10_000_000;
        let minter2_bls = sim.bls(alternative_payment_cat_amount);
        let minter_p2_2 = StandardLayer::new(minter2_bls.pk);

        let (issue_cat, alternative_payment_cat) = Cat::issue_with_coin(
            ctx,
            minter2_bls.coin.coin_id(),
            alternative_payment_cat_amount,
            Conditions::new().create_coin(
                minter2_bls.puzzle_hash,
                alternative_payment_cat_amount,
                Memos::None,
            ),
        )?;
        minter_p2_2.spend(ctx, minter2_bls.coin, issue_cat)?;
        let alternative_payment_cat = alternative_payment_cat[0];

        sim.spend_coins(ctx.take(), slice::from_ref(&minter2_bls.sk))?;

        let (catalog, _alternative_payment_cat) = test_refund_for_catalog(
            ctx,
            &mut sim,
            &mut benchmark,
            "refund_cat_changed",
            catalog.info.state.registration_price,
            alternative_payment_cat,
            None,
            catalog,
            &catalog_constants,
            &slots,
            user_bls.puzzle_hash,
            minter_p2_2,
            minter2_bls.puzzle_hash,
            &[user_bls.sk.clone(), minter2_bls.sk.clone()],
        )?;

        // c - the tail hash has already been registered
        let (_catalog, _payment_cat) = test_refund_for_catalog(
            ctx,
            &mut sim,
            &mut benchmark,
            "refund_cat_already_registered",
            catalog.info.state.registration_price,
            payment_cat,
            Some(tail),
            catalog,
            &catalog_constants,
            &slots,
            user_bls.puzzle_hash,
            minter_p2,
            minter_bls.puzzle_hash,
            &[user_bls.sk.clone(), minter_bls.sk.clone()],
        )?;

        benchmark.print_summary(Some("catalog.costs"));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::similar_names)]
    fn test_refund_for_xchandles(
        ctx: &mut SpendContext,
        sim: &mut Simulator,
        benchmark: &mut Benchmark,
        benchmark_label: &str,
        handle_to_refund: &str,
        pricing_puzzle: NodePtr,
        pricing_solution: NodePtr,
        slot: Option<Slot<XchandlesHandleSlotValue>>,
        payment_cat: Cat,
        payment_cat_amount: u64,
        registry: XchandlesRegistry,
        minter_p2: StandardLayer,
        minter_puzzle_hash: Bytes32,
        minter_sk: &SecretKey,
        user_sk: &SecretKey,
    ) -> anyhow::Result<(XchandlesRegistry, Cat)> {
        let pricing_puzzle_hash = ctx.tree_hash(pricing_puzzle);
        let pricing_solution_hash = ctx.tree_hash(pricing_solution);

        let value = XchandlesPrecommitValue::for_normal_registration(
            payment_cat.info.asset_id.tree_hash(),
            pricing_puzzle_hash,
            &pricing_solution_hash,
            handle_to_refund.to_string(),
            Bytes32::default(),
            Bytes32::default(),
            Bytes32::default(),
        );

        let refund_puzzle = ctx.alloc(&1)?;
        let refund_puzzle_hash = ctx.tree_hash(refund_puzzle);
        let precommit_coin = PrecommitCoin::new(
            ctx,
            payment_cat.coin.coin_id(),
            payment_cat.child_lineage_proof(),
            payment_cat.info.asset_id,
            SingletonStruct::new(registry.info.constants.launcher_id)
                .tree_hash()
                .into(),
            registry.info.constants.relative_block_height,
            registry.info.constants.precommit_payout_puzzle_hash,
            refund_puzzle_hash.into(),
            value,
            payment_cat_amount,
        )?;

        let payment_cat_inner_spend = minter_p2.spend_with_conditions(
            ctx,
            Conditions::new()
                .create_coin(
                    precommit_coin.inner_puzzle_hash,
                    payment_cat_amount,
                    Memos::None,
                )
                .create_coin(
                    minter_puzzle_hash,
                    payment_cat.coin.amount - payment_cat_amount,
                    Memos::None,
                ),
        )?;
        Cat::spend_all(
            ctx,
            &[CatSpend {
                cat: payment_cat,
                spend: payment_cat_inner_spend,
                hidden: false,
            }],
        )?;

        let new_payment_cat = payment_cat.child(
            minter_puzzle_hash,
            payment_cat.coin.amount - payment_cat_amount,
        );

        // sim.spend_coins(ctx.take(), &[user_sk.clone(), minter_sk.clone()])?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            sim,
            spends,
            "create_precommit",
            &[user_sk.clone(), minter_sk.clone()],
        )?;

        let mut registry = registry;
        let used_slot_value_hash = slot.clone().map(|s| s.info.value_hash);
        let secure_cond = registry.new_action::<XchandlesRefundAction>().spend(
            ctx,
            &mut registry,
            &precommit_coin,
            pricing_puzzle,
            pricing_solution,
            slot,
        )?;
        if let Some(used_slot_value_hash) = used_slot_value_hash {
            assert_eq!(
                used_slot_value_hash,
                registry.pending_spend.spent_handle_slots
                    [registry.pending_spend.spent_handle_slots.len() - 1]
                    .tree_hash()
                    .into()
            );
        }

        let (new_registry, _) = registry.finish_spend(ctx)?;

        ensure_conditions_met(ctx, sim, secure_cond.clone(), 0)?;

        // sim.spend_coins(ctx.take(), slice::from_ref(user_sk))?;
        let spends = ctx.take();
        benchmark.add_spends(ctx, sim, spends, benchmark_label, slice::from_ref(user_sk))?;

        Ok((new_registry, new_payment_cat))
    }

    #[allow(clippy::similar_names)]
    #[allow(clippy::cast_possible_truncation)]
    #[test]
    fn test_xchandles() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut sim = Simulator::new();
        let mut benchmark = Benchmark::new("XCHandles".to_string());
        // setup config
        let initial_registration_price = 2000;
        let test_price_schedule = [1000, 500, 250];

        let xchandles_constants = XchandlesConstants {
            launcher_id: Bytes32::from([1; 32]),
            precommit_payout_puzzle_hash: Bytes32::from([8; 32]),
            relative_block_height: 1,
            price_singleton_launcher_id: Bytes32::from(hex!(
                "0000000000000000000000000000000000000000000000000000000000000000"
            )),
        };

        // Create source offer
        let user_bls = sim.bls(0);
        let user_p2 = StandardLayer::new(user_bls.pk);

        let offer_amount = 1;
        let launcher_bls = sim.bls(offer_amount);
        let offer_spend = StandardLayer::new(launcher_bls.pk).spend_with_conditions(
            ctx,
            Conditions::new().create_coin(
                SETTLEMENT_PAYMENT_HASH.into(),
                offer_amount,
                Memos::None,
            ),
        )?;

        let puzzle_reveal = ctx.serialize(&offer_spend.puzzle)?;
        let solution = ctx.serialize(&offer_spend.solution)?;
        let agg_sig = sign_standard_transaction(
            ctx,
            launcher_bls.coin,
            offer_spend,
            &launcher_bls.sk,
            &TESTNET11_CONSTANTS,
        )?;
        let offer = Offer::from_spend_bundle(
            ctx,
            &SpendBundle {
                coin_spends: vec![CoinSpend::new(launcher_bls.coin, puzzle_reveal, solution)],
                aggregated_signature: agg_sig,
            },
        )?;

        // Launch CAT
        let mut payment_cat_amount = 10_000_000;
        let minter_bls = sim.bls(payment_cat_amount);
        let minter_p2 = StandardLayer::new(minter_bls.pk);

        let (issue_cat, payment_cat) = Cat::issue_with_coin(
            ctx,
            minter_bls.coin.coin_id(),
            payment_cat_amount,
            Conditions::new().create_coin(minter_bls.puzzle_hash, payment_cat_amount, Memos::None),
        )?;
        let mut payment_cat = payment_cat[0];
        minter_p2.spend(ctx, minter_bls.coin, issue_cat)?;

        sim.spend_coins(ctx.take(), slice::from_ref(&minter_bls.sk))?;

        // Launch price singleton
        let (
            price_singleton_launcher_id,
            mut price_singleton_coin,
            mut price_singleton_proof,
            _price_singleton_inner_puzzle,
            _price_singleton_inner_puzzle_hash,
            price_singleton_puzzle,
        ) = launch_test_singleton(ctx, &mut sim)?;

        // Launch XCHandles
        let registration_period = 366 * 24 * 60 * 60;
        let (_, security_sk, mut registry, slots_returned_by_launch, _security_coin) =
            launch_xchandles_registry(
                ctx,
                &offer,
                initial_registration_price,
                registration_period,
                |_ctx, _launcher_id, _coin, (xchandles_constants, payment_cat_asset_id)| {
                    Ok((Conditions::new(), xchandles_constants, payment_cat_asset_id))
                },
                &TESTNET11_CONSTANTS,
                (
                    xchandles_constants.with_price_singleton(price_singleton_launcher_id),
                    payment_cat.info.asset_id,
                ),
            )?;

        // Check XCHandlesRegistry::from_launcher_solution
        let spends = ctx.take();
        let mut initial_slots = None;
        for spend in spends {
            if spend.coin.puzzle_hash == SINGLETON_LAUNCHER_HASH.into() {
                let launcher_solution = ctx.alloc(&spend.solution)?;

                if let Some((registry, slots, initial_registration_asset_id, initial_base_price)) =
                    XchandlesRegistry::from_launcher_solution(ctx, spend.coin, launcher_solution)?
                {
                    initial_slots = Some(slots);
                    assert_eq!(initial_registration_asset_id, payment_cat.info.asset_id);
                    assert_eq!(
                        registry.info.constants,
                        xchandles_constants
                            .with_price_singleton(price_singleton_launcher_id)
                            .with_launcher_id(spend.coin.coin_id())
                    );
                    assert_eq!(initial_registration_price, initial_base_price);
                }
            }

            ctx.insert(spend);
        }

        // This will fail if we didn't find (or were not able to parse) the XCHandles launcher
        assert!(initial_slots.is_some());

        // sim.spend_coins(ctx.take(), &[launcher_bls.sk, security_sk])?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "launch",
            &[launcher_bls.sk, security_sk],
        )?;

        let slots = initial_slots.unwrap();
        assert!(sim.coin_state(slots[0].coin.coin_id()).is_some());
        assert!(sim.coin_state(slots[1].coin.coin_id()).is_some());
        assert_eq!(slots, slots_returned_by_launch);

        // Register 7 handles

        let mut base_price = initial_registration_price;

        // this DID will be the owner and resolved of all handles at end of for loop
        let launcher_coin = sim.new_coin(SINGLETON_LAUNCHER_HASH.into(), 1);
        let launcher = Launcher::new(launcher_coin.parent_coin_info, 1);
        let (_, mut owner_did) = launcher.create_simple_did(ctx, &user_p2)?;
        sim.spend_coins(ctx.take(), std::slice::from_ref(&user_bls.sk))?;

        let mut slots: Vec<Slot<XchandlesHandleSlotValue>> = slots.into();
        for i in 0..7 {
            // mint controller singleton (it's a DID, not an NFT - don't rat on me to the NFT board plz)
            let launcher_coin = sim.new_coin(SINGLETON_LAUNCHER_HASH.into(), 1);
            let launcher = Launcher::new(launcher_coin.parent_coin_info, 1);
            let (_, mut did) = launcher.create_simple_did(ctx, &user_p2)?;

            // name is "aa" + "a" * i + "{i}"
            let handle = if i == 0 {
                "aa0".to_string()
            } else {
                "aa".to_string() + &"a".repeat(i).to_string() + &i.to_string()
            };
            let handle_hash = handle.tree_hash().into();

            // create precommit coin
            if i % 2 == 1 {
                base_price = test_price_schedule[i / 2];
            }
            let reg_amount = XchandlesFactorPricingPuzzleArgs::get_price(base_price, &handle, 1);

            let handle_owner_launcher_id = did.info.launcher_id;
            let handle_resolved_launcher_id = did.info.launcher_id;
            let secret = Bytes32::default();

            let value = XchandlesPrecommitValue::for_normal_registration(
                payment_cat.info.asset_id.tree_hash(),
                XchandlesFactorPricingPuzzleArgs {
                    base_price,
                    registration_period,
                }
                .curry_tree_hash(),
                &XchandlesPricingSolution {
                    buy_time: 100,
                    current_expiration: 0,
                    handle: handle.clone(),
                    num_periods: 1,
                }
                .tree_hash(),
                handle.clone(),
                secret,
                handle_owner_launcher_id,
                handle_resolved_launcher_id,
            );

            let refund_puzzle = ctx.alloc(&1)?;
            let refund_puzzle_hash = ctx.tree_hash(refund_puzzle);
            let precommit_coin = PrecommitCoin::new(
                ctx,
                payment_cat.coin.coin_id(),
                payment_cat.child_lineage_proof(),
                payment_cat.info.asset_id,
                SingletonStruct::new(registry.info.constants.launcher_id)
                    .tree_hash()
                    .into(),
                xchandles_constants.relative_block_height,
                xchandles_constants.precommit_payout_puzzle_hash,
                refund_puzzle_hash.into(),
                value,
                reg_amount,
            )?;

            let payment_cat_inner_spend = minter_p2.spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(precommit_coin.inner_puzzle_hash, reg_amount, Memos::None)
                    .create_coin(
                        minter_bls.puzzle_hash,
                        payment_cat_amount - reg_amount,
                        Memos::None,
                    ),
            )?;
            Cat::spend_all(
                ctx,
                &[CatSpend {
                    cat: payment_cat,
                    spend: payment_cat_inner_spend,
                    hidden: false,
                }],
            )?;

            payment_cat_amount -= reg_amount;
            payment_cat = payment_cat.child(minter_bls.puzzle_hash, payment_cat_amount);

            // sim.spend_coins(ctx.take(), &[user_bls.sk.clone(), minter_bls.sk.clone()])?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "create_precommit",
                &[user_bls.sk.clone(), minter_bls.sk.clone()],
            )?;

            // call the 'register' action on the registry
            slots.sort_unstable_by(|a, b| a.info.value.cmp(&b.info.value));

            let slot_value_to_insert = XchandlesHandleSlotValue::new(
                handle_hash,
                Bytes32::default(),
                Bytes32::default(),
                0,
                Bytes32::default(),
                Bytes32::default(),
            );

            let mut left_slot: Option<Slot<XchandlesHandleSlotValue>> = None;
            let mut right_slot: Option<Slot<XchandlesHandleSlotValue>> = None;
            for slot in &slots {
                let slot_value = slot.info.value;

                if slot_value < slot_value_to_insert {
                    // slot belongs to the left
                    if let Some(left_slot_ref) = &left_slot {
                        if slot_value > left_slot_ref.info.value {
                            left_slot = Some(slot.clone());
                        }
                    } else {
                        left_slot = Some(slot.clone());
                    }
                } else {
                    // slot belongs to the right
                    if let Some(right_slot_ref) = &right_slot {
                        if slot_value < right_slot_ref.info.value {
                            right_slot = Some(slot.clone());
                        }
                    } else {
                        right_slot = Some(slot.clone());
                    }
                }
            }

            let (left_slot, right_slot) = (left_slot.unwrap(), right_slot.unwrap());

            // update price
            if i % 2 == 1 {
                let new_price = test_price_schedule[i / 2];
                let new_price_puzzle_hash: Bytes32 = XchandlesFactorPricingPuzzleArgs {
                    base_price: new_price,
                    registration_period,
                }
                .curry_tree_hash()
                .into();
                assert_ne!(
                    new_price_puzzle_hash,
                    registry.info.state.pricing_puzzle_hash
                );

                let (
                    new_price_singleton_coin,
                    new_price_singleton_proof,
                    delegated_state_action_solution,
                ) = spend_price_singleton(
                    ctx,
                    price_singleton_coin,
                    price_singleton_proof,
                    price_singleton_puzzle,
                    &XchandlesRegistryState::from(
                        payment_cat.info.asset_id.tree_hash().into(),
                        new_price,
                        registration_period,
                    ),
                    registry.coin.puzzle_hash,
                )?;

                price_singleton_coin = new_price_singleton_coin;
                price_singleton_proof = new_price_singleton_proof;

                let (_conds, action_spend) = registry.new_action::<DelegatedStateAction>().spend(
                    ctx,
                    registry.coin,
                    delegated_state_action_solution.new_state,
                    delegated_state_action_solution.other_singleton_inner_puzzle_hash,
                )?;

                registry.insert_action_spend(ctx, action_spend)?;
                registry = registry.finish_spend(ctx)?.0;
                // sim.spend_coins(ctx.take(), slice::from_ref(&user_bls.sk))?;
                let spends = ctx.take();
                benchmark.add_spends(
                    ctx,
                    &mut sim,
                    spends,
                    "update_price",
                    slice::from_ref(&user_bls.sk),
                )?;
            }

            let spent_values = [left_slot.info.value, right_slot.info.value];
            let (secure_cond, owner_conds, _resolved_conds) =
                registry.new_action::<XchandlesRegisterAction>().spend(
                    ctx,
                    &mut registry,
                    left_slot.clone(),
                    right_slot.clone(),
                    &precommit_coin,
                    base_price,
                    registration_period,
                    100,
                    did.info.inner_puzzle_hash().into(),
                    did.info.inner_puzzle_hash().into(),
                )?;

            ensure_conditions_met(ctx, &mut sim, secure_cond.clone(), 1)?;
            did = did.update(ctx, &user_p2, owner_conds)?;

            assert_eq!(
                registry
                    .pending_spend
                    .spent_handle_slots
                    .iter()
                    .rev()
                    .take(2)
                    .collect::<Vec<&XchandlesHandleSlotValue>>(),
                spent_values.iter().rev().collect::<Vec<_>>(),
            );
            let new_slots = registry
                .pending_spend
                .created_handle_slots
                .iter()
                .map(|s| registry.created_handle_slot_value_to_slot(*s))
                .collect::<Vec<_>>();
            registry = registry.finish_spend(ctx)?.0;
            sim.pass_time(100); // registration start was at timestamp 100

            // sim.spend_coins(ctx.take(), slice::from_ref(&user_bls.sk))?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "register",
                slice::from_ref(&user_bls.sk),
            )?;

            slots.retain(|s| {
                s.info.value_hash != left_slot.info.value_hash
                    && s.info.value_hash != right_slot.info.value_hash
            });

            let oracle_slot = new_slots[1].clone();
            slots.extend(new_slots);

            // test on-chain oracle for current handle
            let spent_slot_value_hash = oracle_slot.info.value_hash;
            let oracle_conds = registry.new_action::<XchandlesOracleAction>().spend(
                ctx,
                &mut registry,
                oracle_slot.clone(),
            )?;
            let new_slot = registry
                .created_handle_slot_value_to_slot(registry.pending_spend.created_handle_slots[0]);

            ensure_conditions_met(ctx, &mut sim, oracle_conds, 0)?;

            assert_eq!(
                spent_slot_value_hash,
                registry
                    .pending_spend
                    .spent_handle_slots
                    .iter()
                    .next_back()
                    .unwrap()
                    .tree_hash()
                    .into()
            );
            registry = registry.finish_spend(ctx)?.0;

            // sim.spend_coins(ctx.take(), slice::from_ref(&user_bls.sk))?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "oracle",
                slice::from_ref(&user_bls.sk),
            )?;

            slots.retain(|s| s.info.value_hash != oracle_slot.info.value_hash);
            slots.push(new_slot.clone());

            // test on-chain extend mechanism for current handle
            let extension_years = i as u64 + 1;
            let extension_slot = new_slot;
            let pay_for_extension =
                XchandlesFactorPricingPuzzleArgs::get_price(base_price, &handle, extension_years);

            let spent_slot_value_hash = extension_slot.info.value_hash;
            let (extend_conds, notarized_payment) =
                registry.new_action::<XchandlesExtendAction>().spend(
                    ctx,
                    &mut registry,
                    &handle,
                    extension_slot.clone(),
                    payment_cat.info.asset_id,
                    base_price,
                    registration_period,
                    extension_years,
                    0,
                )?;
            let new_slot = registry
                .created_handle_slot_value_to_slot(registry.pending_spend.created_handle_slots[0]);

            assert_eq!(
                spent_slot_value_hash,
                registry
                    .pending_spend
                    .spent_handle_slots
                    .iter()
                    .next_back()
                    .unwrap()
                    .tree_hash()
                    .into()
            );

            let payment_cat_inner_spend = minter_p2.spend_with_conditions(
                ctx,
                extend_conds
                    .create_coin(
                        SETTLEMENT_PAYMENT_HASH.into(),
                        pay_for_extension,
                        Memos::None,
                    )
                    .create_coin(
                        minter_bls.puzzle_hash,
                        payment_cat_amount - pay_for_extension,
                        Memos::None,
                    ),
            )?;

            let cat_offer_inner_spend = Spend::new(
                ctx.alloc_mod::<SettlementPayment>()?,
                ctx.alloc(&clvm_list!(notarized_payment))?,
            );

            Cat::spend_all(
                ctx,
                &[
                    CatSpend {
                        cat: payment_cat,
                        spend: payment_cat_inner_spend,
                        hidden: false,
                    },
                    CatSpend {
                        cat: payment_cat.child(SETTLEMENT_PAYMENT_HASH.into(), pay_for_extension),
                        spend: cat_offer_inner_spend,
                        hidden: false,
                    },
                ],
            )?;

            payment_cat_amount -= pay_for_extension;
            payment_cat = payment_cat.child(minter_bls.puzzle_hash, payment_cat_amount);

            registry = registry.finish_spend(ctx)?.0;

            // sim.spend_coins(spends, &[user_bls.sk.clone(), minter_bls.sk.clone()])?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "extend",
                &[user_bls.sk.clone(), minter_bls.sk.clone()],
            )?;

            slots.retain(|s| s.info.value_hash != extension_slot.info.value_hash);
            slots.push(new_slot.clone());

            // test on-chain mechanism for handle updates
            let new_owner_launcher_id = owner_did.info.launcher_id;
            let new_resolved_launcher_id = owner_did.info.launcher_id;
            let update_slot = new_slot;
            let update_slot_value_hash = update_slot.info.value_hash;

            let min_height = sim.height() + 1;
            let initiate_update_conds = registry
                .new_action::<XchandlesInitiateUpdateAction>()
                .spend(
                    ctx,
                    &mut registry,
                    update_slot.clone(),
                    new_owner_launcher_id,
                    new_resolved_launcher_id,
                    CompactCoinProof {
                        parent_coin_info: did.coin.parent_coin_info,
                        inner_puzzle_hash: did.info.inner_puzzle_hash().into(),
                        amount: 1,
                    },
                    min_height,
                )?;

            slots.retain(|s| s.info.value_hash != update_slot.info.value_hash);
            let mut new_slot = registry
                .created_handle_slot_value_to_slot(registry.pending_spend.created_handle_slots[0]);

            // note: update slot is now taking on a new meaning
            let update_slot = registry
                .created_update_slot_value_to_slot(registry.pending_spend.created_update_slots[0]);
            assert_eq!(
                update_slot.info.value.min_height,
                min_height + xchandles_constants.relative_block_height
            );

            did = did.update(ctx, &user_p2, initiate_update_conds)?;

            assert_eq!(
                update_slot_value_hash,
                registry
                    .pending_spend
                    .spent_handle_slots
                    .iter()
                    .next_back()
                    .unwrap()
                    .tree_hash()
                    .into()
            );
            registry = registry.finish_spend(ctx)?.0;

            // sim.spend_coins(ctx.take(), slice::from_ref(&user_bls.sk))?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "initiate_update",
                slice::from_ref(&user_bls.sk),
            )?;

            slots.push(new_slot.clone());
            for _ in 0..=(xchandles_constants.relative_block_height as usize) {
                sim.create_block();
            }

            let (old_owner_conds, new_owner_conds, new_resolved_conds) = registry
                .new_action::<XchandlesExecuteUpdateAction>()
                .spend(
                    ctx,
                    &mut registry,
                    new_slot.clone(),
                    update_slot.clone(),
                    new_owner_launcher_id,
                    new_resolved_launcher_id,
                    CompactCoinProof {
                        parent_coin_info: did.coin.parent_coin_info,
                        inner_puzzle_hash: did.info.inner_puzzle_hash().into(),
                        amount: 1,
                    },
                    min_height + xchandles_constants.relative_block_height,
                    owner_did.info.inner_puzzle_hash().into(),
                    owner_did.info.inner_puzzle_hash().into(),
                )?;
            slots.retain(|s| s.info.value_hash != new_slot.info.value_hash);
            new_slot = registry
                .created_handle_slot_value_to_slot(registry.pending_spend.created_handle_slots[0]);

            registry = registry.finish_spend(ctx)?.0;

            let _new_did = did.update(ctx, &user_p2, old_owner_conds)?;
            owner_did =
                owner_did.update(ctx, &user_p2, new_owner_conds.extend(new_resolved_conds))?;

            // sim.spend_coins(ctx.take(), slice::from_ref(&user_bls.sk))?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "execute_update",
                slice::from_ref(&user_bls.sk),
            )?;

            slots.push(new_slot.clone());
        }

        assert_eq!(
            registry.info.state.pricing_puzzle_hash,
            // iterations 1, 3, 5 updated the price
            XchandlesFactorPricingPuzzleArgs {
                base_price: test_price_schedule[2],
                registration_period
            }
            .curry_tree_hash()
            .into(),
        );

        // expire one of the slots
        let handle_to_expire = "aa0".to_string();
        let handle_hash = handle_to_expire.tree_hash().into();
        let initial_slot = slots
            .iter()
            .find(|s| s.info.value.handle_hash == handle_hash)
            .unwrap();

        // precommit coin needed
        let refund_puzzle = ctx.alloc(&1)?;
        let refund_puzzle_hash = ctx.tree_hash(refund_puzzle);
        let expiration = initial_slot.info.value.expiration;
        let buy_time = expiration + 27 * 24 * 60 * 60; // last day of auction; 0 < premium < 1 CAT
        let value = XchandlesPrecommitValue::for_normal_registration(
            payment_cat.info.asset_id.tree_hash(),
            XchandlesExpirePricingPuzzle::curry_tree_hash(base_price, registration_period),
            &XchandlesPricingSolution {
                buy_time,
                current_expiration: expiration,
                handle: handle_to_expire.clone(),
                num_periods: 1,
            }
            .tree_hash(),
            handle_to_expire.clone(),
            Bytes32::default(),
            Bytes32::from([42; 32]),
            Bytes32::from([69; 32]),
        );

        let pricing_puzzle =
            XchandlesExpirePricingPuzzle::from_info(ctx, base_price, registration_period)?;
        let reg_amount = XchandlesExpirePricingPuzzle::get_price(
            ctx,
            pricing_puzzle,
            handle_to_expire,
            expiration,
            buy_time,
            1,
        )? as u64;

        let precommit_coin = PrecommitCoin::<XchandlesPrecommitValue>::new(
            ctx,
            payment_cat.coin.coin_id(),
            payment_cat.child_lineage_proof(),
            payment_cat.info.asset_id,
            SingletonStruct::new(registry.info.constants.launcher_id)
                .tree_hash()
                .into(),
            xchandles_constants.relative_block_height,
            xchandles_constants.precommit_payout_puzzle_hash,
            refund_puzzle_hash.into(),
            value,
            reg_amount,
        )?;
        assert!(reg_amount <= payment_cat_amount);

        let payment_cat_inner_spend = minter_p2.spend_with_conditions(
            ctx,
            Conditions::new()
                .create_coin(precommit_coin.inner_puzzle_hash, reg_amount, Memos::None)
                .create_coin(
                    minter_bls.puzzle_hash,
                    payment_cat_amount - reg_amount,
                    Memos::None,
                ),
        )?;
        Cat::spend_all(
            ctx,
            &[CatSpend {
                cat: payment_cat,
                spend: payment_cat_inner_spend,
                hidden: false,
            }],
        )?;

        payment_cat =
            payment_cat.child(minter_bls.puzzle_hash, payment_cat.coin.amount - reg_amount);

        sim.set_next_timestamp(buy_time)?;
        // sim.spend_coins(ctx.take(), &[user_bls.sk.clone(), minter_bls.sk.clone()])?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "create_precommit",
            &[user_bls.sk.clone(), minter_bls.sk.clone()],
        )?;

        let spent_slot_value_hash = initial_slot.info.value_hash;
        let expire_conds = registry.new_action::<XchandlesExpireAction>().spend(
            ctx,
            &mut registry,
            initial_slot.clone(),
            1,
            base_price,
            registration_period,
            &precommit_coin,
            buy_time,
        )?;

        // assert expire conds
        ensure_conditions_met(ctx, &mut sim, expire_conds, 1)?;

        assert_eq!(
            spent_slot_value_hash,
            registry
                .pending_spend
                .spent_handle_slots
                .iter()
                .next_back()
                .unwrap()
                .tree_hash()
                .into()
        );
        registry = registry.finish_spend(ctx)?.0;

        // sim.spend_coins(ctx.take(), slice::from_ref(&user_bls.sk))?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "expire",
            slice::from_ref(&user_bls.sk),
        )?;

        // Test refunds
        let unregistered_handle = "yak7".to_string();

        for use_factor_pricing in [true, false] {
            let pricing_puzzle = if use_factor_pricing {
                ctx.curry(XchandlesFactorPricingPuzzleArgs {
                    base_price,
                    registration_period,
                })?
            } else {
                let args =
                    XchandlesExpirePricingPuzzle::from_info(ctx, base_price, registration_period)?;

                ctx.curry(args)?
            };
            let pricing_solution = ctx.alloc(&XchandlesPricingSolution {
                buy_time: 28 * 24 * 60 * 60 + 1, // premium should be 0
                current_expiration: 0,
                handle: unregistered_handle.clone(),
                num_periods: 1,
            })?;

            let expected_price =
                XchandlesFactorPricingPuzzleArgs::get_price(base_price, &unregistered_handle, 1);
            let other_pricing_puzzle = if use_factor_pricing {
                ctx.curry(XchandlesFactorPricingPuzzleArgs {
                    base_price: base_price + 1,
                    registration_period,
                })?
            } else {
                let args = XchandlesExpirePricingPuzzle::from_info(
                    ctx,
                    base_price + 1,
                    registration_period,
                )?;
                ctx.curry(args)?
            };
            let other_expected_price = XchandlesFactorPricingPuzzleArgs::get_price(
                base_price + 1,
                &unregistered_handle,
                1,
            );
            assert_ne!(other_expected_price, expected_price);

            let existing_handle = if use_factor_pricing {
                "aaa1".to_string()
            } else {
                "aaaa2".to_string()
            };
            let existing_slot = slots
                .iter()
                .find(|s| s.info.value.handle_hash == existing_handle.tree_hash().into())
                .unwrap()
                .clone();
            let existing_handle_pricing_solution = ctx.alloc(&XchandlesPricingSolution {
                buy_time: existing_slot.info.value.expiration + 28 * 24 * 60 * 60 + 1, // premium should be 0
                current_expiration: existing_slot.info.value.expiration,
                handle: existing_handle.clone(),
                num_periods: 1,
            })?;
            let existing_handle_expected_price =
                XchandlesFactorPricingPuzzleArgs::get_price(base_price, &existing_handle, 1);

            // a - the CAT maker puzzle has changed
            let alternative_payment_cat_amount = 10_000_000;
            let minter2 = sim.bls(alternative_payment_cat_amount);
            let minter_p2_2 = StandardLayer::new(minter2.pk);

            let (issue_cat, alternative_payment_cat) = Cat::issue_with_coin(
                ctx,
                minter2.coin.coin_id(),
                alternative_payment_cat_amount,
                Conditions::new().create_coin(
                    minter2.puzzle_hash,
                    alternative_payment_cat_amount,
                    Memos::None,
                ),
            )?;
            minter_p2_2.spend(ctx, minter2.coin, issue_cat)?;

            let alternative_payment_cat = alternative_payment_cat[0];
            sim.spend_coins(ctx.take(), slice::from_ref(&minter2.sk))?;

            registry = test_refund_for_xchandles(
                ctx,
                &mut sim,
                &mut benchmark,
                "refund_cat_wrong",
                &unregistered_handle,
                pricing_puzzle,
                pricing_solution,
                None,
                alternative_payment_cat,
                expected_price,
                registry,
                minter_p2_2,
                minter2.puzzle_hash,
                &minter2.sk,
                &user_bls.sk,
            )?
            .0;

            // b - the amount is wrong
            (registry, payment_cat) = test_refund_for_xchandles(
                ctx,
                &mut sim,
                &mut benchmark,
                "refund_amount_wrong",
                &unregistered_handle,
                pricing_puzzle,
                pricing_solution,
                None,
                payment_cat,
                expected_price + 1,
                registry,
                minter_p2,
                minter_bls.puzzle_hash,
                &minter_bls.sk,
                &user_bls.sk,
            )?;

            // c - the pricing puzzle has changed
            (registry, payment_cat) = test_refund_for_xchandles(
                ctx,
                &mut sim,
                &mut benchmark,
                "refund_pricing_wrong",
                &unregistered_handle,
                other_pricing_puzzle,
                pricing_solution,
                None,
                payment_cat,
                other_expected_price,
                registry,
                minter_p2,
                minter_bls.puzzle_hash,
                &minter_bls.sk,
                &user_bls.sk,
            )?;

            // d - the handle has already been registered
            (registry, payment_cat) = test_refund_for_xchandles(
                ctx,
                &mut sim,
                &mut benchmark,
                "refund_handle_already_registered",
                &existing_handle, // already registered handle
                pricing_puzzle,
                existing_handle_pricing_solution,
                Some(existing_slot),
                payment_cat,
                existing_handle_expected_price,
                registry,
                minter_p2,
                minter_bls.puzzle_hash,
                &minter_bls.sk,
                &user_bls.sk,
            )?;
        }

        benchmark.print_summary(Some("xchandles.costs"));

        Ok(())
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_nft_with_any_metadata_updater() -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut sim = Simulator::new();

        let bls = sim.bls(1);
        let p2 = StandardLayer::new(bls.pk);

        let nft_launcher = Launcher::new(bls.coin.coin_id(), 1);

        let royalty_puzzle_hash = Bytes32::from([7; 32]);

        let metadata = ctx.alloc(&cat_nft_metadata_for_testing())?;
        let metadata = HashedPtr::from_ptr(ctx, metadata);
        let (create_nft, nft) = nft_launcher.mint_nft(
            ctx,
            &NftMint {
                metadata,
                metadata_updater_puzzle_hash: ANY_METADATA_UPDATER_HASH.into(),
                royalty_puzzle_hash,
                royalty_basis_points: 100,
                p2_puzzle_hash: bls.puzzle_hash,
                transfer_condition: None,
            },
        )?;
        p2.spend(ctx, bls.coin, create_nft)?;

        // actually try to run updater
        let new_metadata = CatNftMetadata {
            ticker: "XXX".to_string(),
            name: "Test Name".to_string(),
            description: "Test desc".to_string(),
            precision: 3,
            hidden_puzzle_hash: None,
            image_uris: vec!["img URI".to_string()],
            image_hash: Bytes32::from([31; 32]),
            metadata_uris: vec!["meta URI".to_string()],
            metadata_hash: Some(Bytes32::from([8; 32])),
            license_uris: vec!["license URI".to_string()],
            license_hash: Some(Bytes32::from([9; 32])),
        };

        let metadata_update = Spend {
            puzzle: ctx.alloc_mod::<AnyMetadataUpdater>()?,
            solution: ctx.alloc(&new_metadata)?,
        };

        let new_nft = nft.transfer_with_metadata(
            ctx,
            &p2,
            bls.puzzle_hash,
            metadata_update,
            Conditions::new(),
        )?;

        assert_eq!(
            ctx.extract::<CatNftMetadata>(new_nft.info.metadata.ptr())?,
            new_metadata
        );
        sim.spend_coins(ctx.take(), &[bls.sk])?;
        Ok(())
    }

    // Spends the manager singleton
    fn spend_manager_singleton(
        ctx: &mut SpendContext,
        test_singleton_coin: Coin,
        test_singleton_proof: Proof,
        test_singleton_puzzle: NodePtr,
        test_singleton_output_conditions: Conditions<NodePtr>,
    ) -> Result<(Coin, Proof), DriverError> {
        let test_singleton_inner_puzzle = ctx.alloc(&1)?;
        let test_singleton_inner_puzzle_hash = ctx.tree_hash(test_singleton_inner_puzzle);

        let test_singleton_inner_solution = test_singleton_output_conditions
            .create_coin(test_singleton_inner_puzzle_hash.into(), 1, Memos::None)
            .to_clvm(ctx)?;
        let test_singleton_solution = ctx.alloc(&SingletonSolution {
            lineage_proof: test_singleton_proof,
            amount: 1,
            inner_solution: test_singleton_inner_solution,
        })?;

        let test_singleton_spend = Spend::new(test_singleton_puzzle, test_singleton_solution);
        ctx.spend(test_singleton_coin, test_singleton_spend)?;

        // compute manager singleton info for next spend
        let next_test_singleton_proof = Proof::Lineage(LineageProof {
            parent_parent_coin_info: test_singleton_coin.parent_coin_info,
            parent_inner_puzzle_hash: test_singleton_inner_puzzle_hash.into(),
            parent_amount: test_singleton_coin.amount,
        });
        let next_test_singleton_coin = Coin::new(
            test_singleton_coin.coin_id(),
            test_singleton_coin.puzzle_hash,
            1,
        );

        Ok((next_test_singleton_coin, next_test_singleton_proof))
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum RewardDistributorTestType {
        Managed,
        NftCollection,
        CuratedNft { refreshable: bool },
        Cat,
    }

    #[test]
    fn test_managed_reward_distributor() -> anyhow::Result<()> {
        test_reward_distributor(RewardDistributorTestType::Managed)
    }

    #[test]
    fn test_collection_nft_reward_distributor() -> anyhow::Result<()> {
        test_reward_distributor(RewardDistributorTestType::NftCollection)
    }

    #[test]
    fn test_curated_nft_non_refreshable_reward_distributor() -> anyhow::Result<()> {
        test_reward_distributor(RewardDistributorTestType::CuratedNft { refreshable: false })
    }

    #[test]
    fn test_curated_nft_refreshable_reward_distributor() -> anyhow::Result<()> {
        test_reward_distributor(RewardDistributorTestType::CuratedNft { refreshable: true })
    }

    #[test]
    fn test_cat_reward_distributor() -> anyhow::Result<()> {
        test_reward_distributor(RewardDistributorTestType::Cat)
    }

    fn update_datastore(
        ctx: &mut SpendContext,
        sim: &mut Simulator,
        benchmark: &mut Benchmark,
        datastore: DataStore,
        delegated_puzzles: &[DelegatedPuzzle],
        new_metadata: DataStoreMetadata,
        datastore_p2: &BlsPairWithCoin,
    ) -> anyhow::Result<DataStore<DataStoreMetadata>> {
        let owner_layer = StandardLayer::new(datastore_p2.pk);
        let recreate = DataStore::<()>::owner_create_coin_condition(
            ctx,
            datastore.info.launcher_id,
            owner_layer.tree_hash().into(),
            delegated_puzzles.to_vec(),
            false,
        )?;

        let new_metadata_condition = DataStore::new_metadata_condition(ctx, new_metadata)?;

        let inner_spend = owner_layer.spend_with_conditions(
            ctx,
            Conditions::new()
                .with(recreate)
                .with(new_metadata_condition),
        )?;
        let dl_spend = datastore.spend(ctx, inner_spend)?;

        let new_datastore = DataStore::from_spend(ctx, &dl_spend, delegated_puzzles)?.unwrap();

        benchmark.add_spends(
            ctx,
            sim,
            vec![dl_spend],
            "update_datastore",
            std::slice::from_ref(&datastore_p2.sk),
        )?;

        Ok(new_datastore)
    }

    #[allow(clippy::similar_names)]
    fn test_reward_distributor(test_type: RewardDistributorTestType) -> anyhow::Result<()> {
        let ctx = &mut SpendContext::new();
        let mut sim = Simulator::new();
        let mut benchmark = Benchmark::new(format!(
            "{} Reward Distributor",
            match test_type {
                RewardDistributorTestType::Managed => "Managed",
                RewardDistributorTestType::NftCollection => "NFT Collection",
                RewardDistributorTestType::CuratedNft { refreshable: false } =>
                    "Curated NFT (non-refreshable)",
                RewardDistributorTestType::CuratedNft { refreshable: true } =>
                    "Curated NFT (refreshable)",
                RewardDistributorTestType::Cat => "CAT",
            }
        ));

        // Launch reward token CAT
        let cat_amount = 10_000_000_000;
        let cat_minter = sim.bls(cat_amount);
        let cat_minter_p2 = StandardLayer::new(cat_minter.pk);

        let (issue_cat, source_cat) = Cat::issue_with_coin(
            ctx,
            cat_minter.coin.coin_id(),
            cat_amount,
            Conditions::new().create_coin(cat_minter.puzzle_hash, cat_amount, Memos::None),
        )?;
        cat_minter_p2.spend(ctx, cat_minter.coin, issue_cat)?;

        let source_cat = source_cat[0];
        sim.spend_coins(ctx.take(), slice::from_ref(&cat_minter.sk))?;

        // Launch manager singleton
        // What this singleton is depends on the mode:
        //  - for managed mode, it's a manager singleton
        //  - for nft collection mode, it's a DID singleton
        //  - for curated nft mode, it's a store singleton
        //  - for cat mode, it's not used
        let (
            manager_launcher_id,
            mut manager_coin,
            mut manager_singleton_proof,
            _manager_singleton_inner_puzzle,
            manager_singleton_inner_puzzle_hash,
            manager_singleton_puzzle,
        ) = launch_test_singleton(ctx, &mut sim)?;

        let datastore_p2 = sim.bls(1);
        let oracle_fee = 1336;
        let delegated_puzzles = vec![DelegatedPuzzle::Oracle(Bytes32::default(), 1336)];
        let mut merkle_tree = MerkleTree::new(&[]);
        let mut datastore: Option<DataStore> = if let RewardDistributorTestType::CuratedNft {
            refreshable: _,
        } = test_type
        {
            let (launch_singleton, datastore) = Launcher::new(datastore_p2.coin.coin_id(), 1)
                .mint_datastore(
                    ctx,
                    DataStoreMetadata::root_hash_only(merkle_tree.root()),
                    datastore_p2.puzzle_hash.into(),
                    delegated_puzzles.clone(),
                )?;
            StandardLayer::new(datastore_p2.pk).spend(ctx, datastore_p2.coin, launch_singleton)?;
            sim.spend_coins(ctx.take(), slice::from_ref(&datastore_p2.sk))?;

            Some(datastore)
        } else {
            None
        };

        let stakeable_cat_minter = sim.bls(cat_amount);
        let stakeable_cat_minter_p2 = StandardLayer::new(stakeable_cat_minter.pk);
        let mut source_stakeable_cat = if let RewardDistributorTestType::Cat = test_type {
            let (issue_cat, stakeable_cat) = Cat::issue_with_coin(
                ctx,
                stakeable_cat_minter.coin.coin_id(),
                cat_amount,
                Conditions::new().create_coin(
                    stakeable_cat_minter.puzzle_hash,
                    cat_amount,
                    Memos::None,
                ),
            )?;
            stakeable_cat_minter_p2.spend(ctx, stakeable_cat_minter.coin, issue_cat)?;

            let stakeable_cat = stakeable_cat[0];
            sim.spend_coins(ctx.take(), slice::from_ref(&stakeable_cat_minter.sk))?;
            Some(stakeable_cat)
        } else {
            None
        };

        // setup config
        let require_payout_approval = match test_type {
            RewardDistributorTestType::Managed => false,
            RewardDistributorTestType::NftCollection
            | RewardDistributorTestType::CuratedNft { refreshable: _ }
            | RewardDistributorTestType::Cat => true,
        };
        let constants = RewardDistributorConstants::without_launcher_id(
            match test_type {
                RewardDistributorTestType::Managed => RewardDistributorType::Managed {
                    manager_singleton_launcher_id: manager_launcher_id,
                },
                RewardDistributorTestType::NftCollection => RewardDistributorType::NftCollection {
                    collection_did_launcher_id: manager_launcher_id,
                },
                RewardDistributorTestType::CuratedNft { refreshable } => {
                    RewardDistributorType::CuratedNft {
                        store_launcher_id: datastore.as_ref().unwrap().info.launcher_id,
                        refreshable,
                    }
                }
                RewardDistributorTestType::Cat => RewardDistributorType::Cat {
                    asset_id: source_stakeable_cat.as_ref().unwrap().info.asset_id,
                    hidden_puzzle_hash: None,
                },
            },
            Bytes32::new([1; 32]),
            1000,
            u64::MAX, // precision
            300,
            42,
            require_payout_approval,
            420,  // 4.2% fee
            9000, // 90% of the amount deposited will be returned
            source_cat.info.asset_id,
        );

        // Create source offer
        let entry1_bls = sim.bls(0);
        let entry2_bls = sim.bls(0);

        let offer_amount = 1;
        let launcher_bls = sim.bls(offer_amount);
        let offer_spend = StandardLayer::new(launcher_bls.pk).spend_with_conditions(
            ctx,
            Conditions::new().create_coin(
                SETTLEMENT_PAYMENT_HASH.into(),
                offer_amount,
                Memos::None,
            ),
        )?;

        let puzzle_reveal = ctx.serialize(&offer_spend.puzzle)?;
        let solution = ctx.serialize(&offer_spend.solution)?;

        let cat_minter_inner_puzzle = clvm_quote!(Conditions::new().create_coin(
            SETTLEMENT_PAYMENT_HASH.into(),
            source_cat.coin.amount,
            Memos::None
        ))
        .to_clvm(ctx)?;
        let source_cat_inner_spend = cat_minter_p2.delegated_inner_spend(
            ctx,
            Spend {
                puzzle: cat_minter_inner_puzzle,
                solution: NodePtr::NIL,
            },
        )?;
        source_cat.spend(
            ctx,
            SingleCatSpend {
                prev_coin_id: source_cat.coin.coin_id(),
                next_coin_proof: CoinProof {
                    parent_coin_info: source_cat.coin.parent_coin_info,
                    inner_puzzle_hash: cat_minter.puzzle_hash,
                    amount: source_cat.coin.amount,
                },
                prev_subtotal: 0,
                extra_delta: 0,
                p2_spend: source_cat_inner_spend,
                revoke: false,
            },
        )?;
        let spends = ctx.take();
        let cat_offer_spend = spends
            .iter()
            .find(|s| s.coin.coin_id() == source_cat.coin.coin_id())
            .unwrap()
            .clone();
        for spend in spends {
            if spend.coin.coin_id() != source_cat.coin.coin_id() {
                ctx.insert(spend);
            }
        }

        let agg_sig = sign_standard_transaction(
            ctx,
            launcher_bls.coin,
            offer_spend,
            &launcher_bls.sk,
            &TESTNET11_CONSTANTS,
        )?;
        let offer = Offer::from_spend_bundle(
            ctx,
            &SpendBundle {
                coin_spends: vec![
                    CoinSpend::new(launcher_bls.coin, puzzle_reveal, solution),
                    cat_offer_spend,
                ],
                aggregated_signature: agg_sig,
            },
        )?;

        // Launch the reward distributor
        let first_epoch_start = 1234;
        let (_, security_sk, mut registry, first_epoch_slot, mut source_cat) =
            launch_reward_distributor(
                ctx,
                &offer,
                first_epoch_start,
                source_cat.info.p2_puzzle_hash,
                constants,
                &TESTNET11_CONSTANTS,
                "yak yak yak",
            )?;

        // sim.spend_coins(
        //     ctx.take(),
        //     &[
        //         launcher_bls.sk.clone(),
        //         security_sk.clone(),
        //         cat_minter.sk.clone(),
        //     ],
        // )?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "launch",
            &[
                launcher_bls.sk.clone(),
                security_sk.clone(),
                cat_minter.sk.clone(),
            ],
        )?;

        assert!(sim.coin_state(source_cat.coin.coin_id()).is_some());

        let nft_bls = sim.bls(1);

        // add the 1st entry/NFT before reward epoch ('first epoch') begins
        let (entry1_slot, _nft1) = if test_type == RewardDistributorTestType::Managed {
            let manager_conditions = registry
                .new_action::<RewardDistributorAddEntryAction>()
                .spend(
                    ctx,
                    &mut registry,
                    entry1_bls.puzzle_hash,
                    1,
                    manager_singleton_inner_puzzle_hash,
                )?;
            let entry1_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_entry_slots[0],
                RewardDistributorSlotNonce::ENTRY,
            );
            registry = registry.finish_spend(ctx, vec![])?.0;

            (manager_coin, manager_singleton_proof) = spend_manager_singleton(
                ctx,
                manager_coin,
                manager_singleton_proof,
                manager_singleton_puzzle,
                manager_conditions,
            )?;

            // sim.spend_coins(ctx.take(), &[])?;
            let spends = ctx.take();
            benchmark.add_spends(ctx, &mut sim, spends, "add_entry", &[])?;

            (entry1_slot, None)
        } else if test_type == RewardDistributorTestType::Cat {
            let stakeable_cat = source_stakeable_cat.as_ref().unwrap();
            let offered_cat = stakeable_cat.child(SETTLEMENT_PAYMENT_HASH.into(), 1);
            let (security_conds, np, _locked_cat) = registry
                .new_action::<RewardDistributorStakeAction>()
                .spend_for_cat_mode(
                    ctx,
                    &mut registry,
                    offered_cat,
                    entry1_bls.puzzle_hash,
                    None,
                )?;
            let entry1_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_entry_slots[0],
                RewardDistributorSlotNonce::ENTRY,
            );
            registry = registry.finish_spend(ctx, vec![])?.0;

            let stakeable_cat_delegated_puzzle = ctx.alloc(&clvm_quote!(security_conds
                .create_coin(SETTLEMENT_PAYMENT_HASH.into(), 1, Memos::None)
                .create_coin(
                    stakeable_cat.p2_puzzle_hash(),
                    stakeable_cat.amount() - 1,
                    Memos::None,
                )))?;
            let stakeable_cat_spend = stakeable_cat_minter_p2.delegated_inner_spend(
                ctx,
                Spend::new(stakeable_cat_delegated_puzzle, NodePtr::NIL),
            )?;

            let offer_sol = ctx.alloc(&SettlementPaymentsSolution {
                notarized_payments: vec![np],
            })?;
            let offered_cat_spend = Spend::new(ctx.alloc_mod::<SettlementPayment>()?, offer_sol);
            let _new_cats = Cat::spend_all(
                ctx,
                &[
                    CatSpend::new(*stakeable_cat, stakeable_cat_spend),
                    CatSpend::new(offered_cat, offered_cat_spend),
                ],
            )?;

            source_stakeable_cat = Some(
                stakeable_cat.child(stakeable_cat.p2_puzzle_hash(), stakeable_cat.amount() - 1),
            );

            // sim.spend_coins(ctx.take(), &[])?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "stake_cat",
                &[
                    nft_bls.sk.clone(),
                    datastore_p2.sk.clone(),
                    stakeable_cat_minter.sk.clone(),
                ],
            )?;
            (entry1_slot, None)
        } else {
            let nft_launcher = Launcher::new(manager_coin.coin_id(), 0).with_singleton_amount(1);
            let nft_launcher_coin = nft_launcher.coin();
            let meta = ctx.alloc(&"nft1")?;
            let meta = HashedPtr::from_ptr(ctx, meta);

            let (conds, nft) = nft_launcher.mint_nft(
                ctx,
                &NftMint {
                    metadata: meta,
                    metadata_updater_puzzle_hash: ANY_METADATA_UPDATER_HASH.into(),
                    royalty_puzzle_hash: Bytes32::from([1; 32]),
                    royalty_basis_points: 10,
                    p2_puzzle_hash: nft_bls.puzzle_hash,
                    transfer_condition: None,
                },
            )?;

            (manager_coin, manager_singleton_proof) = spend_manager_singleton(
                ctx,
                manager_coin,
                manager_singleton_proof,
                manager_singleton_puzzle,
                conds,
            )?;

            ensure_conditions_met(
                ctx,
                &mut sim,
                Conditions::new().assert_concurrent_spend(nft_launcher_coin.coin_id()),
                1,
            )?;

            let spends = ctx.take();
            benchmark.add_spends(ctx, &mut sim, spends, "mint_nft", &[])?;

            if let Some(some_datastore) = datastore {
                merkle_tree = MerkleTree::new(&[(nft.info.launcher_id, 1).tree_hash().into()]);
                let metadata = DataStoreMetadata {
                    root_hash: merkle_tree.root(),
                    label: Some("label".to_string()),
                    description: None,
                    bytes: None,
                    size_proof: None,
                };
                datastore = Some(update_datastore(
                    ctx,
                    &mut sim,
                    &mut benchmark,
                    some_datastore,
                    &delegated_puzzles,
                    metadata,
                    &datastore_p2,
                )?);
            }

            let nft_inner_spend = Spend::new(
                ctx.alloc(&clvm_quote!(Conditions::new().create_coin(
                    SETTLEMENT_PAYMENT_HASH.into(),
                    1,
                    Memos::None
                )))?,
                NodePtr::NIL,
            );
            let nft_inner_spend =
                StandardLayer::new(nft_bls.pk).delegated_inner_spend(ctx, nft_inner_spend)?;
            let offer_nft = nft.spend(ctx, nft_inner_spend)?;

            let (sec_conds, notarized_payments, locked_nfts) = if let Some(some_datastore) =
                datastore
            {
                let oracle_layer = match delegated_puzzles[0] {
                    DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee) => {
                        OracleLayer::new(oracle_puzzle_hash, oracle_fee).unwrap()
                    }
                    _ => panic!("expected first member of delegated puzzles to be an oracle"),
                };
                let inner_spend = oracle_layer.construct_spend(ctx, ())?;

                let dl_metadata_updater_hash: Bytes32 = 11.tree_hash().into();
                let dl_inner_puzzle_hash = some_datastore.info.delegation_layer_puzzle_hash(ctx)?;

                let dl_spend = some_datastore.spend(ctx, inner_spend)?;
                datastore =
                    Some(DataStore::from_spend(ctx, &dl_spend, &delegated_puzzles)?.unwrap());
                ctx.insert(dl_spend);

                registry
                    .new_action::<RewardDistributorStakeAction>()
                    .spend_for_curated_nft_mode(
                        ctx,
                        &mut registry,
                        &[offer_nft],
                        &[1],
                        &[merkle_tree
                            .proof((nft.info.launcher_id, 1).tree_hash().into())
                            .unwrap()],
                        nft_bls.puzzle_hash,
                        None,
                        merkle_tree.root(),
                        Some(clvm_tuple!(("l", "label"), ()).tree_hash().into()),
                        dl_metadata_updater_hash.tree_hash().into(),
                        dl_inner_puzzle_hash.into(),
                    )?
            } else {
                let Proof::Lineage(did_proof) = manager_singleton_proof else {
                    panic!("did_proof is not a lineage proof");
                };
                let nft_proof = NftLauncherProof {
                    did_proof,
                    intermediary_coin_proofs: vec![IntermediaryCoinProof {
                        full_puzzle_hash: nft_launcher_coin.puzzle_hash,
                        amount: nft_launcher_coin.amount,
                    }],
                };
                registry
                    .new_action::<RewardDistributorStakeAction>()
                    .spend_for_collection_nft_mode(
                        ctx,
                        &mut registry,
                        &[offer_nft],
                        &[nft_proof],
                        nft_bls.puzzle_hash,
                        None,
                    )?
            };
            let entry1_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_entry_slots[0],
                RewardDistributorSlotNonce::ENTRY,
            );
            registry = registry.finish_spend(ctx, vec![])?.0;

            ensure_conditions_met(
                ctx,
                &mut sim,
                sec_conds,
                if datastore.is_some() { oracle_fee } else { 0 },
            )?;

            let nft_inner_spend = Spend::new(
                ctx.alloc_mod::<SettlementPayment>()?,
                ctx.alloc(&SettlementPaymentsSolution { notarized_payments })?,
            );
            let _locked_nft = offer_nft.spend(ctx, nft_inner_spend)?;

            // sim.spend_coins(spends, slice::from_ref(&nft_bls.sk))?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "stake_nft",
                &[nft_bls.sk.clone(), datastore_p2.sk.clone()],
            )?;

            (entry1_slot, Some(locked_nfts[0]))
        };

        // commit incentives for first epoch
        let rewards_to_add = constants.epoch_seconds;
        let secure_conditions = registry
            .new_action::<RewardDistributorCommitIncentivesAction>()
            .spend(
                ctx,
                &mut registry,
                first_epoch_slot,
                first_epoch_start,
                cat_minter.puzzle_hash,
                rewards_to_add,
            )?;
        let first_epoch_commitment_slot = registry.created_slot_value_to_slot(
            registry.pending_spend.created_commitment_slots[0],
            RewardDistributorSlotNonce::COMMITMENT,
        );
        let mut incentive_slots = registry
            .pending_spend
            .created_reward_slots
            .iter()
            .map(|s| registry.created_slot_value_to_slot(*s, RewardDistributorSlotNonce::REWARD))
            .collect::<Vec<_>>();

        // spend reserve and source cat together so deltas add up
        let hint = ctx.hint(cat_minter.puzzle_hash)?;
        let source_cat_spend = CatSpend::new(
            source_cat,
            cat_minter_p2.spend_with_conditions(
                ctx,
                secure_conditions.create_coin(
                    cat_minter.puzzle_hash,
                    source_cat.coin.amount - rewards_to_add,
                    hint,
                ),
            )?,
        );

        registry = registry.finish_spend(ctx, vec![source_cat_spend])?.0;
        // sim.spend_coins(ctx.take(), slice::from_ref(&cat_minter.sk))?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "commit_incentives",
            slice::from_ref(&cat_minter.sk),
        )?;
        source_cat = source_cat.child(
            cat_minter.puzzle_hash,
            source_cat.coin.amount - rewards_to_add,
        );
        assert!(sim
            .coin_state(first_epoch_commitment_slot.coin.coin_id())
            .is_some());
        for incentive_slot in &incentive_slots {
            assert!(sim.coin_state(incentive_slot.coin.coin_id()).is_some());
        }

        // commit incentives for fifth epoch
        let fifth_epoch_start = first_epoch_start + constants.epoch_seconds * 4;
        let rewards_to_add = constants.epoch_seconds * 10;
        let secure_conditions = registry
            .new_action::<RewardDistributorCommitIncentivesAction>()
            .spend(
                ctx,
                &mut registry,
                incentive_slots.last().unwrap().clone(),
                fifth_epoch_start,
                cat_minter.puzzle_hash,
                rewards_to_add,
            )?;
        let fifth_epoch_commitment_slot = registry.created_slot_value_to_slot(
            registry.pending_spend.created_commitment_slots[0],
            RewardDistributorSlotNonce::COMMITMENT,
        );
        let new_incentive_slots = registry
            .pending_spend
            .created_reward_slots
            .iter()
            .map(|s| registry.created_slot_value_to_slot(*s, RewardDistributorSlotNonce::REWARD))
            .collect::<Vec<_>>();

        let new_value_keys = new_incentive_slots
            .iter()
            .map(|s| s.info.value.epoch_start)
            .collect::<Vec<_>>();
        incentive_slots.retain(|s| !new_value_keys.contains(&s.info.value.epoch_start));
        incentive_slots.extend(new_incentive_slots);

        // spend reserve and source cat together so deltas add up
        let source_cat_spend = CatSpend::new(
            source_cat,
            cat_minter_p2.spend_with_conditions(
                ctx,
                secure_conditions.create_coin(
                    cat_minter.puzzle_hash,
                    source_cat.coin.amount - rewards_to_add,
                    Memos::None,
                ),
            )?,
        );

        registry = registry.finish_spend(ctx, vec![source_cat_spend])?.0;
        // sim.spend_coins(ctx.take(), slice::from_ref(&cat_minter.sk))?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "commit_incentives",
            slice::from_ref(&cat_minter.sk),
        )?;

        source_cat = source_cat.child(
            cat_minter.puzzle_hash,
            source_cat.coin.amount - rewards_to_add,
        );
        assert!(sim
            .coin_state(fifth_epoch_commitment_slot.coin.coin_id())
            .is_some());
        for incentive_slot in &incentive_slots {
            assert!(sim.coin_state(incentive_slot.coin.coin_id()).is_some());
        }

        // 2nd commit incentives for fifth epoch
        let rewards_to_add = constants.epoch_seconds * 2;
        let secure_conditions = registry
            .new_action::<RewardDistributorCommitIncentivesAction>()
            .spend(
                ctx,
                &mut registry,
                incentive_slots
                    .iter()
                    .find(|s| s.info.value.epoch_start == fifth_epoch_start)
                    .unwrap()
                    .clone(),
                fifth_epoch_start,
                cat_minter.puzzle_hash,
                rewards_to_add,
            )?;
        let fifth_epoch_commitment_slot2 = registry.created_slot_value_to_slot(
            registry.pending_spend.created_commitment_slots[0],
            RewardDistributorSlotNonce::COMMITMENT,
        );
        let new_incentive_slots = registry
            .pending_spend
            .created_reward_slots
            .iter()
            .map(|s| registry.created_slot_value_to_slot(*s, RewardDistributorSlotNonce::REWARD))
            .collect::<Vec<_>>();

        let new_value_keys = new_incentive_slots
            .iter()
            .map(|s| s.info.value.epoch_start)
            .collect::<Vec<_>>();
        incentive_slots.retain(|s| !new_value_keys.contains(&s.info.value.epoch_start));
        incentive_slots.extend(new_incentive_slots);

        // spend reserve and source cat together so deltas add up
        let source_cat_spend = CatSpend::new(
            source_cat,
            cat_minter_p2.spend_with_conditions(
                ctx,
                secure_conditions.create_coin(
                    cat_minter.puzzle_hash,
                    source_cat.coin.amount - rewards_to_add,
                    Memos::None,
                ),
            )?,
        );

        registry = registry.finish_spend(ctx, vec![source_cat_spend])?.0;
        // sim.spend_coins(ctx.take(), slice::from_ref(&cat_minter.sk))?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "commit_incentives",
            slice::from_ref(&cat_minter.sk),
        )?;

        source_cat = source_cat.child(
            cat_minter.puzzle_hash,
            source_cat.coin.amount - rewards_to_add,
        );
        assert!(sim
            .coin_state(fifth_epoch_commitment_slot2.coin.coin_id())
            .is_some());
        for incentive_slot in &incentive_slots {
            assert!(sim.coin_state(incentive_slot.coin.coin_id()).is_some());
        }
        assert!(sim
            .coin_state(registry.reserve.coin.coin_id())
            .unwrap()
            .spent_height
            .is_none());

        // withdraw the 1st incentives for epoch 5
        let (withdraw_incentives_conditions, withdrawn_amount) = registry
            .new_action::<RewardDistributorWithdrawIncentivesAction>()
            .spend(
                ctx,
                &mut registry,
                fifth_epoch_commitment_slot.clone(),
                incentive_slots
                    .iter()
                    .find(|s| s.info.value.epoch_start == fifth_epoch_start)
                    .unwrap()
                    .clone(),
            )?;
        let new_reward_slot = registry.created_slot_value_to_slot(
            registry.pending_spend.created_reward_slots[0],
            RewardDistributorSlotNonce::REWARD,
        );

        let payout_coin_id = registry
            .reserve
            .to_cat()
            .child(
                cat_minter.puzzle_hash, // fifth_epoch_commitment_slot.info.value.unwrap().clawback_ph,
                withdrawn_amount,
            )
            .coin
            .coin_id();

        let claimer_coin = sim.new_coin(cat_minter.puzzle_hash, 0);
        cat_minter_p2.spend(ctx, claimer_coin, withdraw_incentives_conditions)?;

        registry = registry.finish_spend(ctx, vec![])?.0;
        sim.set_next_timestamp(first_epoch_start)?;
        // sim.spend_coins(ctx.take(), slice::from_ref(&cat_minter.sk))?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "withdraw_incentives",
            slice::from_ref(&cat_minter.sk),
        )?;

        assert!(sim.coin_state(payout_coin_id).is_some());
        assert!(sim
            .coin_state(fifth_epoch_commitment_slot.coin.coin_id())
            .unwrap()
            .spent_height
            .is_some());
        assert!(sim
            .coin_state(new_reward_slot.coin.coin_id())
            .unwrap()
            .spent_height
            .is_none());
        incentive_slots
            .retain(|s| s.info.value.epoch_start != new_reward_slot.info.value.epoch_start);
        incentive_slots.push(new_reward_slot);

        // start first epoch
        let reserve_cat = registry.reserve.to_cat();
        let first_epoch_incentives_slot = incentive_slots
            .iter()
            .find(|s| s.info.value.epoch_start == first_epoch_start)
            .unwrap()
            .clone();
        let (new_epoch_conditions, fee) = registry
            .new_action::<RewardDistributorNewEpochAction>()
            .spend(
                ctx,
                &mut registry,
                first_epoch_incentives_slot.clone(),
                // first_epoch_incentives_slot.info.value.rewards,
            )?;
        let new_reward_slot = registry.created_slot_value_to_slot(
            registry.pending_spend.created_reward_slots[0],
            RewardDistributorSlotNonce::REWARD,
        );
        let payout_coin_id = reserve_cat
            .child(constants.fee_payout_puzzle_hash, fee)
            .coin
            .coin_id();

        ensure_conditions_met(ctx, &mut sim, new_epoch_conditions, 0)?;

        registry = registry.finish_spend(ctx, vec![])?.0;
        sim.pass_time(100);
        // sim.spend_coins(ctx.take(), &[])?;
        let spends = ctx.take();
        benchmark.add_spends(ctx, &mut sim, spends, "new_epoch", &[])?;

        assert!(sim.coin_state(payout_coin_id).is_some());
        assert_eq!(registry.info.state.active_shares, 1);
        assert_eq!(registry.info.state.total_reserves, 4000 - fee);
        assert_eq!(registry.info.state.round_reward_info.cumulative_payout, 0);
        assert_eq!(
            registry.info.state.round_reward_info.remaining_rewards,
            u128::from(first_epoch_incentives_slot.info.value.rewards - fee)
                * u128::from(constants.precision)
        );
        assert_eq!(
            registry.info.state.round_time_info.last_update,
            first_epoch_start
        );
        assert_eq!(
            registry.info.state.round_time_info.epoch_end,
            first_epoch_start + constants.epoch_seconds
        );
        assert!(sim
            .coin_state(first_epoch_incentives_slot.coin.coin_id())
            .unwrap()
            .spent_height
            .is_some());
        assert!(sim
            .coin_state(new_reward_slot.coin.coin_id())
            .unwrap()
            .spent_height
            .is_none());
        incentive_slots
            .retain(|s| s.info.value.epoch_start != new_reward_slot.info.value.epoch_start);
        incentive_slots.push(new_reward_slot);

        // sync to 10%
        let initial_reward_info = registry.info.state.round_reward_info;
        let sync_conditions = registry.new_action::<RewardDistributorSyncAction>().spend(
            ctx,
            &mut registry,
            first_epoch_start + 100,
        )?;
        ensure_conditions_met(ctx, &mut sim, sync_conditions, 0)?;

        registry = registry.finish_spend(ctx, vec![])?.0;
        sim.pass_time(400);
        // sim.spend_coins(ctx.take(), &[])?;
        let spends = ctx.take();
        benchmark.add_spends(ctx, &mut sim, spends, "sync", &[])?;

        assert!(registry.info.state.round_time_info.last_update == first_epoch_start + 100);

        let cumulative_payout_delta = initial_reward_info.remaining_rewards / 10;
        assert_eq!(
            registry.info.state.round_reward_info.remaining_rewards,
            initial_reward_info.remaining_rewards - cumulative_payout_delta
        );
        assert_eq!(
            registry.info.state.round_reward_info.cumulative_payout,
            initial_reward_info.cumulative_payout + cumulative_payout_delta
        );

        // sync to 50% (so + 40%)
        let initial_reward_info = registry.info.state.round_reward_info;
        let sync_conditions = registry.new_action::<RewardDistributorSyncAction>().spend(
            ctx,
            &mut registry,
            first_epoch_start + 500,
        )?;
        ensure_conditions_met(ctx, &mut sim, sync_conditions, 0)?;

        registry = registry.finish_spend(ctx, vec![])?.0;
        // sim.spend_coins(ctx.take(), &[])?;
        let spends = ctx.take();
        benchmark.add_spends(ctx, &mut sim, spends, "sync", &[])?;
        assert!(registry.info.state.round_time_info.last_update == first_epoch_start + 500);

        let cumulative_payout_delta = initial_reward_info.remaining_rewards * 400 / 900;
        assert!(
            registry.info.state.round_reward_info.remaining_rewards
                == initial_reward_info.remaining_rewards - cumulative_payout_delta
        );
        assert!(
            registry.info.state.round_reward_info.cumulative_payout
                == initial_reward_info.cumulative_payout + cumulative_payout_delta
        );

        // add incentives
        let initial_reward_info = registry.info.state.round_reward_info;
        let incentives_amount =
            u64::try_from(initial_reward_info.remaining_rewards / u128::from(constants.precision))
                .unwrap();
        let registry_info = registry.info;

        let add_incentives_conditions = registry
            .new_action::<RewardDistributorAddIncentivesAction>()
            .spend(ctx, &mut registry, incentives_amount)?;

        // spend reserve and source cat together so deltas add up
        let source_cat_spend = CatSpend::new(
            source_cat,
            cat_minter_p2.spend_with_conditions(
                ctx,
                add_incentives_conditions.create_coin(
                    cat_minter.puzzle_hash,
                    source_cat.coin.amount - incentives_amount,
                    Memos::None,
                ),
            )?,
        );

        registry = registry.finish_spend(ctx, vec![source_cat_spend])?.0;
        // sim.spend_coins(ctx.take(), slice::from_ref(&cat_minter.sk))?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "add_incentives",
            slice::from_ref(&cat_minter.sk),
        )?;

        assert_eq!(
            registry.info.state.round_time_info.last_update,
            first_epoch_start + 500
        );
        assert_eq!(
            registry.info.state.round_reward_info.cumulative_payout,
            registry_info.state.round_reward_info.cumulative_payout
        );
        assert_eq!(
            registry.info.state.round_reward_info.remaining_rewards,
            registry_info.state.round_reward_info.remaining_rewards
                + u128::from(incentives_amount - incentives_amount * constants.fee_bps / 10000)
                    * u128::from(constants.precision)
        );
        source_cat = source_cat.child(
            cat_minter.puzzle_hash,
            source_cat.coin.amount - incentives_amount,
        );

        // add second entry OR 2 more NFTs
        let nft2_bls = sim.bls(0);
        let nft3_bls = sim.bls(0);
        let refreshable = if let RewardDistributorTestType::CuratedNft { refreshable } = test_type {
            refreshable
        } else {
            false
        };
        let (mut entry2_slot, other_nft2_info, locked_cat2) = if test_type
            == RewardDistributorTestType::Managed
        {
            let manager_conditions = registry
                .new_action::<RewardDistributorAddEntryAction>()
                .spend(
                    ctx,
                    &mut registry,
                    entry2_bls.puzzle_hash,
                    2,
                    manager_singleton_inner_puzzle_hash,
                )?;
            let entry2_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_entry_slots[0],
                RewardDistributorSlotNonce::ENTRY,
            );

            (manager_coin, manager_singleton_proof) = spend_manager_singleton(
                ctx,
                manager_coin,
                manager_singleton_proof,
                manager_singleton_puzzle,
                manager_conditions,
            )?;

            registry = registry.finish_spend(ctx, vec![])?.0;
            sim.pass_time(250);
            // sim.spend_coins(ctx.take(), &[])?;
            let spends = ctx.take();
            benchmark.add_spends(ctx, &mut sim, spends, "add_entry", &[])?;

            (entry2_slot, None, None)
        } else if test_type == RewardDistributorTestType::Cat {
            let stakeable_cat = source_stakeable_cat.as_ref().unwrap();
            let offered_cat2 = stakeable_cat.child(SETTLEMENT_PAYMENT_HASH.into(), 2);
            let offered_cat3 = stakeable_cat.child(SETTLEMENT_PAYMENT_HASH.into(), 3);
            let (security_conds2, np2, _locked_cat2) = registry
                .new_action::<RewardDistributorStakeAction>()
                .spend_for_cat_mode(ctx, &mut registry, offered_cat2, nft2_bls.puzzle_hash, None)?;
            let entry2_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_entry_slots[0],
                RewardDistributorSlotNonce::ENTRY,
            );

            let (security_conds3, np3, locked_cat3) = registry
                .new_action::<RewardDistributorStakeAction>()
                .spend_for_cat_mode(
                    ctx,
                    &mut registry,
                    offered_cat3,
                    nft2_bls.puzzle_hash,
                    Some(entry2_slot),
                )?;
            let entry2_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_entry_slots[1],
                RewardDistributorSlotNonce::ENTRY,
            );

            registry = registry.finish_spend(ctx, vec![])?.0;

            StandardLayer::new(nft2_bls.pk).spend(
                ctx,
                nft2_bls.coin,
                security_conds2.extend(security_conds3),
            )?;

            let stakeable_cat_delegated_puzzle = ctx.alloc(&clvm_quote!(Conditions::new()
                .create_coin(SETTLEMENT_PAYMENT_HASH.into(), 2, Memos::None)
                .create_coin(SETTLEMENT_PAYMENT_HASH.into(), 3, Memos::None)
                .create_coin(
                    stakeable_cat.p2_puzzle_hash(),
                    stakeable_cat.amount() - 5,
                    Memos::None,
                )))?;
            let stakeable_cat_spend = stakeable_cat_minter_p2.delegated_inner_spend(
                ctx,
                Spend::new(stakeable_cat_delegated_puzzle, NodePtr::NIL),
            )?;

            let offer2_sol = ctx.alloc(&SettlementPaymentsSolution {
                notarized_payments: vec![np2],
            })?;
            let offer3_sol = ctx.alloc(&SettlementPaymentsSolution {
                notarized_payments: vec![np3],
            })?;
            let offered_cat2_spend = Spend::new(ctx.alloc_mod::<SettlementPayment>()?, offer2_sol);
            let offered_cat3_spend = Spend::new(ctx.alloc_mod::<SettlementPayment>()?, offer3_sol);
            let _new_cats = Cat::spend_all(
                ctx,
                &[
                    CatSpend::new(*stakeable_cat, stakeable_cat_spend),
                    CatSpend::new(offered_cat2, offered_cat2_spend),
                    CatSpend::new(offered_cat3, offered_cat3_spend),
                ],
            )?;

            source_stakeable_cat = Some(
                stakeable_cat.child(stakeable_cat.p2_puzzle_hash(), stakeable_cat.amount() - 5),
            );

            sim.pass_time(250);
            // sim.spend_coins(ctx.take(), &[])?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "stake_2_cats",
                &[stakeable_cat_minter.sk.clone(), nft2_bls.sk.clone()],
            )?;
            (entry2_slot, None, Some(locked_cat3))
        } else {
            let nft2_launcher = Launcher::new(manager_coin.coin_id(), 0).with_singleton_amount(1);
            let nft2_launcher_coin = nft2_launcher.coin();
            let meta2 = ctx.alloc(&"nft2")?;
            let meta2 = HashedPtr::from_ptr(ctx, meta2);

            let nft3_launcher = Launcher::new(manager_coin.coin_id(), 2).with_singleton_amount(1);
            let nft3_launcher_coin = nft3_launcher.coin();
            let meta3 = ctx.alloc(&"nft3")?;
            let meta3 = HashedPtr::from_ptr(ctx, meta3);

            let (conds2, nft2) = nft2_launcher.mint_nft(
                ctx,
                &NftMint {
                    metadata: meta2,
                    metadata_updater_puzzle_hash: ANY_METADATA_UPDATER_HASH.into(),
                    royalty_puzzle_hash: Bytes32::from([2; 32]),
                    royalty_basis_points: 12,
                    p2_puzzle_hash: nft2_bls.puzzle_hash,
                    transfer_condition: None,
                },
            )?;

            let (conds3, nft3) = nft3_launcher.mint_nft(
                ctx,
                &NftMint {
                    metadata: meta3,
                    metadata_updater_puzzle_hash: ANY_METADATA_UPDATER_HASH.into(),
                    royalty_puzzle_hash: Bytes32::from([3; 32]),
                    royalty_basis_points: 15,
                    p2_puzzle_hash: nft3_bls.puzzle_hash,
                    transfer_condition: None,
                },
            )?;

            (manager_coin, manager_singleton_proof) = spend_manager_singleton(
                ctx,
                manager_coin,
                manager_singleton_proof,
                manager_singleton_puzzle,
                conds2.extend(conds3),
            )?;

            ensure_conditions_met(
                ctx,
                &mut sim,
                Conditions::new()
                    .assert_concurrent_spend(nft2_launcher_coin.coin_id())
                    .assert_concurrent_spend(nft3_launcher_coin.coin_id()),
                2,
            )?;

            let spends = ctx.take();
            benchmark.add_spends(ctx, &mut sim, spends, "mint_2_nfts", &[])?;

            // update datastore to contain new NFTs if needed
            if let Some(some_datastore) = datastore {
                merkle_tree = MerkleTree::new(&[
                    (nft2.info.launcher_id, 2).tree_hash().into(),
                    (nft3.info.launcher_id, if refreshable { 6 } else { 3 })
                        .tree_hash()
                        .into(),
                ]);
                datastore = Some(update_datastore(
                    ctx,
                    &mut sim,
                    &mut benchmark,
                    some_datastore,
                    &delegated_puzzles,
                    DataStoreMetadata::root_hash_only(merkle_tree.root()),
                    &datastore_p2,
                )?);
            }

            let Proof::Lineage(did_proof) = manager_singleton_proof else {
                panic!("did_proof is not a lineage proof");
            };
            let nft2_proof = NftLauncherProof {
                did_proof,
                intermediary_coin_proofs: vec![IntermediaryCoinProof {
                    full_puzzle_hash: nft2_launcher_coin.puzzle_hash,
                    amount: nft2_launcher_coin.amount,
                }],
            };
            let nft3_proof = NftLauncherProof {
                did_proof,
                intermediary_coin_proofs: vec![IntermediaryCoinProof {
                    full_puzzle_hash: nft3_launcher_coin.puzzle_hash,
                    amount: nft3_launcher_coin.amount,
                }],
            };

            let nfts_inner_spend = Spend::new(
                ctx.alloc(&clvm_quote!(Conditions::new().create_coin(
                    SETTLEMENT_PAYMENT_HASH.into(),
                    1,
                    Memos::None
                )))?,
                NodePtr::NIL,
            );
            let nft2_inner_spend =
                StandardLayer::new(nft2_bls.pk).delegated_inner_spend(ctx, nfts_inner_spend)?;
            let nft3_inner_spend =
                StandardLayer::new(nft3_bls.pk).delegated_inner_spend(ctx, nfts_inner_spend)?;
            let offer2_nft = nft2.spend(ctx, nft2_inner_spend)?;
            let offer3_nft = nft3.spend(ctx, nft3_inner_spend)?;

            let (
                entry2_slot,
                mut entry3_slot,
                locked_nft2,
                mut locked_nft3,
                notarized_payments2,
                notarized_payments3,
                sec_conds,
                mint_mojos,
            ) = if let Some(some_datastore) = datastore {
                let oracle_layer = match delegated_puzzles[0] {
                    DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee) => {
                        OracleLayer::new(oracle_puzzle_hash, oracle_fee).unwrap()
                    }
                    _ => panic!("expected first member of delegated puzzles to be an oracle"),
                };
                let inner_spend = oracle_layer.construct_spend(ctx, ())?;

                let dl_metadata_updater_hash: Bytes32 = 11.tree_hash().into();
                let dl_inner_puzzle_hash = some_datastore.info.delegation_layer_puzzle_hash(ctx)?;

                let dl_spend = some_datastore.spend(ctx, inner_spend)?;
                datastore =
                    Some(DataStore::from_spend(ctx, &dl_spend, &delegated_puzzles)?.unwrap());
                ctx.insert(dl_spend);

                let (sec_conds2, notarized_payments2, locked_nfts2) = registry
                    .new_action::<RewardDistributorStakeAction>()
                    .spend_for_curated_nft_mode(
                        ctx,
                        &mut registry,
                        &[offer2_nft],
                        &[2],
                        &[merkle_tree
                            .proof((nft2.info.launcher_id, 2).tree_hash().into())
                            .unwrap()],
                        nft2_bls.puzzle_hash,
                        None,
                        merkle_tree.root(),
                        None,
                        dl_metadata_updater_hash.tree_hash().into(),
                        dl_inner_puzzle_hash.into(),
                    )?;
                let entry2_slot = registry.created_slot_value_to_slot(
                    registry.pending_spend.created_entry_slots[0],
                    RewardDistributorSlotNonce::ENTRY,
                );

                let (sec_conds3, notarized_payments3, locked_nfts3) = registry
                    .new_action::<RewardDistributorStakeAction>()
                    .spend_for_curated_nft_mode(
                        ctx,
                        &mut registry,
                        &[offer3_nft],
                        &[if refreshable { 6 } else { 3 }],
                        &[merkle_tree
                            .proof(
                                (nft3.info.launcher_id, if refreshable { 6 } else { 3 })
                                    .tree_hash()
                                    .into(),
                            )
                            .unwrap()],
                        nft3_bls.puzzle_hash,
                        None,
                        merkle_tree.root(),
                        None,
                        dl_metadata_updater_hash.tree_hash().into(),
                        dl_inner_puzzle_hash.into(),
                    )?;
                let entry3_slot = registry.created_slot_value_to_slot(
                    registry.pending_spend.created_entry_slots[1],
                    RewardDistributorSlotNonce::ENTRY,
                );

                (
                    entry2_slot,
                    entry3_slot,
                    locked_nfts2[0],
                    locked_nfts3[0],
                    notarized_payments2,
                    notarized_payments3,
                    sec_conds2.extend(sec_conds3),
                    1336,
                )
            } else {
                let (sec_conds2, notarized_payments2, locked_nfts2) = registry
                    .new_action::<RewardDistributorStakeAction>()
                    .spend_for_collection_nft_mode(
                        ctx,
                        &mut registry,
                        &[offer2_nft],
                        &[nft2_proof],
                        nft2_bls.puzzle_hash,
                        None,
                    )?;
                let entry2_slot = registry.created_slot_value_to_slot(
                    registry.pending_spend.created_entry_slots[0],
                    RewardDistributorSlotNonce::ENTRY,
                );
                let (sec_conds3, notarized_payments3, locked_nfts3) = registry
                    .new_action::<RewardDistributorStakeAction>()
                    .spend_for_collection_nft_mode(
                        ctx,
                        &mut registry,
                        &[offer3_nft],
                        &[nft3_proof],
                        nft3_bls.puzzle_hash,
                        None,
                    )?;
                let entry3_slot = registry.created_slot_value_to_slot(
                    registry.pending_spend.created_entry_slots[1],
                    RewardDistributorSlotNonce::ENTRY,
                );

                (
                    entry2_slot,
                    entry3_slot,
                    locked_nfts2[0],
                    locked_nfts3[0],
                    notarized_payments2,
                    notarized_payments3,
                    sec_conds2.extend(sec_conds3),
                    0,
                )
            };
            registry = registry.finish_spend(ctx, vec![])?.0;

            ensure_conditions_met(ctx, &mut sim, sec_conds, mint_mojos)?;

            let nft2_inner_spend = Spend::new(
                ctx.alloc_mod::<SettlementPayment>()?,
                ctx.alloc(&SettlementPaymentsSolution {
                    notarized_payments: notarized_payments2,
                })?,
            );
            let _new_offer2_nft = offer2_nft.spend(ctx, nft2_inner_spend)?;

            let nft3_inner_spend = Spend::new(
                ctx.alloc_mod::<SettlementPayment>()?,
                ctx.alloc(&SettlementPaymentsSolution {
                    notarized_payments: notarized_payments3,
                })?,
            );
            let _new_offer3_nft = offer3_nft.spend(ctx, nft3_inner_spend)?;

            sim.pass_time(250);
            // sim.spend_coins(spends, &[nft2_bls.sk.clone(), nft3_bls.sk.clone()])?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "stake_2_nfts",
                &[nft2_bls.sk.clone(), nft3_bls.sk.clone()],
            )?;

            if refreshable {
                let some_datastore = datastore.unwrap();
                merkle_tree = MerkleTree::new(&[
                    (nft2.info.launcher_id, 2).tree_hash().into(),
                    (nft3.info.launcher_id, 3).tree_hash().into(),
                ]);
                datastore = Some(update_datastore(
                    ctx,
                    &mut sim,
                    &mut benchmark,
                    some_datastore,
                    &delegated_puzzles,
                    DataStoreMetadata::root_hash_only(merkle_tree.root()),
                    &datastore_p2,
                )?);

                let oracle_layer = match delegated_puzzles[0] {
                    DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee) => {
                        OracleLayer::new(oracle_puzzle_hash, oracle_fee).unwrap()
                    }
                    _ => panic!("expected first member of delegated puzzles to be an oracle"),
                };
                let inner_spend = oracle_layer.construct_spend(ctx, ())?;

                let some_datastore = datastore.unwrap();

                let dl_metadata_updater_hash: Bytes32 = 11.tree_hash().into();
                let dl_inner_puzzle_hash = some_datastore.info.delegation_layer_puzzle_hash(ctx)?;

                let dl_spend = some_datastore.spend(ctx, inner_spend)?;
                datastore =
                    Some(DataStore::from_spend(ctx, &dl_spend, &delegated_puzzles)?.unwrap());
                ctx.insert(dl_spend);

                let (sec_conds, new_locked_nfts) = registry
                    .new_action::<RewardDistributorRefreshAction>()
                    .spend(
                        ctx,
                        &mut registry,
                        vec![entry3_slot],
                        &[&[locked_nft3]],
                        &[&[-3]],
                        &[&[3]],
                        &[&[merkle_tree
                            .proof((locked_nft3.info.launcher_id, 3).tree_hash().into())
                            .unwrap()]],
                        merkle_tree.root(),
                        None,
                        dl_metadata_updater_hash.tree_hash().into(),
                        dl_inner_puzzle_hash.into(),
                    )?;
                entry3_slot = registry.created_slot_value_to_slot(
                    registry.pending_spend.created_entry_slots[0],
                    RewardDistributorSlotNonce::ENTRY,
                );
                registry = registry.finish_spend(ctx, vec![])?.0;

                assert_eq!(
                    new_locked_nfts[0].coin.parent_coin_info,
                    locked_nft3.coin.coin_id()
                );
                locked_nft3 = new_locked_nfts[0];
                ensure_conditions_met(ctx, &mut sim, sec_conds, oracle_fee)?;

                let spends = ctx.take();
                benchmark.add_spends(ctx, &mut sim, spends, "refresh", &[])?;

                assert!(sim.coin_state(locked_nft3.coin.coin_id()).is_some());
                assert!(sim
                    .coin_state(locked_nft3.coin.parent_coin_info)
                    .unwrap()
                    .spent_height
                    .is_some());
            }

            (
                entry2_slot,
                Some((entry3_slot, locked_nft2, locked_nft3)),
                None,
            )
        };
        let active_shares = if datastore.is_some() || source_stakeable_cat.is_some() {
            6
        } else {
            3
        };
        assert_eq!(registry.info.state.active_shares, active_shares);

        // sync to 75% (so + 25%)
        let initial_reward_info = registry.info.state.round_reward_info;
        let sync_conditions = registry.new_action::<RewardDistributorSyncAction>().spend(
            ctx,
            &mut registry,
            first_epoch_start + 750,
        )?;
        ensure_conditions_met(ctx, &mut sim, sync_conditions, 0)?;

        registry = registry.finish_spend(ctx, vec![])?.0;
        // sim.spend_coins(ctx.take(), &[])?;
        let spends = ctx.take();
        benchmark.add_spends(ctx, &mut sim, spends, "sync", &[])?;
        assert!(registry.info.state.round_time_info.last_update == first_epoch_start + 750);

        let cumulative_payout_delta =
            initial_reward_info.remaining_rewards * 250 / (u128::from(active_shares) * 500);
        assert!(
            registry.info.state.round_reward_info.remaining_rewards
                == initial_reward_info.remaining_rewards
                    - cumulative_payout_delta * u128::from(active_shares)
        );
        assert!(
            registry.info.state.round_reward_info.cumulative_payout
                == initial_reward_info.cumulative_payout + cumulative_payout_delta
        );

        // remove 2nd entry/the 2 NFTs
        let mut reserve_cat = registry.reserve.to_cat();
        if let Some((mut entry3_slot, mut locked_nft2, mut locked_nft3)) = other_nft2_info {
            if refreshable {
                // if refreshable, refresh NFTs to 0 shares before removing
                // note that we know the non-0 share case works from the non-refreshable
                // test case
                let some_datastore = datastore.unwrap();
                merkle_tree = MerkleTree::new(&[
                    (locked_nft2.info.launcher_id, 0).tree_hash().into(),
                    (locked_nft3.info.launcher_id, 0).tree_hash().into(),
                ]);
                datastore = Some(update_datastore(
                    ctx,
                    &mut sim,
                    &mut benchmark,
                    some_datastore,
                    &delegated_puzzles,
                    DataStoreMetadata::root_hash_only(merkle_tree.root()),
                    &datastore_p2,
                )?);

                let oracle_layer = match delegated_puzzles[0] {
                    DelegatedPuzzle::Oracle(oracle_puzzle_hash, oracle_fee) => {
                        OracleLayer::new(oracle_puzzle_hash, oracle_fee).unwrap()
                    }
                    _ => panic!("expected first member of delegated puzzles to be an oracle"),
                };
                let inner_spend = oracle_layer.construct_spend(ctx, ())?;

                let some_datastore = datastore.unwrap();

                let dl_metadata_updater_hash: Bytes32 = 11.tree_hash().into();
                let dl_inner_puzzle_hash = some_datastore.info.delegation_layer_puzzle_hash(ctx)?;

                let dl_spend = some_datastore.spend(ctx, inner_spend)?;
                datastore =
                    Some(DataStore::from_spend(ctx, &dl_spend, &delegated_puzzles)?.unwrap());
                ctx.insert(dl_spend);

                let (sec_conds, new_locked_nfts) = registry
                    .new_action::<RewardDistributorRefreshAction>()
                    .spend(
                        ctx,
                        &mut registry,
                        vec![entry2_slot, entry3_slot],
                        &[&[locked_nft2], &[locked_nft3]],
                        &[&[-2], &[-3]],
                        &[&[0], &[0]],
                        &[
                            &[merkle_tree
                                .proof((locked_nft2.info.launcher_id, 0).tree_hash().into())
                                .unwrap()],
                            &[merkle_tree
                                .proof((locked_nft3.info.launcher_id, 0).tree_hash().into())
                                .unwrap()],
                        ],
                        merkle_tree.root(),
                        None,
                        dl_metadata_updater_hash.tree_hash().into(),
                        dl_inner_puzzle_hash.into(),
                    )?;
                entry2_slot = registry.created_slot_value_to_slot(
                    registry.pending_spend.created_entry_slots[0],
                    RewardDistributorSlotNonce::ENTRY,
                );
                entry3_slot = registry.created_slot_value_to_slot(
                    registry.pending_spend.created_entry_slots[1],
                    RewardDistributorSlotNonce::ENTRY,
                );
                registry = registry.finish_spend(ctx, vec![])?.0;

                assert_eq!(
                    new_locked_nfts[0].coin.parent_coin_info,
                    locked_nft2.coin.coin_id()
                );
                assert_eq!(
                    new_locked_nfts[1].coin.parent_coin_info,
                    locked_nft3.coin.coin_id()
                );
                locked_nft2 = new_locked_nfts[0];
                locked_nft3 = new_locked_nfts[1];
                ensure_conditions_met(ctx, &mut sim, sec_conds, oracle_fee)?;

                let spends = ctx.take();
                benchmark.add_spends(ctx, &mut sim, spends, "refresh_2_nfts", &[])?;

                assert_eq!(registry.info.state.active_shares, 1);
                assert!(sim.coin_state(locked_nft2.coin.coin_id()).is_some());
                assert!(sim.coin_state(locked_nft3.coin.coin_id()).is_some());
                assert!(sim
                    .coin_state(locked_nft2.coin.parent_coin_info)
                    .unwrap()
                    .spent_height
                    .is_some());
                assert!(sim
                    .coin_state(locked_nft3.coin.parent_coin_info)
                    .unwrap()
                    .spent_height
                    .is_some());

                reserve_cat = registry.reserve.to_cat();
            }

            let nft2_return_coin_id = locked_nft2
                .child(nft2_bls.puzzle_hash, None, locked_nft2.info.metadata, 1)
                .coin
                .coin_id();
            let nft3_return_coin_id = locked_nft3
                .child(nft3_bls.puzzle_hash, None, locked_nft3.info.metadata, 1)
                .coin
                .coin_id();

            let (custody2_conds, payout2_amount) = registry
                .new_action::<RewardDistributorUnstakeAction>()
                .spend_for_locked_nfts(
                    ctx,
                    &mut registry,
                    entry2_slot.clone(),
                    &[locked_nft2],
                    &[if datastore.is_some() {
                        if refreshable {
                            0
                        } else {
                            2
                        }
                    } else {
                        1
                    }],
                )?;
            let (custody3_conds, payout3_amount) = registry
                .new_action::<RewardDistributorUnstakeAction>()
                .spend_for_locked_nfts(
                    ctx,
                    &mut registry,
                    entry3_slot.clone(),
                    &[locked_nft3],
                    &[if datastore.is_some() {
                        if refreshable {
                            0
                        } else {
                            3
                        }
                    } else {
                        1
                    }],
                )?;

            StandardLayer::new(nft2_bls.pk).spend(ctx, nft2_bls.coin, custody2_conds)?;
            StandardLayer::new(nft3_bls.pk).spend(ctx, nft3_bls.coin, custody3_conds)?;

            registry = registry.finish_spend(ctx, vec![])?.0;

            // sim.spend_coins(spends, &[nft2_bls.sk.clone(), nft3_bls.sk.clone()])?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "unstake_2_nfts",
                &[nft2_bls.sk.clone(), nft3_bls.sk.clone()],
            )?;

            let payout_coin_id2 = reserve_cat
                .child(nft2_bls.puzzle_hash, payout2_amount)
                .coin
                .coin_id();
            let payout_coin_id3 = reserve_cat
                .child(nft3_bls.puzzle_hash, payout3_amount)
                .coin
                .coin_id();

            assert!(sim.coin_state(payout_coin_id2).is_some());
            assert!(sim.coin_state(payout_coin_id3).is_some());
            assert!(sim
                .coin_state(entry3_slot.coin.coin_id())
                .unwrap()
                .spent_height
                .is_some());
            assert!(sim.coin_state(nft2_return_coin_id).is_some());
            assert!(sim.coin_state(nft3_return_coin_id).is_some());
        } else if let Some(locked_cat2) = locked_cat2 {
            assert_eq!(locked_cat2.amount(), 3);
            let cat2_return_coin_id = locked_cat2.child(nft2_bls.puzzle_hash, 3).coin.coin_id();

            let (custody2_conds, payout2_amount) = registry
                .new_action::<RewardDistributorUnstakeAction>()
                .spend_for_locked_cats(ctx, &mut registry, entry2_slot.clone(), locked_cat2)?;
            let new_entry2_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_entry_slots[0],
                RewardDistributorSlotNonce::ENTRY,
            );
            assert_eq!(new_entry2_slot.info.value.shares, 2);

            assert_eq!(payout2_amount, 234);
            StandardLayer::new(nft2_bls.pk).spend(
                ctx,
                sim.new_coin(nft2_bls.puzzle_hash, 0),
                custody2_conds,
            )?;

            registry = registry.finish_spend(ctx, vec![])?.0;

            // sim.spend_coins(spends, &[nft2_bls.sk.clone(), nft3_bls.sk.clone()])?;
            let spends = ctx.take();
            benchmark.add_spends(
                ctx,
                &mut sim,
                spends,
                "unstake_cat",
                &[nft2_bls.sk.clone(), nft3_bls.sk.clone()],
            )?;

            let payout_coin_id2 = reserve_cat
                .child(nft2_bls.puzzle_hash, payout2_amount)
                .coin
                .coin_id();

            assert!(sim.coin_state(payout_coin_id2).is_some());
            assert!(sim
                .coin_state(entry2_slot.coin.coin_id())
                .unwrap()
                .spent_height
                .is_some());
            assert!(sim.coin_state(cat2_return_coin_id).is_some());
        } else {
            let (remove_entry_manager_conditions, entry2_payout_amount) = registry
                .new_action::<RewardDistributorRemoveEntryAction>()
                .spend(
                    ctx,
                    &mut registry,
                    entry2_slot.clone(),
                    manager_singleton_inner_puzzle_hash,
                )?;

            let (_manager_coin, _manager_singleton_proof) = spend_manager_singleton(
                ctx,
                manager_coin,
                manager_singleton_proof,
                manager_singleton_puzzle,
                remove_entry_manager_conditions,
            )?;

            registry = registry.finish_spend(ctx, vec![])?.0;
            // sim.spend_coins(ctx.take(), &[])?;
            let spends = ctx.take();
            benchmark.add_spends(ctx, &mut sim, spends, "remove_entry", &[])?;
            let payout_coin_id = reserve_cat
                .child(entry2_bls.puzzle_hash, entry2_payout_amount)
                .coin
                .coin_id();

            assert!(sim.coin_state(payout_coin_id).is_some());
        }
        assert_eq!(
            registry.info.state.active_shares,
            if source_stakeable_cat.is_some() { 3 } else { 1 }
        );
        assert!(sim
            .coin_state(entry2_slot.coin.coin_id())
            .unwrap()
            .spent_height
            .is_some());

        for epoch in 1..7 {
            let update_time = registry.info.state.round_time_info.epoch_end;
            let first_update_time =
                u64::midpoint(registry.info.state.round_time_info.last_update, update_time);
            let sync_conditions1 = registry.new_action::<RewardDistributorSyncAction>().spend(
                ctx,
                &mut registry,
                first_update_time,
            )?;

            let sync_conditions2 = registry.new_action::<RewardDistributorSyncAction>().spend(
                ctx,
                &mut registry,
                update_time,
            )?;

            let reward_slot = incentive_slots
                .iter()
                .find(|s| {
                    s.info.value.epoch_start
                        == first_epoch_start
                            + if epoch <= 4 { epoch } else { 4 } * constants.epoch_seconds
                })
                .unwrap()
                .clone();

            let (new_epoch_conditions, _manager_fee) = registry
                .new_action::<RewardDistributorNewEpochAction>()
                .spend(ctx, &mut registry, reward_slot)?;
            let new_reward_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_reward_slots[0],
                RewardDistributorSlotNonce::REWARD,
            );
            incentive_slots
                .retain(|s| s.info.value.epoch_start != new_reward_slot.info.value.epoch_start);
            incentive_slots.push(new_reward_slot);

            ensure_conditions_met(
                ctx,
                &mut sim,
                sync_conditions1
                    .extend(sync_conditions2)
                    .extend(new_epoch_conditions),
                0,
            )?;

            registry = registry.finish_spend(ctx, vec![])?.0;
            sim.set_next_timestamp(update_time)?;
            // sim.spend_coins(ctx.take(), &[])?;
            let spends = ctx.take();
            benchmark.add_spends(ctx, &mut sim, spends, "sync_and_new_epoch", &[])?;
        }

        // commit incentives for 10th epoch
        let tenth_epoch_start = first_epoch_start + constants.epoch_seconds * 9;
        let rewards_to_add = constants.epoch_seconds * 10;
        let secure_conditions = registry
            .new_action::<RewardDistributorCommitIncentivesAction>()
            .spend(
                ctx,
                &mut registry,
                incentive_slots.last().unwrap().clone(),
                tenth_epoch_start,
                cat_minter.puzzle_hash,
                rewards_to_add,
            )?;
        let tenth_epoch_commitment_slot = registry.created_slot_value_to_slot(
            registry.pending_spend.created_commitment_slots[0],
            RewardDistributorSlotNonce::COMMITMENT,
        );
        let new_incentive_slots = registry
            .pending_spend
            .created_reward_slots
            .iter()
            .map(|s| registry.created_slot_value_to_slot(*s, RewardDistributorSlotNonce::REWARD))
            .collect::<Vec<_>>();

        let new_value_keys = new_incentive_slots
            .iter()
            .map(|s| s.info.value.epoch_start)
            .collect::<Vec<_>>();
        incentive_slots.retain(|s| !new_value_keys.contains(&s.info.value.epoch_start));
        incentive_slots.extend(new_incentive_slots);

        // spend reserve and source cat together so deltas add up
        let source_cat_spend = CatSpend::new(
            source_cat,
            cat_minter_p2.spend_with_conditions(
                ctx,
                secure_conditions.create_coin(
                    cat_minter.puzzle_hash,
                    source_cat.coin.amount - rewards_to_add,
                    Memos::None,
                ),
            )?,
        );

        registry = registry.finish_spend(ctx, vec![source_cat_spend])?.0;
        // sim.spend_coins(ctx.take(), slice::from_ref(&cat_minter.sk))?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "commit_incentives",
            slice::from_ref(&cat_minter.sk),
        )?;
        let _source_cat = source_cat.child(
            cat_minter.puzzle_hash,
            source_cat.coin.amount - rewards_to_add,
        );
        assert!(sim
            .coin_state(tenth_epoch_commitment_slot.coin.coin_id())
            .is_some());
        for incentive_slot in &incentive_slots {
            assert!(sim.coin_state(incentive_slot.coin.coin_id()).is_some());
        }

        for epoch in 7..10 {
            let update_time = registry.info.state.round_time_info.epoch_end;
            let sync_conditions = registry.new_action::<RewardDistributorSyncAction>().spend(
                ctx,
                &mut registry,
                update_time,
            )?;

            let reward_slot = incentive_slots
                .iter()
                .find(|s| {
                    s.info.value.epoch_start == first_epoch_start + epoch * constants.epoch_seconds
                })
                .unwrap()
                .clone();
            let (new_epoch_conditions, _manager_fee) = registry
                .new_action::<RewardDistributorNewEpochAction>()
                .spend(
                    ctx,
                    &mut registry,
                    reward_slot.clone(),
                    // reward_slot.info.value.rewards,
                )?;
            let new_reward_slot = registry.created_slot_value_to_slot(
                registry.pending_spend.created_reward_slots[0],
                RewardDistributorSlotNonce::REWARD,
            );
            incentive_slots
                .retain(|s| s.info.value.epoch_start != new_reward_slot.info.value.epoch_start);
            incentive_slots.push(new_reward_slot);

            ensure_conditions_met(
                ctx,
                &mut sim,
                sync_conditions.extend(new_epoch_conditions),
                0,
            )?;

            registry = registry.finish_spend(ctx, vec![])?.0;
            sim.set_next_timestamp(update_time)?;
            // sim.spend_coins(ctx.take(), &[])?;
            let spends = ctx.take();
            benchmark.add_spends(ctx, &mut sim, spends, "sync", &[])?;
        }

        let update_time = registry.info.state.round_time_info.epoch_end - 100;
        let sync_conditions = registry.new_action::<RewardDistributorSyncAction>().spend(
            ctx,
            &mut registry,
            update_time,
        )?;

        // payout entry
        let reserve_cat = registry.reserve.to_cat();
        let payout_bls = match test_type {
            RewardDistributorTestType::NftCollection
            | RewardDistributorTestType::CuratedNft { refreshable: _ } => nft_bls,
            RewardDistributorTestType::Managed | RewardDistributorTestType::Cat => entry1_bls,
        };
        let payout_puzzle_hash = entry1_slot.info.value.payout_puzzle_hash;
        let (payout_conditions, withdrawal_amount) = registry
            .new_action::<RewardDistributorInitiatePayoutAction>()
            .spend(ctx, &mut registry, entry1_slot)?;
        let coin = sim.new_coin(payout_bls.puzzle_hash, 0);
        StandardLayer::new(payout_bls.pk).spend(
            ctx,
            coin,
            payout_conditions.extend(sync_conditions),
        )?;

        let _registry = registry.finish_spend(ctx, vec![])?.0;

        sim.set_next_timestamp(update_time)?;
        // sim.spend_coins(ctx.take(), &[])?;
        let spends = ctx.take();
        benchmark.add_spends(
            ctx,
            &mut sim,
            spends,
            "initiate_payout",
            std::slice::from_ref(&payout_bls.sk),
        )?;

        let payout_coin_id = reserve_cat
            .child(payout_puzzle_hash, withdrawal_amount)
            .coin
            .coin_id();

        assert!(sim.coin_state(payout_coin_id).is_some());
        // lower payout for curated NFT since the 1st NF receives a 6th
        //  of the rewards afte the other two NFTs have been paid out
        assert_eq!(
            sim.coin_state(payout_coin_id).unwrap().coin.amount,
            if datastore.is_some() {
                12523
            } else if source_stakeable_cat.is_some() {
                4545
            } else {
                12601
            }
        );

        benchmark.print_summary(Some(&format!(
            "{}-reward-distributor.costs",
            match test_type {
                RewardDistributorTestType::Managed => "managed",
                RewardDistributorTestType::NftCollection => "collection-nft",
                RewardDistributorTestType::CuratedNft { refreshable: false } =>
                    "curated-nft-non-refreshable",
                RewardDistributorTestType::CuratedNft { refreshable: true } =>
                    "curated-nft-refreshable",
                RewardDistributorTestType::Cat => "cat",
            }
        )));

        Ok(())
    }
}
