use chia_protocol::Bytes32;
use chia_puzzle_types::{EveProof, Proof};
use chia_sdk_types::{conditions::CreateCoin, Conditions};
use clvm_traits::clvm_quote;
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;

use crate::{DriverError, Launcher, Spend, SpendContext};

use super::{OptionContract, OptionInfo, OptionMetadata, OptionType, OptionUnderlying};

#[derive(Debug, Clone, Copy)]
pub struct UnspecifiedOption {
    owner_puzzle_hash: Bytes32,
    underlying: OptionUnderlying,
    metadata: OptionMetadata,
}

#[derive(Debug, Clone, Copy)]
pub struct ReadyOption {
    info: OptionInfo,
    metadata: OptionMetadata,
}

#[derive(Debug, Clone)]
pub struct OptionLauncher<S = UnspecifiedOption> {
    launcher: Launcher,
    state: S,
}

impl<S> OptionLauncher<S> {
    pub fn launcher(&self) -> &Launcher {
        &self.launcher
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionLauncherInfo {
    pub creator_puzzle_hash: Bytes32,
    pub owner_puzzle_hash: Bytes32,
    pub seconds: u64,
    pub underlying_amount: u64,
    pub strike_type: OptionType,
}

impl OptionLauncherInfo {
    pub fn new(
        creator_puzzle_hash: Bytes32,
        owner_puzzle_hash: Bytes32,
        seconds: u64,
        underlying_amount: u64,
        strike_type: OptionType,
    ) -> Self {
        Self {
            creator_puzzle_hash,
            owner_puzzle_hash,
            seconds,
            underlying_amount,
            strike_type,
        }
    }
}

impl OptionLauncher<UnspecifiedOption> {
    pub fn new(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        info: OptionLauncherInfo,
        singleton_amount: u64,
    ) -> Result<Self, DriverError> {
        Self::with_amount(ctx, parent_coin_id, 1, info, singleton_amount)
    }

    pub fn with_amount(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        launcher_amount: u64,
        info: OptionLauncherInfo,
        singleton_amount: u64,
    ) -> Result<Self, DriverError> {
        let memos = ctx.hint(info.creator_puzzle_hash)?;
        let launcher = Launcher::with_memos(parent_coin_id, launcher_amount, memos)
            .with_singleton_amount(singleton_amount);
        let launcher_id = launcher.coin().coin_id();

        Ok(Self {
            launcher,
            state: UnspecifiedOption {
                owner_puzzle_hash: info.owner_puzzle_hash,
                underlying: OptionUnderlying::new(
                    launcher_id,
                    info.creator_puzzle_hash,
                    info.seconds,
                    info.underlying_amount,
                    info.strike_type,
                ),
                metadata: OptionMetadata::new(info.seconds, info.strike_type),
            },
        })
    }

    pub fn create_early(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        launcher_amount: u64,
        info: OptionLauncherInfo,
        singleton_amount: u64,
    ) -> Result<(CreateCoin<NodePtr>, Self), DriverError> {
        let memos = ctx.hint(info.creator_puzzle_hash)?;
        let (create_coin, launcher) =
            Launcher::create_early_with_memos(parent_coin_id, launcher_amount, memos);
        let launcher = launcher.with_singleton_amount(singleton_amount);
        let launcher_id = launcher.coin().coin_id();

        let launcher = Self {
            launcher,
            state: UnspecifiedOption {
                owner_puzzle_hash: info.owner_puzzle_hash,
                underlying: OptionUnderlying::new(
                    launcher_id,
                    info.creator_puzzle_hash,
                    info.seconds,
                    info.underlying_amount,
                    info.strike_type,
                ),
                metadata: OptionMetadata::new(info.seconds, info.strike_type),
            },
        };

        Ok((create_coin, launcher))
    }

    pub fn underlying(&self) -> OptionUnderlying {
        self.state.underlying
    }

    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        self.state.underlying.tree_hash().into()
    }

    pub fn metadata(&self) -> OptionMetadata {
        self.state.metadata
    }

    pub fn with_underlying(self, underlying_coin_id: Bytes32) -> OptionLauncher<ReadyOption> {
        let launcher_id = self.launcher.coin().coin_id();

        let info = OptionInfo::new(
            launcher_id,
            underlying_coin_id,
            self.state.underlying.delegated_puzzle().tree_hash().into(),
            self.state.owner_puzzle_hash,
        );

        OptionLauncher {
            launcher: self.launcher,
            state: ReadyOption {
                info,
                metadata: self.state.metadata,
            },
        }
    }
}

impl OptionLauncher<ReadyOption> {
    pub fn info(&self) -> OptionInfo {
        self.state.info
    }

    pub fn mint(self, ctx: &mut SpendContext) -> Result<(Conditions, OptionContract), DriverError> {
        let owner_puzzle_hash = self.state.info.p2_puzzle_hash;

        let memos = ctx.hint(owner_puzzle_hash)?;
        let singleton_amount = self.launcher.singleton_amount();
        let conditions = Conditions::new().create_coin(owner_puzzle_hash, singleton_amount, memos);

        let inner_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        let eve_p2_puzzle_hash = ctx.tree_hash(inner_puzzle).into();
        let inner_spend = Spend::new(inner_puzzle, NodePtr::NIL);

        let (mint_eve_option, eve_option) = self.mint_eve(ctx, eve_p2_puzzle_hash)?;
        eve_option.spend(ctx, inner_spend)?;

        let child = eve_option.child(owner_puzzle_hash, singleton_amount);

        Ok((mint_eve_option, child))
    }

    pub fn mint_eve(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, OptionContract), DriverError> {
        let launcher_coin = self.launcher.coin();

        let info = self.state.info.with_p2_puzzle_hash(p2_puzzle_hash);

        let (launch_singleton, eve_coin) =
            self.launcher
                .spend(ctx, info.inner_puzzle_hash().into(), self.state.metadata)?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((launch_singleton, OptionContract::new(eve_coin, proof, info)))
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Coin;
    use chia_puzzle_types::Memos;
    use chia_sdk_test::Simulator;

    use crate::StandardLayer;

    use super::*;

    #[test]
    fn test_mint_option() -> anyhow::Result<()> {
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
        let launcher =
            launcher.with_underlying(Coin::new(parent_coin.coin_id(), p2_option, 1).coin_id());

        let (mint_option, _option) = launcher.mint(ctx)?;
        alice_p2.spend(ctx, alice.coin, mint_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        Ok(())
    }
}
