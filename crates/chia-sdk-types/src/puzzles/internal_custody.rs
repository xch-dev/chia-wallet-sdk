//! CLVM types for the reference prefarm/internal-custody puzzles. These are distinct
//! from MIPS vaults and `ClawbackV2`. No wallet request format or trust policy lives here.
use crate::{MerkleProof, Mod};
use chia_protocol::Bytes32;
use chia_puzzle_types::singleton::SingletonStruct;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use std::borrow::Cow;
mod mods;
pub use mods::*;

/// The pair of child-puzzle commitments in prefarm STATE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyPuzzleHashes {
    /// Hash of the unbound rekey drop puzzle.
    pub rekey: Bytes32,
    /// Hash of the unbound ACH drop puzzle.
    #[clvm(rest)]
    pub payment: Bytes32,
}
/// Withdrawal and rekey timing parameters in the prefarm STATE tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyTimelocks {
    /// Withdrawal timelock in seconds.
    pub withdrawal: u64,
    /// Base rekey timelock in seconds.
    pub rekey_increment: u64,
    /// Additional penalty in seconds for slower rekey paths.
    #[clvm(rest)]
    pub slow_rekey: u64,
}
/// Puzzle commitments and timelocks curried into the prefarm singleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyState {
    /// Child puzzle commitments.
    pub puzzles: InternalCustodyPuzzleHashes,
    /// Withdrawal and rekey timing parameters.
    #[clvm(rest)]
    pub timelocks: InternalCustodyTimelocks,
}
/// Authorized rekey puzzle and timelock multiple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyRekeyFilter {
    /// Unbound drop-puzzle hash enforced by this filter.
    pub puzzle_hash: Bytes32,
    /// Authorized rekey timelock multiple.
    #[clvm(rest)]
    pub allowed_timelock: u64,
}
/// ACH puzzle and clawback-return commitments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyPaymentFilter {
    /// Unbound drop-puzzle hash enforced by this filter.
    pub puzzle_hash: Bytes32,
    /// Puzzle hash receiving clawed-back funds.
    #[clvm(rest)]
    pub clawback_puzzle_hash: Bytes32,
}
/// Rekey and payment restrictions applied to the delegated conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyFilterConfig {
    /// Rekey branch configuration.
    pub rekey: InternalCustodyRekeyFilter,
    /// Payment branch configuration.
    #[clvm(rest)]
    pub payment: InternalCustodyPaymentFilter,
}

/// Curried prefarm configuration: (rekey-hash . payment-hash) and timelocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyPrefarmArgs {
    /// Self-commitment to the prefarm module hash.
    pub mod_hash: Bytes32,
    /// Merkle root committing to the authorized custody puzzles.
    pub puzzle_root: Bytes32,
    /// Configuration committed by this puzzle.
    pub config: InternalCustodyState,
}

/// Curried Merkle-gated puzzle used by ACH and rekey branches.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyDropArgs<I> {
    /// Inner puzzle to execute after proving inclusion.
    pub inner_puzzle: I,
    /// Merkle root authorizing the revealed puzzle.
    pub merkle_root: Bytes32,
}

/// Both authorization filters use an empty `CURRIED_ARGS` argument; they must not be
/// confused with the two-argument ACH/rekey Merkle puzzle.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyFilterArgs<I> {
    /// Inner puzzle to execute after proving inclusion.
    pub inner_puzzle: I,
    /// Merkle root authorizing the revealed puzzle.
    pub merkle_root: Bytes32,
    /// Empty `CURRIED_ARGS` required by both authorization filters.
    pub curried_args: (),
}

/// Curried configuration for the payment/rekey restriction filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyRekeyAndPaymentArgs {
    /// Configuration committed by this puzzle.
    pub config: InternalCustodyFilterConfig,
}

/// Curried relative delay for ACH completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyAchCompletionArgs {
    /// Relative completion delay in seconds.
    pub timelock: u64,
}
/// Curried return destination for ACH clawback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyAchClawbackArgs {
    /// Puzzle hash receiving clawed-back funds.
    pub return_puzzle_hash: Bytes32,
}
/// Curried singleton identity and relative rekey delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyRekeyCompletionArgs {
    /// Singleton identity and top-layer commitments.
    pub singleton_struct: SingletonStruct,
    /// Relative completion delay in seconds.
    pub timelock: u64,
}
/// Uncurried rekey cancellation module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalCustodyRekeyClawback;

impl Mod for InternalCustodyPrefarmArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_PREFARM_INNER)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_PREFARM_INNER_HASH.into()
    }
}
impl<I> Mod for InternalCustodyDropArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_P2_MERKLE_TREE)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_P2_MERKLE_TREE_HASH.into()
    }
}
impl<I> Mod for InternalCustodyFilterArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_P2_MERKLE_TREE)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_P2_MERKLE_TREE_HASH.into()
    }
}
impl Mod for InternalCustodyRekeyAndPaymentArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_REKEY_AND_PAYMENT)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_REKEY_AND_PAYMENT_HASH.into()
    }
}
impl Mod for InternalCustodyAchCompletionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_ACH_COMPLETION)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_ACH_COMPLETION_HASH.into()
    }
}
impl Mod for InternalCustodyAchClawbackArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_ACH_CLAWBACK)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_ACH_CLAWBACK_HASH.into()
    }
}
impl Mod for InternalCustodyRekeyCompletionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_REKEY_COMPLETION)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_REKEY_COMPLETION_HASH.into()
    }
}
impl Mod for InternalCustodyRekeyClawback {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_REKEY_CLAWBACK)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_REKEY_CLAWBACK_HASH.into()
    }
}

/// Filter permitting only the committed rekey branch and timelock multiple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyOnlyRekeyArgs {
    /// Rekey puzzle hash and authorized timelock multiple.
    pub config: InternalCustodyRekeyFilter,
}
/// Dedicated higher-threshold authorization for the precommitted next root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct InternalCustodyLockArgs {
    /// Unsynthesized aggregate public key.
    pub public_key: chia_bls::PublicKey,
    /// Rekey puzzle commitment.
    pub rekey_puzzle_hash: Bytes32,
    /// Precommitted root of the next security level.
    pub next_root: Bytes32,
}
impl Mod for InternalCustodyOnlyRekeyArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_ONLY_REKEY)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_ONLY_REKEY_HASH.into()
    }
}
impl Mod for InternalCustodyLockArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&INTERNAL_CUSTODY_NEW_LOCK_LEVEL)
    }
    fn mod_hash() -> TreeHash {
        INTERNAL_CUSTODY_NEW_LOCK_LEVEL_HASH.into()
    }
}

/// Action arguments for starting or finishing a rekey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyRekeyArguments {
    /// Zero for a lock, one for a normal rekey, larger for slow recovery.
    pub timelock_multiple: u8,
    /// New singleton authorization root.
    pub new_root: Bytes32,
}
/// State committed by a pending zero-value rekey coin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyRekeyState {
    /// Proposed root.
    pub new_root: Bytes32,
    /// Root authorizing this attempt.
    pub old_root: Bytes32,
    /// Committed delay multiple; positive values must cover the authorization
    /// filter's minimum. Zero is reserved for security increases.
    pub timelock_multiple: u8,
}
/// Authorization arguments for cancelling a pending ACH payment.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyAchClawbackSolution<P, S> {
    /// Full pending coin amount.
    pub amount: u64,
    /// Authorized filter reveal.
    pub puzzle: P,
    /// Filter membership under the committed policy root.
    pub proof: MerkleProof,
    /// Filter solution.
    pub solution: S,
}
/// Authorization arguments for cancelling an ordinary or slow rekey.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyRekeyClawbackSolution<P, S> {
    /// Exact pending puzzle hash being cancelled.
    pub puzzle_hash: Bytes32,
    /// Authorized filter reveal.
    pub puzzle: P,
    /// Filter membership under the committed old root.
    pub proof: MerkleProof,
    /// Filter solution.
    pub solution: S,
}

/// Discriminator encoded as an atom in the reference prefarm solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[repr(u8)]
#[clvm(atom)]
pub enum InternalCustodySpendType {
    /// Finish a pending rekey or lock.
    FinishRekey = 0,
    /// Initiate a rekey or lock.
    StartRekey = 1,
    /// Withdraw, absorb deposits, or recreate the singleton unchanged.
    HandlePayment = 2,
}

/// State curried into an ACH (Automated Clearing House) pending payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyAchState {
    /// Policy root authorizing cancellation.
    pub puzzle_root: Bytes32,
    /// Recipient puzzle hash used by completion.
    #[clvm(rest)]
    pub destination: Bytes32,
}

