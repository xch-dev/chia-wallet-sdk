use std::io;

use chia_consensus::validation_error::ErrorCode;
#[cfg(feature = "serde")]
use chia_protocol::Bytes32;
use chia_sdk_signer::SignerError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimulatorError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Validation error: {0:?}")]
    Validation(ErrorCode),

    #[error("Signer error: {0}")]
    Signer(#[from] SignerError),

    #[error("Missing key")]
    MissingKey,

    #[error(transparent)]
    ChainState(#[from] ChainStateError),

    #[cfg(feature = "serde")]
    #[error(transparent)]
    StateDump(#[from] StateDumpError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ChainStateError {
    #[error("canonical height has no tip header")]
    MissingTipHeader,

    #[error("canonical tip block is missing")]
    MissingTipBlock,

    #[error("block height does not extend the canonical chain")]
    InvalidBlockHeight,

    #[error("block previous hash does not match the canonical tip")]
    InvalidPreviousHash,

    #[error("block timestamp does not match the next timestamp")]
    InvalidBlockTimestamp,

    #[error("block header is already canonical")]
    DuplicateBlockHeader,

    #[error("block is not the canonical tip")]
    BlockIsNotTip,

    #[error("block delta changes a coin more than once")]
    DuplicateCoinChange,

    #[error("block delta changes a coin spend more than once")]
    DuplicateSpendChange,

    #[error("block delta changes a coin hint more than once")]
    DuplicateHintChange,

    #[error("coin state does not match block delta")]
    CoinStateMismatch,

    #[error("coin spend state does not match block delta")]
    CoinSpendStateMismatch,

    #[error("coin hint state does not match block delta")]
    CoinHintStateMismatch,
}

#[cfg(feature = "serde")]
#[derive(Debug, Error)]
pub enum StateDumpError {
    #[error("failed to serialize simulator state: {0}")]
    Serialize(String),

    #[error("failed to deserialize simulator state: {0}")]
    Deserialize(String),

    #[error("missing canonical block {0}")]
    MissingCanonicalBlock(Bytes32),

    #[error("cannot dump state with canonical spend of unsupported manual coin {0}")]
    UnsupportedManualCoinSpend(Bytes32),

    #[error("block key {block_key} does not match record header hash {record_header_hash}")]
    BlockHeaderMismatch {
        block_key: Bytes32,
        record_header_hash: Bytes32,
    },

    #[error("canonical block is missing a timestamp")]
    MissingBlockTimestamp,

    #[error("{0} contain a duplicate key")]
    DuplicateKey(&'static str),

    #[error("previous coin record does not match replayed state for {0}")]
    PreviousCoinRecordMismatch(Bytes32),

    #[error("previous coin records contain an unchanged coin")]
    PreviousCoinRecordForUnchangedCoin,

    #[error("missing final coin record for addition {0}")]
    MissingFinalCoinRecord(Bytes32),

    #[error("added coin record is inconsistent with block {0}")]
    InconsistentAddedCoinRecord(u32),

    #[error("block removes unknown coin {0}")]
    UnknownRemovedCoin(Bytes32),

    #[error("block spends contain a duplicate coin")]
    DuplicateBlockSpend,

    #[error("block spend is not listed as a removal")]
    SpendNotRemoval,

    #[error("coin spend does not match final index for {0}")]
    CoinSpendIndexMismatch(Bytes32),

    #[error("block hint is not for an added coin")]
    HintNotAddition,

    #[error("missing final hint for coin {0}")]
    MissingFinalHint(Bytes32),

    #[error("coin hint does not match canonical transaction for {0}")]
    CoinHintMismatch(Bytes32),

    #[error("block additions do not match canonical transactions")]
    BlockAdditionsMismatch,

    #[error("block removals do not match canonical transactions")]
    BlockRemovalsMismatch,

    #[error("block spends do not match canonical transactions")]
    BlockSpendsMismatch,

    #[error("block hints do not match canonical transactions")]
    BlockHintsMismatch,

    #[error("serialized {0} do not match canonical blocks")]
    SerializedIndexMismatch(&'static str),

    #[error("unsupported full node simulator state format {0}")]
    UnsupportedFormat(String),

    #[error("unsupported full node simulator state version {0}")]
    UnsupportedVersion(u32),

    #[error("height {height} does not match {header_count} header hashes")]
    HeightHeaderCountMismatch { height: u32, header_count: usize },

    #[error("{header_count} canonical headers do not match {block_count} blocks")]
    HeaderBlockCountMismatch {
        header_count: usize,
        block_count: usize,
    },

    #[error("coin record key does not match coin id {0}")]
    CoinRecordKeyMismatch(Bytes32),

    #[error("coin spend key does not match coin id {0}")]
    CoinSpendKeyMismatch(Bytes32),

    #[error("invalid master secret key: {0}")]
    InvalidMasterSecretKey(String),

    #[error("canonical genesis block is missing a timestamp")]
    MissingGenesisTimestamp,

    #[error("canonical block ordering does not match header hashes")]
    CanonicalBlockOrderMismatch,

    #[error("replayed height does not match serialized height")]
    ReplayedHeightMismatch,

    #[error("next timestamp does not follow the canonical blocks")]
    NextTimestampMismatch,
}
