use chia::{
    clvm_utils::{ToTreeHash, TreeHash},
    protocol::{Bytes32, Coin, CoinSpend},
    puzzles::{
        singleton::{SingletonArgs, SingletonSolution},
        EveProof, LineageProof, Proof,
    },
};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_wallet_sdk::driver::{DriverError, Layer, Puzzle, SingletonLayer, SpendContext};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::{Allocator, NodePtr};

use crate::{
    VerificationLayer, VerificationLayer2ndCurryArgs, VerificationLayerSolution, VerifiedData,
};

use super::VerificationInfo;

type VerificationLayers = SingletonLayer<VerificationLayer>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Verification {
    pub coin: Coin,
    pub proof: Proof,

    pub info: VerificationInfo,
}

impl Verification {
    pub fn new(coin: Coin, proof: Proof, info: VerificationInfo) -> Self {
        Self { coin, proof, info }
    }

    pub fn after_mint(
        launcher_parent: Bytes32,
        revocation_singleton_launcher_id: Bytes32,
        verified_data: VerifiedData,
    ) -> Self {
        let launcher_coin = Coin::new(launcher_parent, SINGLETON_LAUNCHER_HASH.into(), 0);
        let verification_launcher_id = launcher_coin.coin_id();

        let info = VerificationInfo {
            launcher_id: verification_launcher_id,
            revocation_singleton_launcher_id,
            verified_data,
        };

        Self {
            coin: Coin::new(verification_launcher_id, Self::puzzle_hash(&info).into(), 1),
            proof: Proof::Eve(EveProof {
                parent_parent_coin_info: launcher_parent,
                parent_amount: 0,
            }),
            info,
        }
    }

    pub fn inner_puzzle_hash<T>(
        revocation_singleton_launcher_id: Bytes32,
        verified_data: T,
    ) -> TreeHash
    where
        T: ToTreeHash,
    {
        VerificationLayer2ndCurryArgs::curry_tree_hash(
            revocation_singleton_launcher_id,
            verified_data,
        )
    }

    pub fn puzzle_hash(info: &VerificationInfo) -> TreeHash {
        SingletonArgs::curry_tree_hash(
            info.launcher_id,
            Self::inner_puzzle_hash(
                info.revocation_singleton_launcher_id,
                info.verified_data.tree_hash(),
            ),
        )
    }

    pub fn into_layers(self) -> VerificationLayers {
        SingletonLayer::new(
            self.info.launcher_id,
            VerificationLayer::new(
                self.info.revocation_singleton_launcher_id,
                self.info.verified_data,
            ),
        )
    }

    pub fn into_layers_with_clone(&self) -> VerificationLayers {
        SingletonLayer::new(
            self.info.launcher_id,
            VerificationLayer::new(
                self.info.revocation_singleton_launcher_id,
                self.info.verified_data.clone(),
            ),
        )
    }

    pub fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let layers = self.into_layers_with_clone();

        layers.construct_puzzle(ctx)
    }

    pub fn spend(
        self,
        ctx: &mut SpendContext,
        revocation_singleton_inner_puzzle_hash: Option<Bytes32>,
    ) -> Result<(), DriverError> {
        let sol = SingletonSolution {
            lineage_proof: self.proof,
            amount: self.coin.amount,
            inner_solution: VerificationLayerSolution {
                revocation_singleton_inner_puzzle_hash,
            },
        };
        let my_coin = self.coin;

        let layers = self.into_layers();

        let puzzle_reveal = layers.construct_puzzle(ctx)?;
        let solution = layers.construct_solution(ctx, sol)?;

        let puzzle_reveal = ctx.serialize(&puzzle_reveal)?;
        let solution = ctx.serialize(&solution)?;

        ctx.insert(CoinSpend::new(my_coin, puzzle_reveal, solution));

        Ok(())
    }
}

