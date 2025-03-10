use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{cat::CatArgs, EveProof, Proof};
use chia_sdk_types::Conditions;
use clvm_traits::clvm_quote;
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;

use crate::{DriverError, Launcher, Spend, SpendContext};

use super::{OptionContract, OptionInfo, OptionMetadata, OptionType, OptionUnderlying};

#[derive(Debug, Clone, Copy)]
pub struct UnspecifiedOption {
    creator_puzzle_hash: Bytes32,
    owner_puzzle_hash: Bytes32,
    seconds: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ReadyOption {
    info: OptionInfo,
    metadata: OptionMetadata,
    underlying: OptionUnderlying,
    underlying_coin: Coin,
}

#[derive(Debug, Clone)]
pub struct OptionLauncher<S> {
    launcher: Launcher,
    state: S,
}

impl<S> OptionLauncher<S> {
    pub fn launcher(&self) -> &Launcher {
        &self.launcher
    }
}

impl OptionLauncher<UnspecifiedOption> {
    pub fn new(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        creator_puzzle_hash: Bytes32,
        owner_puzzle_hash: Bytes32,
        seconds: u64,
    ) -> Result<Self, DriverError> {
        let memos = ctx.hint(creator_puzzle_hash)?;

        Ok(Self {
            launcher: Launcher::with_memos(parent_coin_id, 1, memos),
            state: UnspecifiedOption {
                creator_puzzle_hash,
                owner_puzzle_hash,
                seconds,
            },
        })
    }

    pub fn lock_underlying(
        self,
        parent_coin_id: Bytes32,
        asset_id: Option<Bytes32>,
        amount: u64,
    ) -> (Conditions, OptionLauncher<ReadyOption>) {
        let launcher_id = self.launcher.coin().coin_id();

        let underlying = OptionUnderlying::new(
            launcher_id,
            self.state.creator_puzzle_hash,
            self.state.seconds,
            amount,
        );

        let underlying_puzzle_hash = underlying.tree_hash().into();
        let conditions = Conditions::new().create_coin(underlying_puzzle_hash, amount, None);

        let wrapped_underlying_puzzle_hash = if let Some(asset_id) = asset_id {
            CatArgs::curry_tree_hash(asset_id, underlying_puzzle_hash.into()).into()
        } else {
            underlying_puzzle_hash
        };

        let underlying_coin = Coin::new(parent_coin_id, wrapped_underlying_puzzle_hash, amount);

        let info = OptionInfo::new(
            launcher_id,
            underlying_coin.coin_id(),
            underlying.delegated_puzzle().tree_hash().into(),
            self.state.owner_puzzle_hash,
        );

        let metadata = OptionMetadata::new(
            self.state.seconds,
            if let Some(asset_id) = asset_id {
                OptionType::Cat { asset_id, amount }
            } else {
                OptionType::Xch { amount }
            },
        );

        let launcher = OptionLauncher {
            launcher: self.launcher,
            state: ReadyOption {
                info,
                metadata,
                underlying,
                underlying_coin,
            },
        };

        (conditions, launcher)
    }
}

impl OptionLauncher<ReadyOption> {
    pub fn info(&self) -> OptionInfo {
        self.state.info
    }

    pub fn metadata(&self) -> OptionMetadata {
        self.state.metadata
    }

    pub fn underlying_coin(&self) -> Coin {
        self.state.underlying_coin
    }

    pub fn underlying(&self) -> OptionUnderlying {
        self.state.underlying
    }

    pub fn mint(self, ctx: &mut SpendContext) -> Result<(Conditions, OptionContract), DriverError> {
        let owner_puzzle_hash = self.state.info.p2_puzzle_hash;

        let memos = ctx.hint(owner_puzzle_hash)?;
        let conditions = Conditions::new().create_coin(owner_puzzle_hash, 1, Some(memos));

        let inner_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        let eve_p2_puzzle_hash = ctx.tree_hash(inner_puzzle).into();
        let inner_spend = Spend::new(inner_puzzle, NodePtr::NIL);

        let (mint_eve_option, eve_option) = self.mint_eve(ctx, eve_p2_puzzle_hash)?;
        eve_option.spend(ctx, inner_spend)?;

        let child = eve_option.wrapped_child(owner_puzzle_hash);

        Ok((mint_eve_option, child))
    }

    fn mint_eve(
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
            alice.puzzle_hash,
            alice.puzzle_hash,
            10,
        )?;

        let (lock, launcher) = launcher.lock_underlying(parent_coin.coin_id(), None, 1);
        alice_p2.spend(ctx, parent_coin, lock)?;

        let (mint_option, _option) = launcher.mint(ctx)?;
        alice_p2.spend(ctx, alice.coin, mint_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk.clone()])?;

        Ok(())
    }
}
