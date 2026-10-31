use chia_protocol::{Bytes, Bytes32, Coin};
use chia_puzzle_types::{EveProof, Proof, singleton::SingletonSolution};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_types::{
    Conditions, announcement_id,
    puzzles::{
        P2SingletonClaimSolution, P2SingletonDelayedSolution, PoolMemberAbsorbSolution,
        PoolWaitingRoomAbsorbSolution, pool_reward_height, pool_reward_prefix,
    },
};
use clvm_traits::{FromClvm, clvm_list};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{
    DriverError, Launcher, Layer, P2SingletonOrDelayedSolution, PoolMemberLayer,
    PoolMemberSolution, PoolWaitingRoomLayer, PoolWaitingRoomSolution, Singleton, SingletonInfo,
    SingletonV1Layer, SpendContext,
};

mod plot_nft_info;
mod pool_state;

pub use plot_nft_info::*;
pub use pool_state::*;

use pool_state::key_value_pairs;

/// The key of the seconds delay in the launcher's key value list.
const SECONDS_DELAY_KEY: &[u8] = b"t";

/// The key of the delayed puzzle hash in the launcher's key value list.
const DELAYED_PUZZLE_HASH_KEY: &[u8] = b"h";

/// The default delay used by the reference wallet before farmed coins can be swept to the delayed
/// puzzle hash.
pub const DEFAULT_P2_SINGLETON_DELAY: u64 = 604_800;

/// A plot NFT, also known as a pool wallet, is a singleton that lets a farmer's plots commit to a
/// pay to singleton puzzle hash rather than to a pool. The owner can then choose where pool rewards
/// are paid to, by joining a pool or self pooling, without replotting.
///
/// Unlike other singletons in the SDK, plot NFTs use the v1.0 singleton top layer, and their
/// [`PoolState`] is only revealed by the spends that change it. To sync one, start at the launcher
/// with [`PlotNft::parse_launcher`] and follow the lineage with [`PlotNft::parse_child`].
pub type PlotNft = Singleton<PlotNftInfo>;

impl PlotNft {
    /// Parses a plot NFT from the solution of its launcher coin. Returns [`None`] if the launcher
    /// isn't for a plot NFT on the network with the given genesis challenge.
    pub fn parse_launcher(
        allocator: &Allocator,
        launcher_coin: Coin,
        solution: NodePtr,
        genesis_challenge: Bytes32,
    ) -> Result<Option<Self>, DriverError> {
        if launcher_coin.puzzle_hash != SINGLETON_LAUNCHER_HASH.into() {
            return Ok(None);
        }

        let (singleton_puzzle_hash, (amount, (key_value_list, _))) =
            <(Bytes32, (u64, (NodePtr, NodePtr)))>::from_clvm(allocator, solution)?;

        // The singleton can't be spent with an even amount
        if amount.is_multiple_of(2) {
            return Ok(None);
        }

        let Some(pool_state) = PoolState::from_key_value_list(allocator, key_value_list) else {
            return Ok(None);
        };

        let mut seconds_delay = None;
        let mut delayed_puzzle_hash = None;

        for (key, value) in key_value_pairs(allocator, key_value_list) {
            let key = allocator.atom(key);

            if key.as_ref() == SECONDS_DELAY_KEY {
                seconds_delay = u64::from_clvm(allocator, value).ok();
            } else if key.as_ref() == DELAYED_PUZZLE_HASH_KEY {
                delayed_puzzle_hash = Bytes32::from_clvm(allocator, value).ok();
            }
        }

        let (Some(seconds_delay), Some(delayed_puzzle_hash)) = (seconds_delay, delayed_puzzle_hash)
        else {
            return Ok(None);
        };

        let launcher_id = launcher_coin.coin_id();

        let info = PlotNftInfo::new(
            launcher_id,
            pool_state,
            seconds_delay,
            delayed_puzzle_hash,
            pool_reward_prefix(genesis_challenge),
        );

        // None of the launcher's key value list is enforced, so it could be for anything
        if info.puzzle_hash() != singleton_puzzle_hash.into() {
            return Ok(None);
        }

        Ok(Some(Self::new(
            Coin::new(launcher_id, singleton_puzzle_hash, amount),
            Proof::Eve(EveProof {
                parent_parent_coin_info: launcher_coin.parent_coin_info,
                parent_amount: launcher_coin.amount,
            }),
            info,
        )))
    }

