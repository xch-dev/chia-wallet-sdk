use chia_protocol::Bytes32;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::error::EvalErr;
use thiserror::Error;

/// An error that occurs while calculating the signatures required by coin spends.
#[derive(Debug, Error)]
pub enum SignerError {
    /// Running the puzzle with its solution failed, for example because it raised an
    /// exception or exceeded the maximum cost.
    #[error("Eval error: {0}")]
    Eval(#[from] EvalErr),

    /// The puzzle reveal or solution couldn't be loaded into the [`Allocator`](clvmr::Allocator).
    #[error("To CLVM error: {0}")]
    ToClvm(#[from] ToClvmError),

    /// The puzzle's output couldn't be parsed as a list of conditions.
    #[error("From CLVM error: {0}")]
    FromClvm(#[from] FromClvmError),

    /// An `AGG_SIG_*` condition used the infinity public key, which consensus rejects.
    #[error("Infinity public key")]
    InfinityPublicKey,

    /// A secp256k1 or secp256r1 key was invalid.
    #[error("Invalid secp key")]
    InvalidSecpKey(#[from] k256::ecdsa::Error),

    /// The tree hash of the puzzle reveal didn't match the puzzle hash of the coin being spent.
    #[error("Puzzle reveal hash {found} does not match coin puzzle hash {expected}")]
    PuzzleHashMismatch {
        /// The puzzle hash of the coin being spent.
        expected: Bytes32,
        /// The tree hash of the puzzle reveal.
        found: Bytes32,
    },

    /// An `AGG_SIG_UNSAFE` message ends with one of the [`AggSigConstants`](crate::AggSigConstants)
    /// values, so a signature for it could be reused for a different `AGG_SIG_*` condition.
    /// Consensus rejects these conditions.
    #[error("AGG_SIG_UNSAFE message ends with AGG_SIG additional data")]
    UnsafeMessageHasAdditionalData,

    /// An `AGG_SIG_*` message was longer than [`MAX_AGG_SIG_MESSAGE_LEN`] bytes. Contains the
    /// length of the message.
    #[error("AGG_SIG message length {0} exceeds the maximum of {MAX_AGG_SIG_MESSAGE_LEN} bytes")]
    AggSigMessageTooLong(usize),
}

/// The maximum length of an `AGG_SIG_*` message allowed by consensus.
pub const MAX_AGG_SIG_MESSAGE_LEN: usize = 1024;
