#![allow(clippy::needless_pass_by_value)]
#![allow(missing_debug_implementations)]
#![allow(missing_copy_implementations)]
#![allow(clippy::inherent_to_string)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::return_self_not_must_use)]

mod action_layer;
mod action_system;
mod address;
mod auction;
mod bls;
mod clear_signing;
mod clvm;
mod clvm_types;
mod coin;
mod compact_representation;
mod conditions;
mod constants;
mod convert;
mod eip712;
mod full_node_simulator;
mod key_pairs;
mod merkle_tree;
mod mips;
mod mnemonic;
mod offer;
mod partial_offer;
mod program;
mod puzzle;
mod rpc;
mod runtime;
mod secp;
mod simulator;
mod utils;

pub use action_layer::*;
pub use action_system::*;
pub use address::*;
pub use auction::*;
pub use bls::*;
pub use clear_signing::*;
pub use clvm::*;
pub use clvm_types::*;
pub use coin::*;
pub use compact_representation::*;
pub use conditions::*;
pub use constants::*;
pub use eip712::*;
pub use full_node_simulator::*;
pub use key_pairs::*;
pub use merkle_tree::*;
pub use mips::*;
pub use mnemonic::*;
pub use offer::*;
pub use partial_offer::*;
pub use program::*;
pub use puzzle::*;
pub use rpc::*;
pub use secp::*;
pub use simulator::*;
pub use utils::*;

// Exposed for the bindy_uniffi_sync! macro (C++ backend), which turns async binding
// methods into blocking calls. Other backends use the native async path.
#[cfg(feature = "uniffi")]
pub use runtime::block_on;

#[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
mod peer;

#[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
pub use peer::*;

pub use chia_bls::{PublicKey, SecretKey, Signature};
pub use chia_protocol::{
    BlockRecord, Bytes, Bytes32, ChallengeChainSubSlot, Coin, CoinSpend, CoinState,
    EndOfSubSlotBundle, Foliage, FoliageBlockData, FoliageTransactionBlock, FullBlock,
    InfusedChallengeChainSubSlot, PoolTarget, Program as SerializedProgram, ProofOfSpace,
    RewardChainBlock, RewardChainSubSlot, SpendBundle, SubEpochSummary, SubSlotProofs,
    TransactionsInfo, VDFInfo, VDFProof,
};
pub use chia_puzzle_types::{LineageProof, nft::NftMetadata};
pub use chia_sdk_coinset::{
    AdditionsAndRemovalsResponse, BlockchainState, BlockchainStateResponse, CoinRecord,
    GetBlockRecordByHeightResponse, GetBlockRecordResponse, GetBlockRecordsResponse,
    GetBlockResponse, GetBlockSpendsResponse, GetBlocksResponse, GetCoinRecordResponse,
    GetCoinRecordsResponse, GetMempoolItemResponse, GetMempoolItemsResponse,
    GetNetworkInfoResponse, GetPuzzleAndSolutionResponse, MempoolItem, MempoolMinFees,
    PushTxResponse, SyncState,
};
pub use chia_sdk_driver::{
    AuctionBidVerifier, AuctionInfo, AuctionLaunch, AuctionReserve, AuctionSettings, Bulletin,
    BulletinMessage, Cat, CatAssetInfo, CatInfo, CatalogDelegatedStateActionLog,
    CatalogRefundActionLog, CatalogRegisterActionLog, CatalogRegistryConstants,
    CatalogRegistryState, Clawback, ClawbackPath, ClawbackV2, DatastoreMetadata, DelegatedPuzzle,
    Delta, DropCoin, MedievalVaultHint, MedievalVaultInfo, MetadataUpdate, OptionAssetInfo,
    OptionInfo, OptionMetadata, OptionType, OptionUnderlying, P2ConditionsOrSingleton,
    P2ConditionsOrSingletonVersion, P2ParentCoin, PartialOfferAsset,
    RewardDistributorAddEntryActionLog, RewardDistributorAddIncentivesActionLog,
    RewardDistributorCommitIncentivesActionLog, RewardDistributorConstants,
    RewardDistributorInitiatePayoutActionLog, RewardDistributorNewEpochActionLog,
    RewardDistributorNftStakeEntry, RewardDistributorRefreshNftsFromDlActionLog,
    RewardDistributorRemoveEntryActionLog, RewardDistributorStakeActionLog, RewardDistributorState,
    RewardDistributorStateTransition, RewardDistributorSyncActionLog,
    RewardDistributorUnstakeActionLog, RewardDistributorWithdrawIncentivesActionLog,
    RoundRewardInfo, RoundTimeInfo, RoyaltyInfo, StreamedAsset, StreamingPuzzleInfo, UriKind,
    VaultInfo, VaultMessageProof, VaultOutput, VaultVersion, XchandlesConstants,
    XchandlesDelegatedStateActionLog, XchandlesExecuteUpdateActionLog, XchandlesExpireActionLog,
    XchandlesExtendActionLog, XchandlesInitiateUpdateActionLog, XchandlesOracleActionLog,
    XchandlesPrecommitValueLog, XchandlesRefundActionLog, XchandlesRegisterActionLog,
    XchandlesRegistryState,
};
pub use chia_sdk_types::puzzles::HandleNftMetadata;
pub use chia_sdk_types::puzzles::{CompactCoinProof, CompactLineageProof};
pub use chia_sdk_types::{
    MerkleProof,
    conditions::TradePrice,
    puzzles::{
        AuctionBid, AuctionBpsPayment, AuctionPayments, AuctionState, AuctionTimings,
        CatalogSlotValue, IntermediaryCoinProof, NftLauncherProof,
        RewardDistributorCommitmentSlotValue, RewardDistributorDepositSlotValue,
        RewardDistributorEntrySlotValue, RewardDistributorRewardSlotValue, SlotNeigborsInfo,
        XchandlesHandleSlotValue, XchandlesPricingSolution, XchandlesUpdateSlotValue,
    },
};

#[cfg(any(feature = "napi", feature = "pyo3", feature = "uniffi"))]
pub use chia_protocol::{
    CoinStateUpdate, NewPeakWallet, PuzzleSolutionResponse, RespondCoinState, RespondPuzzleState,
};

pub(crate) use convert::{AsProgram, AsPtr, conditions_to_programs};
