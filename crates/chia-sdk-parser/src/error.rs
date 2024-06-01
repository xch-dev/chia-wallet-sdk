use clvm_traits::FromClvmError;
use clvmr::reduction::EvalErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("failed to parse clvm value: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("failed to evaluate puzzle and solution: {0}")]
    Eval(#[from] EvalErr),

    #[error("invalid mod hash")]
    InvalidModHash,

    #[error("non-standard inner puzzle layer")]
    NonStandardLayer,

    #[error("missing child")]
    MissingChild,

    #[error("missing hint")]
    MissingHint,

    #[error("invalid singleton struct")]
    InvalidSingletonStruct,

    #[error("mismatched singleton output (maybe no spend revealed the new singleton state)")]
    MismatchedOutput,
}