impl Verification {
    pub fn from_parent_spend(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
    ) -> Result<Option<Self>, DriverError> {
        let Some(parent_layers) = VerificationLayers::parse_puzzle(allocator, parent_puzzle)?
        else {
            return Ok(None);
        };

        let parent_inner_puzzle_hash = Verification::inner_puzzle_hash(
            parent_layers.inner_puzzle.revocation_singleton_launcher_id,
            parent_layers.inner_puzzle.verified_data.tree_hash(),
        )
        .into();

        Ok(Some(Self {
            coin: Coin::new(parent_coin.coin_id(), parent_coin.puzzle_hash, 1),
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash,
                parent_amount: parent_coin.amount,
            }),
            info: VerificationInfo::new(
                parent_layers.launcher_id,
                parent_layers.inner_puzzle.revocation_singleton_launcher_id,
                parent_layers.inner_puzzle.verified_data,
            ),
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct VerificationLauncherKVList {
    pub revocation_singleton_launcher_id: Bytes32,
    pub verified_data: VerifiedData,
}

#[cfg(test)]
mod tests {
    use anyhow::Ok;
    use chia::protocol::Bytes;
    use chia_puzzle_types::Memos;
    use chia_puzzles::SINGLETON_LAUNCHER_HASH;
    use chia_wallet_sdk::{
        driver::{Launcher, StandardLayer},
        test::Simulator,
        types::Conditions,
    };

    use crate::{VerificationAsserter, VerifiedData};

    use super::*;

    #[test]
    fn test_verifications_and_asserter() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();
        let bls = sim.bls(1);
        let p2 = StandardLayer::new(bls.pk);

        let did_launcher = Launcher::new(bls.coin.coin_id(), 1);
        let (create_did, did) = did_launcher.create_simple_did(ctx, &p2)?;
        p2.spend(ctx, bls.coin, create_did)?;

        let verifier_proof = did.child_lineage_proof();
        let did = did.update(
            ctx,
            &p2,
            Conditions::new().create_coin(SINGLETON_LAUNCHER_HASH.into(), 0, Memos::None),
        )?;
        let verification_launcher = Launcher::new(did.coin.parent_coin_info, 0);
        // we don't need an extra mojo for the verification coin since it's melted in the same tx

        let verified_data = VerifiedData {
            version: 1,
            asset_id: Bytes32::new([2; 32]),
            data_hash: Bytes32::new([3; 32]),
            comment: "Test verification for test testing purposes only.".to_string(),
        };
        let verification = Verification::after_mint(
            verification_launcher.coin().parent_coin_info,
            did.info.launcher_id,
            verified_data.clone(),
        );

        let (_conds, new_coin) = verification_launcher.with_singleton_amount(1).spend(
            ctx,
            Verification::inner_puzzle_hash(
                verification.info.revocation_singleton_launcher_id,
                verification.info.verified_data.clone(),
            )
            .into(),
            (),
        )?;

        assert_eq!(new_coin, verification.coin);

        // spend the verification coin in oracle mode
        verification.clone().spend(ctx, None)?;

        let parent_puzzle = verification.construct_puzzle(ctx)?;
        let parent_puzzle = Puzzle::parse(ctx, parent_puzzle);
        let verification =
            Verification::from_parent_spend(ctx, verification.coin, parent_puzzle)?.unwrap();

        // create verification payment and spend it
        let verification_asserter = VerificationAsserter::from(
            did.info.launcher_id,
            verified_data.version,
            verified_data.asset_id.tree_hash(),
            verified_data.data_hash.tree_hash(),
        );

        let payment_coin = sim.new_coin(verification_asserter.tree_hash().into(), 1337);
        verification_asserter.spend(ctx, payment_coin, verifier_proof, 0, verified_data.comment)?;

        // melt verification coin
        let revocation_singleton_inner_ph = did.info.inner_puzzle_hash().into();

        let msg_data = ctx.alloc(&verification.coin.puzzle_hash)?;
        let _ = did.update(
            ctx,
            &p2,
            Conditions::new().send_message(18, Bytes::default(), vec![msg_data]),
        )?;

        verification.spend(ctx, Some(revocation_singleton_inner_ph))?;

        sim.spend_coins(ctx.take(), &[bls.sk])?;

        Ok(())
    }
}
