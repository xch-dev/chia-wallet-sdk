use std::io;

use chia_consensus::validation_error::ErrorCode;
#[cfg(feature = "serde")]
use chia_protocol::Bytes32;
use chia_sdk_signer::SignerError;
use thiserror::Error;

/// An error returned by the [`Simulator`](crate::Simulator) and
/// [`FullNodeSimulator`](crate::FullNodeSimulator).
#[derive(Debug, Error)]
pub enum SimulatorError {
    /// An I/O error.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// The spend bundle was rejected with the given consensus error code. This covers invalid
    /// spends, as well as double spends and mempool conflicts.
    #[error("Validation error: {0:?}")]
    Validation(ErrorCode),

    /// The required signatures couldn't be calculated from the coin spends.
    #[error("Signer error: {0}")]
    Signer(#[from] SignerError),

    /// A required BLS signature uses a public key that doesn't match any of the secret keys passed
    /// to [`sign_transaction`](crate::sign_transaction).
    #[error("Missing key")]
    MissingKey,

    /// The [`FullNodeSimulator`](crate::FullNodeSimulator) chain state failed an internal
    /// consistency check.
    #[error(transparent)]
    ChainState(#[from] ChainStateError),

    /// A [`FullNodeSimulator`](crate::FullNodeSimulator) state dump couldn't be created or
    /// restored.
    #[cfg(feature = "serde")]
    #[error(transparent)]
    StateDump(#[from] StateDumpError),
}

/// A consistency check failed while applying or reverting a block in the
/// [`FullNodeSimulator`](crate::FullNodeSimulator).
///
/// These errors indicate a bug or a corrupted state, not an invalid transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ChainStateError {
    /// There is no header hash for the tip of the canonical chain.
    #[error("canonical height has no tip header")]
    MissingTipHeader,

    /// The block for the tip header hash is missing.
    #[error("canonical tip block is missing")]
    MissingTipBlock,

    /// The block's height isn't one more than the current height.
    #[error("block height does not extend the canonical chain")]
    InvalidBlockHeight,

    /// The block's previous header hash isn't the current tip.
    #[error("block previous hash does not match the canonical tip")]
    InvalidPreviousHash,

    /// The block's timestamp isn't the next timestamp.
    #[error("block timestamp does not match the next timestamp")]
    InvalidBlockTimestamp,

    /// A block with the same header hash is already in the chain.
    #[error("block header is already canonical")]
    DuplicateBlockHeader,

    /// The block being reverted isn't the current tip.
    #[error("block is not the canonical tip")]
    BlockIsNotTip,

    /// The block's changes list the same coin record more than once.
    #[error("block delta changes a coin more than once")]
    DuplicateCoinChange,

    /// The block's changes list the same coin spend more than once.
    #[error("block delta changes a coin spend more than once")]
    DuplicateSpendChange,

    /// The block's changes list the same coin hint more than once.
    #[error("block delta changes a coin hint more than once")]
    DuplicateHintChange,

    /// A coin record doesn't match the value the block's changes expect.
    #[error("coin state does not match block delta")]
    CoinStateMismatch,

    /// A coin spend doesn't match the value the block's changes expect.
    #[error("coin spend state does not match block delta")]
    CoinSpendStateMismatch,

    /// A coin hint doesn't match the value the block's changes expect.
    #[error("coin hint state does not match block delta")]
    CoinHintStateMismatch,
}

/// An error from [`FullNodeSimulator::dump_state`](crate::FullNodeSimulator::dump_state) or
/// [`FullNodeSimulator::restore_state`](crate::FullNodeSimulator::restore_state).
///
/// Restoring replays every dumped block and checks that the result matches the dumped indexes,
/// so most variants describe a dump that is inconsistent with itself.
#[cfg(feature = "serde")]
#[derive(Debug, Error)]
pub enum StateDumpError {
    /// The state couldn't be serialized to JSON.
    #[error("failed to serialize simulator state: {0}")]
    Serialize(String),

    /// The state couldn't be parsed from JSON.
    #[error("failed to deserialize simulator state: {0}")]
    Deserialize(String),

    /// A canonical header hash has no block.
    #[error("missing canonical block {0}")]
    MissingCanonicalBlock(Bytes32),

    /// A block spends a coin that was added with
    /// [`FullNodeSimulator::insert_coin`](crate::FullNodeSimulator::insert_coin) or
    /// [`FullNodeSimulator::new_coin`](crate::FullNodeSimulator::new_coin). Such coins aren't
    /// created by any block, so the dump can't replay their spend.
    #[error("cannot dump state with canonical spend of unsupported manual coin {0}")]
    UnsupportedManualCoinSpend(Bytes32),

    /// A dumped block's header hash doesn't match the header hash in its block record.
    #[error("block key {block_key} does not match record header hash {record_header_hash}")]
    BlockHeaderMismatch {
        /// The header hash the block is listed under.
        block_key: Bytes32,
        /// The header hash in the block record.
        record_header_hash: Bytes32,
    },

    /// A dumped block record has no timestamp.
    #[error("canonical block is missing a timestamp")]
    MissingBlockTimestamp,

    /// A dumped list has the same key more than once. The value names the list.
    #[error("{0} contain a duplicate key")]
    DuplicateKey(&'static str),

    /// A block's previous coin record doesn't match the replayed state for the coin.
    #[error("previous coin record does not match replayed state for {0}")]
    PreviousCoinRecordMismatch(Bytes32),

    /// A block has a previous coin record for a coin that the block doesn't change.
    #[error("previous coin records contain an unchanged coin")]
    PreviousCoinRecordForUnchangedCoin,

    /// A block adds a coin that has no coin record in the dump.
    #[error("missing final coin record for addition {0}")]
    MissingFinalCoinRecord(Bytes32),

    /// A coin record for a coin added at the given height doesn't match that block.
    #[error("added coin record is inconsistent with block {0}")]
    InconsistentAddedCoinRecord(u32),

    /// A block removes a coin that doesn't exist.
    #[error("block removes unknown coin {0}")]
    UnknownRemovedCoin(Bytes32),

    /// A block spends the same coin more than once.
    #[error("block spends contain a duplicate coin")]
    DuplicateBlockSpend,

    /// A block has a coin spend for a coin it doesn't remove.
    #[error("block spend is not listed as a removal")]
    SpendNotRemoval,

    /// A block's coin spend doesn't match the dumped coin spend for the coin.
    #[error("coin spend does not match final index for {0}")]
    CoinSpendIndexMismatch(Bytes32),

    /// A block hints a coin that it doesn't add.
    #[error("block hint is not for an added coin")]
    HintNotAddition,

    /// A block hints a coin that has no hint in the dump.
    #[error("missing final hint for coin {0}")]
    MissingFinalHint(Bytes32),

    /// A coin's dumped hint doesn't match the hint from the block's transactions.
    #[error("coin hint does not match canonical transaction for {0}")]
    CoinHintMismatch(Bytes32),

    /// A block's additions don't match the result of replaying its transactions.
    #[error("block additions do not match canonical transactions")]
    BlockAdditionsMismatch,

    /// A block's removals don't match the result of replaying its transactions.
    #[error("block removals do not match canonical transactions")]
    BlockRemovalsMismatch,

    /// A block's coin spends don't match the result of replaying its transactions.
    #[error("block spends do not match canonical transactions")]
    BlockSpendsMismatch,

    /// A block's hints don't match the result of replaying its transactions.
    #[error("block hints do not match canonical transactions")]
    BlockHintsMismatch,

    /// A dumped index doesn't match the state replayed from the blocks. The value names the index.
    #[error("serialized {0} do not match canonical blocks")]
    SerializedIndexMismatch(&'static str),

    /// The dump's format identifier isn't supported.
    #[error("unsupported full node simulator state format {0}")]
    UnsupportedFormat(String),

    /// The dump's version isn't supported.
    #[error("unsupported full node simulator state version {0}")]
    UnsupportedVersion(u32),

    /// The dumped height doesn't equal the number of header hashes.
    #[error("height {height} does not match {header_count} header hashes")]
    HeightHeaderCountMismatch {
        /// The dumped height.
        height: u32,
        /// The number of dumped header hashes.
        header_count: usize,
    },

    /// The number of header hashes doesn't equal the number of blocks.
    #[error("{header_count} canonical headers do not match {block_count} blocks")]
    HeaderBlockCountMismatch {
        /// The number of dumped header hashes.
        header_count: usize,
        /// The number of dumped blocks.
        block_count: usize,
    },

    /// A dumped coin record is keyed by an id other than its coin id.
    #[error("coin record key does not match coin id {0}")]
    CoinRecordKeyMismatch(Bytes32),

    /// A dumped coin spend is keyed by an id other than its coin id.
    #[error("coin spend key does not match coin id {0}")]
    CoinSpendKeyMismatch(Bytes32),

    /// The dumped master secret key isn't valid.
    #[error("invalid master secret key: {0}")]
    InvalidMasterSecretKey(String),

    /// The genesis block record has no timestamp.
    #[error("canonical genesis block is missing a timestamp")]
    MissingGenesisTimestamp,

    /// The dumped blocks aren't in the same order as the header hashes.
    #[error("canonical block ordering does not match header hashes")]
    CanonicalBlockOrderMismatch,

    /// The height after replaying the blocks doesn't match the dumped height.
    #[error("replayed height does not match serialized height")]
    ReplayedHeightMismatch,

    /// The dumped next timestamp is earlier than the timestamp of the last block.
    #[error("next timestamp does not follow the canonical blocks")]
    NextTimestampMismatch,
}
