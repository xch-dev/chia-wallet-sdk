use chia_protocol::Bytes32;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::error::EvalErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("Eval error: {0}")]
    Eval(#[from] EvalErr),

    #[error("To CLVM error: {0}")]
    ToClvm(#[from] ToClvmError),

    #[error("From CLVM error: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("Infinity public key")]
    InfinityPublicKey,

    #[error("Invalid secp key")]
    InvalidSecpKey(#[from] k256::ecdsa::Error),

    #[error("Puzzle reveal hash {found} does not match coin puzzle hash {expected}")]
    PuzzleHashMismatch { expected: Bytes32, found: Bytes32 },

    #[error("AGG_SIG_UNSAFE message ends with AGG_SIG additional data")]
    UnsafeMessageHasAdditionalData,

    #[error("AGG_SIG message length {0} exceeds the maximum of {MAX_AGG_SIG_MESSAGE_LEN} bytes")]
    AggSigMessageTooLong(usize),
}

/// The maximum length of an `AGG_SIG_*` message allowed by consensus.
pub const MAX_AGG_SIG_MESSAGE_LEN: usize = 1024;