    /// Parses the child of this plot NFT from the solution it was spent with.
    ///
    /// Returns [`None`] if the owner moved out of the waiting room to a puzzle which doesn't match
    /// the state they revealed, since the waiting room lets them go anywhere and then nothing is
    /// known about the child. Leaving a pool can only go to one puzzle, so if the state revealed
    /// doesn't match it, the child is still parsed as a [`PoolSingletonState::LeavingPool`] state.
    pub fn parse_child(
        &self,
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Option<Self>, DriverError> {
        // The puzzles ignore anything after the arguments they use
        let (_, (_, (inner_solution, _))) =
            <(NodePtr, (NodePtr, (NodePtr, NodePtr)))>::from_clvm(allocator, solution)?;

        let unchanged = || self.child_with(self.info.clone(), self.coin.amount);

        if !self.info.pool_state.is_waiting_room() {
            let Some(extra_data) = PoolMemberLayer::parse_escape(allocator, inner_solution)? else {
                return Ok(Some(unchanged()));
            };

            let waiting_room = self.info.waiting_room_layer().tree_hash();

            let pool_state = PoolState::from_key_value_list(allocator, extra_data)
                .filter(|state| {
                    self.info.with_pool_state(state.clone()).inner_puzzle_hash() == waiting_room
                })
                .unwrap_or_else(|| self.info.pool_state.leaving());

            let info = self.info.with_pool_state(pool_state);

            return Ok(Some(self.child_with(info, self.coin.amount)));
        }

        let Some((extra_data, destination)) =
            PoolWaitingRoomLayer::parse_travel(allocator, inner_solution)?
        else {
            return Ok(Some(unchanged()));
        };

        let Some(pool_state) = PoolState::from_key_value_list(allocator, extra_data) else {
            return Ok(None);
        };

        let info = self.info.with_pool_state(pool_state);

        if Bytes32::from_clvm(allocator, destination).ok() != Some(info.inner_puzzle_hash().into())
        {
            return Ok(None);
        }

        Ok(Some(self.child_with(info, self.coin.amount)))
    }

    /// Starts leaving a pool by moving to the waiting room puzzle, which must be signed by the
    /// owner. The plot NFT can leave the waiting room after the relative lock height.
    pub fn leave_pool(&self, ctx: &mut SpendContext) -> Result<Self, DriverError> {
        if self.info.pool_state.state != PoolSingletonState::FarmingToPool {
            return Err(DriverError::InvalidPlotNftTransition(
                "only a plot NFT that is farming to a pool can leave it",
            ));
        }

        let pool_state = self.info.pool_state.leaving();

        let extra_data = ctx.alloc(&[pool_state.key_value()?])?;

        self.spend_member(ctx, PoolMemberSolution::Escape { extra_data })?;

        Ok(self.child_with(self.info.with_pool_state(pool_state), self.coin.amount))
    }

    /// Joins a pool, which must be signed by the owner. A plot NFT that's already in a pool must
    /// use [`PlotNft::leave_pool`] and wait for the relative lock height first.
    ///
    /// Rewards are always paid to the current target puzzle hash at the time of the claim, so any
    /// that have already been farmed while self pooling must be claimed with
    /// [`PlotNft::claim_rewards`] first, otherwise they can be claimed for the pool. That has to be
    /// confirmed in an earlier block, since this spend asserts a relative height, which isn't
    /// allowed on a coin created in the same block.
    pub fn join_pool(
        &self,
        ctx: &mut SpendContext,
        target_puzzle_hash: Bytes32,
        pool_url: String,
        relative_lock_height: u32,
    ) -> Result<Self, DriverError> {
        self.travel(
            ctx,
            PoolState::farming_to_pool(
                target_puzzle_hash,
                self.info.pool_state.owner_public_key,
                pool_url,
                relative_lock_height,
            ),
        )
    }

    /// Starts self pooling, which must be signed by the owner. A plot NFT that's in a pool must use
    /// [`PlotNft::leave_pool`] and wait for the relative lock height first.
    pub fn self_pool(
        &self,
        ctx: &mut SpendContext,
        target_puzzle_hash: Bytes32,
    ) -> Result<Self, DriverError> {
        self.travel(
            ctx,
            PoolState::self_pooling(target_puzzle_hash, self.info.pool_state.owner_public_key),
        )
    }

    /// Moves out of the waiting room to the given state. The chain enforces the relative lock
    /// height, which is counted from [`PlotNft::coin`](Singleton::coin) being created. That means
    /// that the lock restarts if a reward is claimed in the meantime.
    pub(crate) fn travel(
        &self,
        ctx: &mut SpendContext,
        pool_state: PoolState,
    ) -> Result<Self, DriverError> {
        if !self.info.pool_state.is_waiting_room() {
            return Err(DriverError::InvalidPlotNftTransition(
                "a plot NFT that is farming to a pool must leave it first",
            ));
        }

        pool_state.validate()?;

        let info = self.info.with_pool_state(pool_state);
        let destination_puzzle_hash = info.inner_puzzle_hash().into();

        if destination_puzzle_hash == self.info.inner_puzzle_hash().into() {
            return Err(DriverError::InvalidPlotNftTransition(
                "the plot NFT is already in this state",
            ));
        }

        let extra_data = ctx.alloc(&[info.pool_state.key_value()?])?;

        self.spend_waiting_room(
            ctx,
            PoolWaitingRoomSolution::Travel {
                extra_data,
                destination_puzzle_hash,
            },
        )?;

        Ok(self.child_with(info, self.coin.amount))
    }

    /// Claims pool rewards that were farmed to [`PlotNftInfo::p2_puzzle_hash`], which pays them to
    /// the current target puzzle hash. The singleton is recreated once for each reward, in the
    /// order provided. Nothing has to be signed, so anyone can do this.
    ///
    /// Note that this restarts the relative lock height if the plot NFT is in the waiting room.
    ///
    /// To pay a fee, add the returned conditions to the spend that pays it, so that the two can't
    /// be separated.
    pub fn claim_rewards(
        &self,
        ctx: &mut SpendContext,
        rewards: &[Coin],
    ) -> Result<(Conditions, Self), DriverError> {
        let Some(first) = rewards.first() else {
            return Err(DriverError::InvalidPoolReward("no rewards to claim"));
        };

        let is_waiting_room = self.info.pool_state.is_waiting_room();

        // Nothing can be added to the context until every reward is known to be valid
        let heights = rewards
            .iter()
            .map(|reward| self.reward_height(reward, is_waiting_room))
            .collect::<Result<Vec<_>, _>>()?;

        let mut plot_nft = self.clone();

        for (&reward, pool_reward_height) in rewards.iter().zip(heights) {
            let pool_reward_amount = reward.amount;

            if is_waiting_room {
                plot_nft.spend_waiting_room(
                    ctx,
                    PoolWaitingRoomSolution::Absorb(PoolWaitingRoomAbsorbSolution {
                        spend_type: (),
                        pool_reward_amount,
                        pool_reward_height,
                    }),
                )?;
            } else {
                plot_nft.spend_member(
                    ctx,
                    PoolMemberSolution::Absorb(PoolMemberAbsorbSolution {
                        pool_reward_amount,
                        pool_reward_height,
                    }),
                )?;
            }

            let coin_spend = self.info.p2_layer().construct_coin_spend(
                ctx,
                reward,
                P2SingletonOrDelayedSolution::Claim(P2SingletonClaimSolution {
                    singleton_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
                    my_id: reward.coin_id(),
                }),
            )?;

            ctx.insert(coin_spend);

            plot_nft = plot_nft.child_with(plot_nft.info.clone(), plot_nft.coin.amount);
        }

        Ok((
            Conditions::new().assert_coin_announcement(announcement_id(first.coin_id(), b"$")),
            plot_nft,
        ))
    }

    fn reward_height(&self, reward: &Coin, is_waiting_room: bool) -> Result<u32, DriverError> {
        if reward.puzzle_hash != self.info.p2_puzzle_hash().into() {
            return Err(DriverError::InvalidPoolReward(
                "the coin is not locked to the plot NFT's p2 puzzle hash",
            ));
        }

        let Some(height) =
            pool_reward_height(self.info.pool_reward_prefix, reward.parent_coin_info)
        else {
            return Err(DriverError::InvalidPoolReward(
                "the coin is not a pool reward, so it can only be claimed after the delay",
            ));
        };

        // Only the singleton itself can have an odd amount
        if !reward.amount.is_multiple_of(2) {
            return Err(DriverError::InvalidPoolReward(
                "a pool reward with an odd amount can't be claimed by the singleton",
            ));
        }

        if height == 0 && !is_waiting_room {
            return Err(DriverError::InvalidPoolReward(
                "a reward height of 0 would be encoded as nil, which means leaving the pool",
            ));
        }

        Ok(height)
    }

    fn spend_member(
        &self,
        ctx: &mut SpendContext,
        inner_solution: PoolMemberSolution,
    ) -> Result<(), DriverError> {
        let layer = SingletonV1Layer::new(self.info.launcher_id, self.info.member_layer());
        let coin_spend = layer.construct_coin_spend(
            ctx,
            self.coin,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution,
            },
        )?;
        ctx.insert(coin_spend);
        Ok(())
    }

