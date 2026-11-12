use super::{InternalCustodyPaymentOptions, InternalCustodyPolicy};
use crate::{DriverError, InternalCustodyLayer, Singleton, SingletonInfo, SpendContext};
use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use clvm_utils::{ToTreeHash, TreeHash};

/// A custody singleton with its lineage proof and public authorization policy.
/// The inherited `child_with` and `child_lineage_proof` helpers track continuations.
/// Constructing this value does not establish chain inclusion or policy provenance.
pub type InternalCustody = Singleton<InternalCustodyPolicy>;

impl SingletonInfo for InternalCustodyPolicy {
    fn launcher_id(&self) -> Bytes32 {
        self.info().launcher_id
    }
    fn inner_puzzle_hash(&self) -> TreeHash {
        self.layer().tree_hash()
    }
}

impl SingletonInfo for &InternalCustodyPolicy {
    fn launcher_id(&self) -> Bytes32 {
        self.info().launcher_id
    }
    fn inner_puzzle_hash(&self) -> TreeHash {
        self.layer().tree_hash()
    }
}

impl InternalCustodyPolicy {
    pub(super) fn layer(&self) -> InternalCustodyLayer {
        let info = self.info();
        InternalCustodyLayer {
            puzzle_root: info.puzzle_root,
            state: chia_sdk_types::puzzles::InternalCustodyState {
                puzzles: self.drop_puzzle_hashes,
                timelocks: chia_sdk_types::puzzles::InternalCustodyTimelocks {
                    withdrawal: info.withdrawal_timelock,
                    rekey_increment: info.rekey_increment,
                    slow_rekey: info.slow_rekey,
                },
            },
        }
    }
}

impl InternalCustody {
    /// Construct a payment or absorption using this singleton's coin and proof.
    pub fn payment_spends(
        &self,
        ctx: &mut SpendContext,
        options: InternalCustodyPaymentOptions,
        signers: &[PublicKey],
        deposits: &[Coin],
    ) -> Result<Vec<CoinSpend>, DriverError> {
        self.info
            .payment_spends_with_options(ctx, self.coin, self.proof, options, signers, deposits)
    }
    /// Propose a replacement policy with the subset's minimum initiation delay.
    pub fn start_rekey_spend(
        &self,
        ctx: &mut SpendContext,
        new_policy: &InternalCustodyPolicy,
        signers: &[PublicKey],
        fee_announcement: bool,
    ) -> Result<CoinSpend, DriverError> {
        self.info.start_rekey_spend(
            ctx,
            self.coin,
            self.proof,
            new_policy,
            signers,
            fee_announcement,
        )
    }
}
