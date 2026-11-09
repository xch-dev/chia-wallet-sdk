use chia_protocol::Bytes32;
use chia_puzzle_types::{EveProof, Proof};
use chia_sdk_types::{Conditions, conditions::CreateCoin};
use clvm_traits::clvm_quote;
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;

use crate::{DriverError, Launcher, SingletonInfo, Spend, SpendContext};

use super::{OptionContract, OptionInfo, OptionMetadata, OptionType, OptionUnderlying};

/// The [`OptionLauncher`] state before the underlying coin has been specified.
#[derive(Debug, Clone, Copy)]
pub struct UnspecifiedOption {
    owner_puzzle_hash: Bytes32,
    underlying: OptionUnderlying,
    metadata: OptionMetadata,
}

/// The [`OptionLauncher`] state once the underlying coin is known, so the option can be minted.
#[derive(Debug, Clone, Copy)]
pub struct ReadyOption {
    info: OptionInfo,
    metadata: OptionMetadata,
}

/// A builder for minting an [`OptionContract`].
///
/// The launcher id must be known before the underlying coin can be created, since the underlying
/// coin's p2 puzzle commits to it. So the flow is:
/// 1. Create the launcher with [`OptionLauncher::new`].
/// 2. Lock the underlying coin with [`OptionLauncher::p2_puzzle_hash`].
/// 3. Specify the underlying coin with [`OptionLauncher::with_underlying`].
/// 4. Mint the option with [`OptionLauncher::mint`].
///
/// # Example
///
/// ```
/// use chia_protocol::Coin;
/// use chia_puzzle_types::Memos;
/// use chia_sdk_driver::{OptionLauncher, OptionLauncherInfo, OptionType, SpendContext, StandardLayer};
/// use chia_sdk_test::Simulator;
/// use chia_sdk_types::Conditions;
///
/// let mut sim = Simulator::new();
/// let ctx = &mut SpendContext::new();
///
/// let alice = sim.bls(1);
/// let alice_p2 = StandardLayer::new(alice.pk);
/// let underlying_parent = sim.new_coin(alice.puzzle_hash, 1);
///
/// // An option to buy 1 mojo of XCH for 1 mojo of XCH, which expires at a timestamp of 10.
/// let launcher = OptionLauncher::new(
///     ctx,
///     alice.coin.coin_id(),
///     OptionLauncherInfo::new(alice.puzzle_hash, alice.puzzle_hash, 10, 1, OptionType::Xch { amount: 1 }),
///     1,
/// )?;
///
/// // Lock the underlying coin with the option's p2 puzzle.
/// let p2_option = launcher.p2_puzzle_hash();
/// alice_p2.spend(ctx, underlying_parent, Conditions::new().create_coin(p2_option, 1, Memos::None))?;
/// let underlying_coin = Coin::new(underlying_parent.coin_id(), p2_option, 1);
///
/// let (mint_option, option) = launcher.with_underlying(underlying_coin.coin_id()).mint(ctx)?;
/// alice_p2.spend(ctx, alice.coin, mint_option)?;
/// sim.spend_coins(ctx.take(), &[alice.sk])?;
///
/// assert_eq!(option.info.underlying_coin_id, underlying_coin.coin_id());
/// assert_eq!(option.info.p2_puzzle_hash, alice.puzzle_hash);
/// assert!(sim.coin_state(option.coin.coin_id()).is_some());
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct OptionLauncher<S = UnspecifiedOption> {
    launcher: Launcher,
    state: S,
}

impl<S> OptionLauncher<S> {
    /// Returns the underlying singleton [`Launcher`].
    pub fn launcher(&self) -> &Launcher {
        &self.launcher
    }
}

/// The terms of a new option contract, used to create an [`OptionLauncher`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionLauncherInfo {
    /// The puzzle hash of the creator, who locks up the underlying coin and receives the strike payment.
    pub creator_puzzle_hash: Bytes32,
    /// The p2 puzzle hash of the initial owner of the option.
    pub owner_puzzle_hash: Bytes32,
    /// The absolute timestamp (in seconds) at which the option expires.
    pub seconds: u64,
    /// The amount of the underlying coin.
    pub underlying_amount: u64,
    /// The asset and amount that must be paid to exercise the option.
    pub strike_type: OptionType,
}

impl OptionLauncherInfo {
    /// Creates a new [`OptionLauncherInfo`] from its fields.
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
    /// Creates an option launcher with a launcher amount of 1, created by `parent_coin_id`.
    /// See [`OptionLauncher::with_amount`].
    pub fn new(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
        info: OptionLauncherInfo,
        singleton_amount: u64,
    ) -> Result<Self, DriverError> {
        Self::with_amount(ctx, parent_coin_id, 1, info, singleton_amount)
    }

    /// Creates an option launcher with the given launcher amount and singleton amount, created by
    /// `parent_coin_id`. The launcher coin is hinted with the creator's puzzle hash.
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

    /// Like [`OptionLauncher::with_amount`], but the launcher coin is created separately by the
    /// returned `CREATE_COIN` condition. See [`Launcher::create_early`].
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

    /// Returns the [`OptionUnderlying`] that will lock the underlying coin.
    pub fn underlying(&self) -> OptionUnderlying {
        self.state.underlying
    }

    /// Returns the p2 puzzle hash to lock the underlying coin with. For CATs and NFTs, this is the
    /// inner puzzle hash inside of the asset's layers.
    pub fn p2_puzzle_hash(&self) -> Bytes32 {
        self.state.underlying.tree_hash().into()
    }

    /// Returns the [`OptionMetadata`] that will be stored in the launcher solution.
    pub fn metadata(&self) -> OptionMetadata {
        self.state.metadata
    }

    /// Specifies the coin id of the underlying coin, which must be locked with
    /// [`OptionLauncher::p2_puzzle_hash`], making the launcher ready to mint.
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
    /// Returns the [`OptionInfo`] of the option that will be minted, owned by the initial owner.
    pub fn info(&self) -> OptionInfo {
        self.state.info
    }

    /// Mints the option, sending it to the owner's puzzle hash (hinted).
    ///
    /// The eve option is spent immediately. Returns the conditions that the launcher's parent must
    /// output, and the resulting [`OptionContract`].
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

    /// Mints the eve option with the given p2 puzzle hash, without spending it.
    ///
    /// Returns the conditions that the launcher's parent must output, and the eve [`OptionContract`].
    pub fn mint_eve(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
    ) -> Result<(Conditions, OptionContract), DriverError> {
        let launcher_coin = self.launcher.coin();

        let info = OptionInfo {
            p2_puzzle_hash,
            ..self.state.info
        };

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
    use std::slice;

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

        sim.spend_coins(ctx.take(), slice::from_ref(&alice.sk))?;

        Ok(())
    }
}
