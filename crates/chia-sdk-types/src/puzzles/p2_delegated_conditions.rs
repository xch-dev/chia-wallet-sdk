use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_puzzles::{P2_DELEGATED_CONDITIONS, P2_DELEGATED_CONDITIONS_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{Condition, Mod};

/// Curried arguments for the p2 delegated conditions puzzle, which outputs any conditions signed
/// by a public key.
///
/// When spent, it outputs the conditions from the solution along with an `AGG_SIG_ME` condition
/// that requires `public_key` to sign the tree hash of the conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2DelegatedConditionsArgs {
    /// The public key that must sign the conditions.
    pub public_key: PublicKey,
}

impl P2DelegatedConditionsArgs {
    /// Creates new [`P2DelegatedConditionsArgs`] from the public key.
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl Mod for P2DelegatedConditionsArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_DELEGATED_CONDITIONS)
    }

    fn mod_hash() -> TreeHash {
        P2_DELEGATED_CONDITIONS_HASH.into()
    }
}

/// The solution for the p2 delegated conditions puzzle. See [`P2DelegatedConditionsArgs`].
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2DelegatedConditionsSolution<T = NodePtr> {
    /// The conditions to output, whose tree hash must be signed.
    pub conditions: Vec<Condition<T>>,
}

impl P2DelegatedConditionsSolution {
    /// Creates a new [`P2DelegatedConditionsSolution`] from the conditions to output.
    pub fn new(conditions: Vec<Condition>) -> Self {
        Self { conditions }
    }
}
