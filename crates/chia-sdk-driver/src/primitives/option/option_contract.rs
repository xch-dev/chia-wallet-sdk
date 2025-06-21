use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    singleton::{LauncherSolution, SingletonArgs, SingletonSolution},
    LineageProof, Proof,
};
use chia_sdk_types::{
    puzzles::{OptionContractArgs, OptionContractSolution},
    run_puzzle, Condition, Conditions, Mod,
};
use clvm_traits::FromClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext, SpendWithConditions};

use super::{OptionContractLayers, OptionInfo, OptionMetadata};

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionContract {
    pub coin: Coin,
    pub proof: Proof,
    pub info: OptionInfo,
}

impl OptionContract {
    pub fn new(coin: Coin, proof: Proof, info: OptionInfo) -> Self {
        Self { coin, proof, info }
    }

    pub fn parse_child(
        allocator: &mut Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        let Some(singleton) =
            OptionContractLayers::<Puzzle>::parse_puzzle(allocator, parent_puzzle)?
        else {
            return Ok(None);
        };

        let solution = OptionContractLayers::<Puzzle>::parse_solution(allocator, parent_solution)?;
        let output = run_puzzle(
            allocator,
            singleton.inner_puzzle.inner_puzzle.ptr(),
            solution.inner_solution.inner_solution,
        )?;
        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;

        let Some(create_coin) = conditions
            .into_iter()
            .filter_map(Condition::into_create_coin)
            .find(|cond| cond.amount % 2 == 1)
        else {
            return Err(DriverError::MissingChild);
        };

        let puzzle_hash = SingletonArgs::curry_tree_hash(
            singleton.launcher_id,
            OptionContractArgs::new(
                singleton.inner_puzzle.underlying_coin_id,
                singleton.inner_puzzle.underlying_delegated_puzzle_hash,
                TreeHash::from(create_coin.puzzle_hash),
            )
            .curry_tree_hash(),
        );

        let option = Self {
            coin: Coin::new(
                parent_coin.coin_id(),
                puzzle_hash.into(),
                create_coin.amount,
            ),
            proof: Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: singleton.inner_puzzle.tree_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            info: OptionInfo {
                launcher_id: singleton.launcher_id,
                underlying_coin_id: singleton.inner_puzzle.underlying_coin_id,
                underlying_delegated_puzzle_hash: singleton
                    .inner_puzzle
                    .underlying_delegated_puzzle_hash,
                p2_puzzle_hash: create_coin.puzzle_hash,
            },
        };

        Ok(Some(option))
    }

    pub fn parse_metadata(
        allocator: &mut Allocator,
        launcher_solution: NodePtr,
    ) -> Result<OptionMetadata, DriverError> {
        let solution = LauncherSolution::<OptionMetadata>::from_clvm(allocator, launcher_solution)?;
        Ok(solution.key_value_list)
    }

    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        inner_spend: Spend,
    ) -> Result<Option<Self>, DriverError> {
        let layers = self.info.into_layers(inner_spend.puzzle);

        let spend = layers.construct_spend(
            ctx,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution: OptionContractSolution::new(inner_spend.solution),
            },
        )?;

        ctx.spend(self.coin, spend)?;

        let output = ctx.run(inner_spend.puzzle, inner_spend.solution)?;
        let conditions = Vec::<Condition>::from_clvm(ctx, output)?;

        for condition in conditions {
            if let Some(create_coin) = condition.into_create_coin() {
                if create_coin.amount % 2 == 1 {
                    return Ok(Some(
                        self.child(create_coin.puzzle_hash, create_coin.amount),
                    ));
                }
            }
        }

        Ok(None)
    }

    pub fn spend_with<I>(
        &self,
        ctx: &mut SpendContext,
        inner: &I,
        conditions: Conditions,
    ) -> Result<Option<Self>, DriverError>
    where
        I: SpendWithConditions,
    {
        let inner_spend = inner.spend_with_conditions(ctx, conditions)?;
        self.spend(ctx, inner_spend)
    }

    pub fn transfer<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        p2_puzzle_hash: Bytes32,
        extra_conditions: Conditions,
    ) -> Result<Self, DriverError>
    where
        I: SpendWithConditions,
    {
        let memos = ctx.hint(p2_puzzle_hash)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions.create_coin(p2_puzzle_hash, self.coin.amount, memos),
        )?;

        Ok(self.child(p2_puzzle_hash, self.coin.amount))
    }

    pub fn exercise<I>(
        self,
        ctx: &mut SpendContext,
        inner: &I,
        extra_conditions: Conditions,
    ) -> Result<(), DriverError>
    where
        I: SpendWithConditions,
    {
        let data = ctx.alloc(&self.info.underlying_coin_id)?;

        self.spend_with(
            ctx,
            inner,
            extra_conditions
                .send_message(
                    23,
                    self.info.underlying_delegated_puzzle_hash.into(),
                    vec![data],
                )
                .melt_singleton(),
        )?;

        Ok(())
    }

    pub fn child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Self {
        let info = self.info.with_p2_puzzle_hash(p2_puzzle_hash);

        let inner_puzzle_hash = info.inner_puzzle_hash();

        Self::new(
            Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(info.launcher_id, inner_puzzle_hash).into(),
                amount,
            ),
            Proof::Lineage(self.child_lineage_proof()),
            info,
        )
    }
}