/// Raw prefarm solution. The action arguments depend on `spend_type`; clear
/// signing must explicitly recognize an operation before interpreting them.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodySpendSolution<A, F, S, Q> {
    /// Input coin amount in mojos.
    pub amount: u64,
    /// Operation selecting the action argument layout.
    pub spend_type: InternalCustodySpendType,
    /// Arguments specific to the selected operation.
    pub action: A,
    /// Authorized filter puzzle reveal.
    pub filter_puzzle: F,
    /// Filter inclusion proof under the custody root.
    pub filter_proof: Q,
    /// Solution passed to the filter puzzle.
    pub filter_solution: S,
}
/// Incoming, outgoing and recipient arguments for payment handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyPaymentArguments {
    /// Outgoing ACH amount in mojos.
    pub outgoing: u64,
    /// Absorbed deposit amount in mojos.
    pub incoming: u64,
    /// Recipient puzzle hash committed by the outgoing payment.
    pub destination: Bytes32,
}
/// Revealed puzzle, inclusion proof and solution for a Merkle gate.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct InternalCustodyMerkleSolution<P, S> {
    /// Puzzle reveal whose inclusion must be proven.
    pub puzzle: P,
    /// Merkle inclusion path for the leaf.
    pub proof: MerkleProof,
    /// Solution passed to the revealed puzzle.
    pub solution: S,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clvmr::Allocator;

    #[test]
    fn pinned_module_hashes_match_reveals() -> anyhow::Result<()> {
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_ONLY_REKEY => INTERNAL_CUSTODY_ONLY_REKEY_HASH);
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_NEW_LOCK_LEVEL => INTERNAL_CUSTODY_NEW_LOCK_LEVEL_HASH);
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_PREFARM_INNER => INTERNAL_CUSTODY_PREFARM_INNER_HASH);
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_P2_MERKLE_TREE => INTERNAL_CUSTODY_P2_MERKLE_TREE_HASH);
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_REKEY_AND_PAYMENT => INTERNAL_CUSTODY_REKEY_AND_PAYMENT_HASH);
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_ACH_COMPLETION => INTERNAL_CUSTODY_ACH_COMPLETION_HASH);
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_ACH_CLAWBACK => INTERNAL_CUSTODY_ACH_CLAWBACK_HASH);
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_REKEY_COMPLETION => INTERNAL_CUSTODY_REKEY_COMPLETION_HASH);
        crate::assert_puzzle_hash!(INTERNAL_CUSTODY_REKEY_CLAWBACK => INTERNAL_CUSTODY_REKEY_CLAWBACK_HASH);
        Ok(())
    }
    #[test]
    fn serialized_module_hashes_match_notice() {
        for (name, bytes) in [
            (
                "singleton_top_layer_v1_1",
                &chia_puzzles::SINGLETON_TOP_LAYER_V1_1[..],
            ),
            ("prefarm_inner", &INTERNAL_CUSTODY_PREFARM_INNER[..]),
            ("p2_merkle_tree", &INTERNAL_CUSTODY_P2_MERKLE_TREE[..]),
            ("rekey_and_payment", &INTERNAL_CUSTODY_REKEY_AND_PAYMENT[..]),
            ("only_rekey", &INTERNAL_CUSTODY_ONLY_REKEY[..]),
            ("p2_new_lock_level", &INTERNAL_CUSTODY_NEW_LOCK_LEVEL[..]),
            ("ach_completion", &INTERNAL_CUSTODY_ACH_COMPLETION[..]),
            ("ach_clawback", &INTERNAL_CUSTODY_ACH_CLAWBACK[..]),
            ("rekey_completion", &INTERNAL_CUSTODY_REKEY_COMPLETION[..]),
            ("rekey_clawback", &INTERNAL_CUSTODY_REKEY_CLAWBACK[..]),
        ] {
            let mut hash = chia_sha2::Sha256::new();
            hash.update(bytes);
            let entry = format!("\"{name}.clsp.hex\": \"{}\"", hex::encode(hash.finalize()));
            assert!(
                include_str!("internal_custody/NOTICE.md").contains(&entry),
                "{name}"
            );
        }
    }
    #[test]
    fn spend_discriminators_match_reference_atoms_and_reject_unknown_values() -> anyhow::Result<()>
    {
        let mut a = Allocator::new();
        for kind in [
            InternalCustodySpendType::FinishRekey,
            InternalCustodySpendType::StartRekey,
            InternalCustodySpendType::HandlePayment,
        ] {
            let node = kind.to_clvm(&mut a)?;
            assert_eq!(u8::from_clvm(&a, node)?, kind as u8);
            assert_eq!(InternalCustodySpendType::from_clvm(&a, node)?, kind);
        }
        for value in 3u8..=255 {
            let node = value.to_clvm(&mut a)?;
            assert!(InternalCustodySpendType::from_clvm(&a, node).is_err());
        }
        Ok(())
    }
}