    fn spend_waiting_room(
        &self,
        ctx: &mut SpendContext,
        inner_solution: PoolWaitingRoomSolution,
    ) -> Result<(), DriverError> {
        let layer = SingletonV1Layer::new(self.info.launcher_id, self.info.waiting_room_layer());
        let coin_spend = layer.construct_coin_spend(
            ctx,
            self.coin,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution,
            },
        )?;
        ctx.insert(coin_spend);
        Ok(())
    }
}

impl PlotNftInfo {
    /// Sends a coin locked to the [`PlotNftInfo::p2_puzzle_hash`] to the delayed puzzle hash, which
    /// is the only way to spend coins that aren't pool rewards. The coin must have existed for
    /// [`PlotNftInfo::seconds_delay`]. Nothing has to be signed, so anyone can do this.
    pub fn claim_delayed(&self, ctx: &mut SpendContext, coin: Coin) -> Result<Coin, DriverError> {
        if coin.puzzle_hash != self.p2_puzzle_hash().into() {
            return Err(DriverError::InvalidPoolReward(
                "the coin is not locked to the plot NFT's p2 puzzle hash",
            ));
        }

        let coin_spend = self.p2_layer().construct_coin_spend(
            ctx,
            coin,
            P2SingletonOrDelayedSolution::Delayed(P2SingletonDelayedSolution {
                amount: coin.amount,
                my_id: (),
            }),
        )?;

        ctx.insert(coin_spend);

        Ok(Coin::new(
            coin.coin_id(),
            self.delayed_puzzle_hash,
            coin.amount,
        ))
    }
}