#[cfg(test)]
mod tests {
    use chia_puzzle_types::{offer::SettlementPaymentsSolution, Memos};
    use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
    use chia_sdk_test::{expect_spend, Simulator};
    use chia_sdk_types::{
        conditions::TransferNft,
        puzzles::{RevocationArgs, RevocationSolution},
    };
    use rstest::rstest;

    use crate::{
        Cat, CatSpend, HashedPtr, Launcher, Nft, NftMint, OptionLauncher, OptionLauncherInfo,
        OptionType, SettlementLayer, StandardLayer,
    };

    use super::*;

    enum Action {
        Exercise,
        ExerciseWithoutPayment,
        Clawback,
    }

    enum Type {
        Xch,
        Cat,
        RevocableCat,
        Nft,
    }

    enum OptionCoin {
        Xch(Coin),
        Cat(Cat),
        RevocableCat(Cat),
        Nft(Nft<HashedPtr>),
    }

    impl OptionCoin {
        fn coin_id(&self) -> Bytes32 {
            match self {
                Self::Xch(coin) => coin.coin_id(),
                Self::Cat(cat) | Self::RevocableCat(cat) => cat.coin.coin_id(),
                Self::Nft(nft) => nft.coin.coin_id(),
            }
        }
    }

    #[rstest]
    fn test_option_actions(
        #[values(true, false)] expired: bool,
        #[values(Action::Exercise, Action::ExerciseWithoutPayment, Action::Clawback)]
        action: Action,
        #[values(Type::Xch, Type::Cat, Type::RevocableCat, Type::Nft)] underlying_type: Type,
        #[values(1, 1000, u64::MAX)] underlying_amount: u64,
        #[values(Type::Xch, Type::Cat, Type::RevocableCat, Type::Nft)] strike_type: Type,
        #[values(1, 1000, u64::MAX)] strike_amount: u64,
    ) -> anyhow::Result<()> {
        if matches!(underlying_type, Type::Nft) && underlying_amount != 1 {
            return Ok(());
        }

        if matches!(strike_type, Type::Nft) && strike_amount != 1 {
            return Ok(());
        }

        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        if expired {
            sim.set_next_timestamp(100)?;
        }

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let strike_parent_coin = sim.new_coin(
            alice.puzzle_hash,
            if matches!(strike_type, Type::Nft) {
                strike_amount + 1
            } else {
                strike_amount
            },
        );
        let (strike_coin, strike_type) = match strike_type {
            Type::Xch => {
                alice_p2.spend(
                    ctx,
                    strike_parent_coin,
                    Conditions::new().create_coin(
                        SETTLEMENT_PAYMENT_HASH.into(),
                        strike_amount,
                        Memos::None,
                    ),
                )?;
                let coin = OptionCoin::Xch(Coin::new(
                    strike_parent_coin.coin_id(),
                    SETTLEMENT_PAYMENT_HASH.into(),
                    strike_amount,
                ));
                (
                    coin,
                    OptionType::Xch {
                        amount: strike_amount,
                    },
                )
            }
            Type::Cat => {
                let hint = ctx.hint(SETTLEMENT_PAYMENT_HASH.into())?;
                let (issue_cat, cats) = Cat::issue_with_coin(
                    ctx,
                    strike_parent_coin.coin_id(),
                    strike_amount,
                    Conditions::new().create_coin(
                        SETTLEMENT_PAYMENT_HASH.into(),
                        strike_amount,
                        hint,
                    ),
                )?;
                alice_p2.spend(ctx, strike_parent_coin, issue_cat)?;
                let coin = OptionCoin::Cat(cats[0]);
                (
                    coin,
                    OptionType::Cat {
                        asset_id: cats[0].info.asset_id,
                        amount: strike_amount,
                    },
                )
            }
            Type::RevocableCat => {
                let hint = ctx.hint(SETTLEMENT_PAYMENT_HASH.into())?;
                let revocation_settlement_hash =
                    RevocationArgs::new(Bytes32::default(), SETTLEMENT_PAYMENT_HASH.into())
                        .curry_tree_hash()
                        .into();
                let (issue_cat, cats) = Cat::issue_with_coin(
                    ctx,
                    strike_parent_coin.coin_id(),
                    strike_amount,
                    Conditions::new().create_coin(revocation_settlement_hash, strike_amount, hint),
                )?;
                alice_p2.spend(ctx, strike_parent_coin, issue_cat)?;
                let coin = OptionCoin::RevocableCat(cats[0]);
                (
                    coin,
                    OptionType::RevocableCat {
                        asset_id: cats[0].info.asset_id,
                        hidden_puzzle_hash: Bytes32::default(),
                        amount: strike_amount,
                    },
                )
            }
            Type::Nft => {
                let (create_did, did) = Launcher::new(strike_parent_coin.coin_id(), 1)
                    .create_simple_did(ctx, &alice_p2)?;

                let (mint_nft, nft) = Launcher::new(did.coin.coin_id(), 0)
                    .with_singleton_amount(strike_amount)
                    .mint_nft(
                        ctx,
                        NftMint::new(
                            HashedPtr::NIL,
                            SETTLEMENT_PAYMENT_HASH.into(),
                            0,
                            Some(TransferNft::new(
                                Some(did.info.launcher_id),
                                Vec::new(),
                                Some(did.info.inner_puzzle_hash().into()),
                            )),
                        ),
                    )?;

                alice_p2.spend(ctx, strike_parent_coin, create_did)?;
                let _did = did.update(ctx, &alice_p2, mint_nft)?;

                let launcher_id = nft.info.launcher_id;

                (
                    OptionCoin::Nft(nft),
                    OptionType::Nft {
                        launcher_id,
                        settlement_puzzle_hash: nft.coin.puzzle_hash,
                        amount: strike_amount,
                    },
                )
            }
        };

        let launcher = OptionLauncher::new(
            ctx,
            alice.coin.coin_id(),
            OptionLauncherInfo::new(
                alice.puzzle_hash,
                alice.puzzle_hash,
                10,
                underlying_amount,
                strike_type,
            ),
            1,
        )?;
        let underlying = launcher.underlying();
        let p2_option = launcher.p2_puzzle_hash();

        let underlying_parent_coin = sim.new_coin(
            alice.puzzle_hash,
            if matches!(underlying_type, Type::Nft) {
                underlying_amount + 1
            } else {
                underlying_amount
            },
        );
        let underlying_coin = match underlying_type {
            Type::Xch => {
                alice_p2.spend(
                    ctx,
                    underlying_parent_coin,
                    Conditions::new().create_coin(p2_option, underlying_amount, Memos::None),
                )?;
                OptionCoin::Xch(Coin::new(
                    underlying_parent_coin.coin_id(),
                    p2_option,
                    underlying_amount,
                ))
            }
            Type::Cat => {
                let hint = ctx.hint(p2_option)?;
                let (issue_cat, cats) = Cat::issue_with_coin(
                    ctx,
                    underlying_parent_coin.coin_id(),
                    underlying_amount,
                    Conditions::new().create_coin(p2_option, underlying_amount, hint),
                )?;
                alice_p2.spend(ctx, underlying_parent_coin, issue_cat)?;
                OptionCoin::Cat(cats[0])
            }
            Type::RevocableCat => {
                let hint = ctx.hint(p2_option)?;
                let revocation_p2_option = RevocationArgs::new(Bytes32::default(), p2_option)
                    .curry_tree_hash()
                    .into();
                let (issue_cat, cats) = Cat::issue_with_coin(
                    ctx,
                    underlying_parent_coin.coin_id(),
                    underlying_amount,
                    Conditions::new().create_coin(revocation_p2_option, underlying_amount, hint),
                )?;
                alice_p2.spend(ctx, underlying_parent_coin, issue_cat)?;
                OptionCoin::RevocableCat(cats[0])
            }
            Type::Nft => {
                let (create_did, did) = Launcher::new(underlying_parent_coin.coin_id(), 1)
                    .create_simple_did(ctx, &alice_p2)?;

                let (mint_nft, nft) = Launcher::new(did.coin.coin_id(), 0)
                    .with_singleton_amount(underlying_amount)
                    .mint_nft(
                        ctx,
                        NftMint::new(
                            HashedPtr::NIL,
                            p2_option,
                            0,
                            Some(TransferNft::new(
                                Some(did.info.launcher_id),
                                Vec::new(),
                                Some(did.info.inner_puzzle_hash().into()),
                            )),
                        ),
                    )?;

                alice_p2.spend(ctx, underlying_parent_coin, create_did)?;
                let _did = did.update(ctx, &alice_p2, mint_nft)?;

                OptionCoin::Nft(nft)
            }
        };

        let launcher = launcher.with_underlying(underlying_coin.coin_id());

        let (mint_option, option) = launcher.mint(ctx)?;
        alice_p2.spend(ctx, alice.coin, mint_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        match action {
            Action::Exercise | Action::ExerciseWithoutPayment => {
                option.exercise(ctx, &alice_p2, Conditions::new())?;

                match underlying_coin {
                    OptionCoin::Xch(coin) => {
                        underlying.exercise_coin_spend(
                            ctx,
                            coin,
                            option.info.inner_puzzle_hash().into(),
                            option.coin.amount,
                        )?;
                    }
                    OptionCoin::Cat(cat) => {
                        let exercise_spend = underlying.exercise_spend(
                            ctx,
                            option.info.inner_puzzle_hash().into(),
                            option.coin.amount,
                        )?;
                        Cat::spend_all(ctx, &[CatSpend::new(cat, exercise_spend)])?;
                    }
                    OptionCoin::RevocableCat(cat) => {
                        let exercise_spend = underlying.exercise_spend(
                            ctx,
                            option.info.inner_puzzle_hash().into(),
                            option.coin.amount,
                        )?;
                        let puzzle =
                            ctx.curry(RevocationArgs::new(Bytes32::default(), p2_option))?;
                        let solution = ctx.alloc(&RevocationSolution::new(
                            false,
                            exercise_spend.puzzle,
                            exercise_spend.solution,
                        ))?;
                        let exercise_spend = Spend::new(puzzle, solution);
                        Cat::spend_all(ctx, &[CatSpend::new(cat, exercise_spend)])?;
                    }
                    OptionCoin::Nft(nft) => {
                        let exercise_spend = underlying.exercise_spend(
                            ctx,
                            option.info.inner_puzzle_hash().into(),
                            option.coin.amount,
                        )?;
                        let _nft = nft.spend(ctx, exercise_spend)?;
                    }
                }
            }
            Action::Clawback => match underlying_coin {
                OptionCoin::Xch(coin) => {
                    let clawback_spend = alice_p2.spend_with_conditions(
                        ctx,
                        Conditions::new().create_coin(
                            alice.puzzle_hash,
                            underlying_amount,
                            Memos::None,
                        ),
                    )?;
                    underlying.clawback_coin_spend(ctx, coin, clawback_spend)?;
                }
                OptionCoin::Cat(cat) => {
                    let hint = ctx.hint(alice.puzzle_hash)?;
                    let clawback_spend = alice_p2.spend_with_conditions(
                        ctx,
                        Conditions::new().create_coin(alice.puzzle_hash, underlying_amount, hint),
                    )?;
                    let clawback_spend = underlying.clawback_spend(ctx, clawback_spend)?;
                    Cat::spend_all(ctx, &[CatSpend::new(cat, clawback_spend)])?;
                }
                OptionCoin::RevocableCat(cat) => {
                    let hint = ctx.hint(alice.puzzle_hash)?;
                    let clawback_spend = alice_p2.spend_with_conditions(
                        ctx,
                        Conditions::new().create_coin(alice.puzzle_hash, underlying_amount, hint),
                    )?;
                    let clawback_spend = underlying.clawback_spend(ctx, clawback_spend)?;
                    let puzzle = ctx.curry(RevocationArgs::new(Bytes32::default(), p2_option))?;
                    let solution = ctx.alloc(&RevocationSolution::new(
                        false,
                        clawback_spend.puzzle,
                        clawback_spend.solution,
                    ))?;
                    let clawback_spend = Spend::new(puzzle, solution);
                    Cat::spend_all(ctx, &[CatSpend::new(cat, clawback_spend)])?;
                }
                OptionCoin::Nft(nft) => {
                    let hint = ctx.hint(alice.puzzle_hash)?;
                    let clawback_spend = alice_p2.spend_with_conditions(
                        ctx,
                        Conditions::new().create_coin(alice.puzzle_hash, underlying_amount, hint),
                    )?;
                    let clawback_spend = underlying.clawback_spend(ctx, clawback_spend)?;
                    let _nft = nft.spend(ctx, clawback_spend)?;
                }
            },
        }

        if matches!(action, Action::Exercise) {
            match strike_coin {
                OptionCoin::Xch(coin) => {
                    let payment = underlying.requested_payment(&mut **ctx)?;
                    let coin_spend = SettlementLayer.construct_coin_spend(
                        ctx,
                        coin,
                        SettlementPaymentsSolution::new(vec![payment]),
                    )?;
                    ctx.insert(coin_spend);
                }
                OptionCoin::Cat(cat) => {
                    let payment = underlying.requested_payment(&mut **ctx)?;
                    let spend = SettlementLayer
                        .construct_spend(ctx, SettlementPaymentsSolution::new(vec![payment]))?;
                    Cat::spend_all(ctx, &[CatSpend::new(cat, spend)])?;
                }
                OptionCoin::RevocableCat(cat) => {
                    let payment = underlying.requested_payment(&mut **ctx)?;
                    let spend = SettlementLayer
                        .construct_spend(ctx, SettlementPaymentsSolution::new(vec![payment]))?;
                    let puzzle = ctx.curry(RevocationArgs::new(
                        Bytes32::default(),
                        SETTLEMENT_PAYMENT_HASH.into(),
                    ))?;
                    let solution = ctx.alloc(&RevocationSolution::new(
                        false,
                        spend.puzzle,
                        spend.solution,
                    ))?;
                    Cat::spend_all(ctx, &[CatSpend::new(cat, Spend::new(puzzle, solution))])?;
                }
                OptionCoin::Nft(nft) => {
                    let payment = underlying.requested_payment(&mut **ctx)?;
                    let spend = SettlementLayer
                        .construct_spend(ctx, SettlementPaymentsSolution::new(vec![payment]))?;
                    let _nft = nft.spend(ctx, spend)?;
                }
            }
        }

        expect_spend(
            sim.spend_coins(ctx.take(), &[alice.sk]),
            match action {
                Action::Exercise => !expired,
                Action::ExerciseWithoutPayment => false,
                Action::Clawback => expired,
            },
        );

        Ok(())
    }

    #[test]
    fn test_transfer_option() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let parent_coin = sim.new_coin(alice.puzzle_hash, 1);

        let launcher = OptionLauncher::new(
            ctx,
            alice.coin.coin_id(),
            OptionLauncherInfo::new(
                alice.puzzle_hash,
                alice.puzzle_hash,
                10,
                1,
                OptionType::Xch { amount: 1 },
            ),
            1,
        )?;
        let p2_option = launcher.p2_puzzle_hash();

        alice_p2.spend(
            ctx,
            parent_coin,
            Conditions::new().create_coin(p2_option, 1, Memos::None),
        )?;
        let underlying_coin = Coin::new(parent_coin.coin_id(), p2_option, 1);
        let launcher = launcher.with_underlying(underlying_coin.coin_id());

        let (mint_option, mut option) = launcher.mint(ctx)?;
        alice_p2.spend(ctx, alice.coin, mint_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        for _ in 0..5 {
            option = option.transfer(ctx, &alice_p2, alice.puzzle_hash, Conditions::new())?;
        }

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }

    #[rstest]
    fn test_incomplete_exercise(#[values(true, false)] melt: bool) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let parent_coin = sim.new_coin(alice.puzzle_hash, 1);

        let launcher = OptionLauncher::new(
            ctx,
            alice.coin.coin_id(),
            OptionLauncherInfo::new(
                alice.puzzle_hash,
                alice.puzzle_hash,
                10,
                1,
                OptionType::Xch { amount: 1 },
            ),
            1,
        )?;
        let p2_option = launcher.p2_puzzle_hash();

        alice_p2.spend(
            ctx,
            parent_coin,
            Conditions::new().create_coin(p2_option, 1, Memos::None),
        )?;
        let underlying_coin = Coin::new(parent_coin.coin_id(), p2_option, 1);
        let launcher = launcher.with_underlying(underlying_coin.coin_id());

        let (mint_option, option) = launcher.mint(ctx)?;
        alice_p2.spend(ctx, alice.coin, mint_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        let data = ctx.alloc(&option.info.underlying_coin_id)?;

        option.spend_with(
            ctx,
            &alice_p2,
            if melt {
                Conditions::new().melt_singleton()
            } else {
                Conditions::new().send_message(
                    23,
                    option.info.underlying_delegated_puzzle_hash.into(),
                    vec![data],
                )
            },
        )?;

        assert!(sim.spend_coins(ctx.take(), &[alice.sk]).is_err());

        Ok(())
    }
}
