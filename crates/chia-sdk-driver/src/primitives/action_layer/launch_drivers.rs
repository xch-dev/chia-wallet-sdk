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
        XchandlesSlotValue,
    },
    Condition, Conditions, Mod,
};
use clvm_traits::{clvm_list, clvm_quote, clvm_tuple, FromClvm, ToClvm};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{
    Cat, CatSpend, CatalogRegistry, CatalogRegistryConstants, CatalogRegistryInfo,
    CatalogRegistryState, DriverError, HashedPtr, Launcher, Layer, Nft, Offer, Reserve,
    RewardDistributor, RewardDistributorConstants, RewardDistributorInfo, RewardDistributorState,
    Slot, SlotProof, Spend, SpendContext, StandardLayer, XchandlesConstants, XchandlesRegistry,
    XchandlesRegistryInfo, XchandlesRegistryState,
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
    left_slot_value: S,
    right_slot_value: S,
    memos_after_hint: NodePtr,
    target_inner_puzzle_hash: Bytes32,
) -> Result<NodePtr, DriverError>
where
    S: ToTreeHash,
{
    let left_slot_info = SlotInfo::from_value(launcher_id, 0, left_slot_value);
    let left_slot_puzzle_hash = Slot::<S>::puzzle_hash(&left_slot_info);

    let right_slot_info = SlotInfo::from_value(launcher_id, 0, right_slot_value);
    let right_slot_puzzle_hash = Slot::<S>::puzzle_hash(&right_slot_info);

    let slot_hint: Bytes32 = Slot::<()>::first_curry_hash(launcher_id, 0).into();
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
fn spend_eve_coin_and_create_registry<S, M, KV>(
    ctx: &mut SpendContext,
    launcher: Launcher,
    target_inner_puzzle_hash: Bytes32,
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

    let slot_proof = SlotProof {
        parent_parent_info: eve_coin.parent_coin_info,
        parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
    };
    let left_slot = Slot::new(
        slot_proof,
        SlotInfo::from_value(launcher_id, 0, left_slot_value),
    );
    let right_slot = Slot::new(
        slot_proof,
        SlotInfo::from_value(launcher_id, 0, right_slot_value),
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
    let security_coin_puzzle_hash: Bytes32 =
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

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
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
        [Slot<XchandlesSlotValue>; 2],
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
            XchandlesSlotValue::initial_left_end(),
            XchandlesSlotValue::initial_right_end(),
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
) -> Result<(Nft<HashedPtr>, Conditions), DriverError> {
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
pub fn launch_dig_reward_distributor(
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

    let controller_singleton_struct_hash: Bytes32 =
        SingletonStruct::new(launcher_id).tree_hash().into();
    let reserve_inner_ph: Bytes32 =
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

    let slot_hint: Bytes32 = first_epoch_start.tree_hash().into();
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
    let new_proof = Proof::Lineage(LineageProof {
        parent_parent_coin_info: eve_coin.parent_coin_info,
        parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
        parent_amount: 1,
    });

    let slot_proof = SlotProof {
        parent_parent_info: eve_coin.parent_coin_info,
        parent_inner_puzzle_hash: eve_singleton_inner_puzzle_hash.into(),
    };
    let slot = Slot::new(slot_proof, slot_info);

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
    let registry = RewardDistributor::new(new_registry_coin, new_proof, target_info, reserve);

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