impl Launcher {
    /// Launches a plot NFT in the given state, which must be either self pooling or farming to a
    /// pool. The owner can't be changed afterward, and neither can the seconds delay or delayed
    /// puzzle hash, since they determine the p2 puzzle hash which plots are created for.
    ///
    /// The reference wallet uses a launcher amount of 1 and a delay of
    /// [`DEFAULT_P2_SINGLETON_DELAY`].
    pub fn mint_plot_nft(
        self,
        ctx: &mut SpendContext,
        pool_state: PoolState,
        seconds_delay: u64,
        delayed_puzzle_hash: Bytes32,
        genesis_challenge: Bytes32,
    ) -> Result<(Conditions, PlotNft), DriverError> {
        if pool_state.state == PoolSingletonState::LeavingPool {
            return Err(DriverError::InvalidPoolState(
                "a plot NFT can't be launched in the leaving pool state",
            ));
        }

        pool_state.validate()?;

        if self.singleton_amount().is_multiple_of(2) {
            return Err(DriverError::InvalidPoolState(
                "the singleton amount must be odd",
            ));
        }

        let launcher_coin = self.coin();
        let launcher_id = launcher_coin.coin_id();
        let info = PlotNftInfo::new(
            launcher_id,
            pool_state,
            seconds_delay,
            delayed_puzzle_hash,
            pool_reward_prefix(genesis_challenge),
        );

        let (key, value): (Bytes, Bytes) = info.pool_state.key_value()?;
        let key_value_list = ctx.alloc(&clvm_list!(
            (key, value),
            (Bytes::from(SECONDS_DELAY_KEY.to_vec()), seconds_delay),
            (
                Bytes::from(DELAYED_PUZZLE_HASH_KEY.to_vec()),
                delayed_puzzle_hash
            )
        ))?;

        let (parent_conditions, coin) =
            self.spend_to_puzzle_hash(ctx, info.puzzle_hash().into(), key_value_list)?;

        Ok((
            parent_conditions,
            PlotNft::new(
                coin,
                Proof::Eve(EveProof {
                    parent_parent_coin_info: launcher_coin.parent_coin_info,
                    parent_amount: launcher_coin.amount,
                }),
                info,
            ),
        ))
    }
}

#[cfg(test)]
mod tests;
