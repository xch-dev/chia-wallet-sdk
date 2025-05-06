#![allow(clippy::wildcard_imports)]

use bindy::Result;
use chia_protocol::Program;
use chia_puzzles::*;
use chia_sdk_types::puzzles::{
    OPTION_CONTRACT_PUZZLE as OPTION_CONTRACT, OPTION_CONTRACT_PUZZLE_HASH as OPTION_CONTRACT_HASH,
    P2_CURRIED_PUZZLE as P2_CURRIED, P2_CURRIED_PUZZLE_HASH as P2_CURRIED_HASH,
};
use clvm_utils::TreeHash;
use paste::paste;

#[derive(Clone)]
pub struct Constants;

macro_rules! puzzle_constants {
    ( $( $lower:ident => $upper:ident, )* ) => {
        paste! {
            impl Constants {
                $( pub fn $lower() -> Result<Program> {
                    Ok($upper.to_vec().into())
                } )*

                $( pub fn [<$lower _hash>]() -> Result<TreeHash> {
                    Ok([<$upper _HASH>].into())
                } )*
            }
        }
    };
}

// Keep this as up to date as possible with chia-puzzles and chia-sdk-types
puzzle_constants! {
    // chia-puzzles
    acs_transfer_program => ACS_TRANSFER_PROGRAM,
    augmented_condition => AUGMENTED_CONDITION,
    block_program_zero => BLOCK_PROGRAM_ZERO,
    cat_puzzle => CAT_PUZZLE,
    chialisp_deserialisation => CHIALISP_DESERIALISATION,
    conditions_w_fee_announce => CONDITIONS_W_FEE_ANNOUNCE,
    covenant_layer => COVENANT_LAYER,
    create_nft_launcher_from_did => CREATE_NFT_LAUNCHER_FROM_DID,
    credential_restriction => CREDENTIAL_RESTRICTION,
    dao_cat_eve => DAO_CAT_EVE,
    dao_cat_launcher => DAO_CAT_LAUNCHER,
    dao_finished_state => DAO_FINISHED_STATE,
    dao_lockup => DAO_LOCKUP,
    dao_proposal => DAO_PROPOSAL,
    dao_proposal_timer => DAO_PROPOSAL_TIMER,
    dao_proposal_validator => DAO_PROPOSAL_VALIDATOR,
    dao_spend_p2_singleton => DAO_SPEND_P2_SINGLETON,
    dao_treasury => DAO_TREASURY,
    dao_update_proposal => DAO_UPDATE_PROPOSAL,
    decompress_coin_spend_entry => DECOMPRESS_COIN_SPEND_ENTRY,
    decompress_coin_spend_entry_with_prefix => DECOMPRESS_COIN_SPEND_ENTRY_WITH_PREFIX,
    decompress_puzzle => DECOMPRESS_PUZZLE,
    delegated_tail => DELEGATED_TAIL,
    did_innerpuzzle => DID_INNERPUZ,
    eml_covenant_morpher => EML_COVENANT_MORPHER,
    eml_transfer_program_covenant_adapter => EML_TRANSFER_PROGRAM_COVENANT_ADAPTER,
    eml_update_metadata_with_did => EML_UPDATE_METADATA_WITH_DID,
    everything_with_signature => EVERYTHING_WITH_SIGNATURE,
    exigent_metadata_layer => EXIGENT_METADATA_LAYER,
    flag_proofs_checker => FLAG_PROOFS_CHECKER,
    genesis_by_coin_id => GENESIS_BY_COIN_ID,
    genesis_by_coin_id_or_singleton => GENESIS_BY_COIN_ID_OR_SINGLETON,
    genesis_by_puzzle_hash => GENESIS_BY_PUZZLE_HASH,
    graftroot_dl_offers => GRAFTROOT_DL_OFFERS,
    nft_intermediate_launcher => NFT_INTERMEDIATE_LAUNCHER,
    nft_metadata_updater_default => NFT_METADATA_UPDATER_DEFAULT,
    nft_metadata_updater_updateable => NFT_METADATA_UPDATER_UPDATEABLE,
    nft_ownership_layer => NFT_OWNERSHIP_LAYER,
    nft_ownership_transfer_program_one_way_claim_with_royalties => NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES,
    nft_state_layer => NFT_STATE_LAYER,
    notification => NOTIFICATION,
    p2_1_of_n => P2_1_OF_N,
    p2_announced_delegated_puzzle => P2_ANNOUNCED_DELEGATED_PUZZLE,
    p2_conditions => P2_CONDITIONS,
    p2_delegated_conditions => P2_DELEGATED_CONDITIONS,
    p2_delegated_puzzle => P2_DELEGATED_PUZZLE,
    p2_delegated_puzzle_or_hidden_puzzle => P2_DELEGATED_PUZZLE_OR_HIDDEN_PUZZLE,
    p2_m_of_n_delegate_direct => P2_M_OF_N_DELEGATE_DIRECT,
    p2_parent => P2_PARENT,
    p2_puzzle_hash => P2_PUZZLE_HASH,
    p2_singleton => P2_SINGLETON,
    p2_singleton_aggregator => P2_SINGLETON_AGGREGATOR,
    p2_singleton_or_delayed_puzzle_hash => P2_SINGLETON_OR_DELAYED_PUZHASH,
    p2_singleton_via_delegated_puzzle => P2_SINGLETON_VIA_DELEGATED_PUZZLE,
    pool_member_innerpuzzle => POOL_MEMBER_INNERPUZ,
    pool_waitingroom_innerpuzzle => POOL_WAITINGROOM_INNERPUZ,
    revocation_layer => REVOCATION_LAYER,
    rom_bootstrap_generator => ROM_BOOTSTRAP_GENERATOR,
    settlement_payment => SETTLEMENT_PAYMENT,
    singleton_launcher => SINGLETON_LAUNCHER,
    singleton_top_layer => SINGLETON_TOP_LAYER,
    singleton_top_layer_v1_1 => SINGLETON_TOP_LAYER_V1_1,
    standard_vc_revocation_puzzle => STANDARD_VC_REVOCATION_PUZZLE,
    std_parent_morpher => STD_PARENT_MORPHER,

    // chia-sdk-types
    option_contract => OPTION_CONTRACT,
    p2_curried => P2_CURRIED,
}
