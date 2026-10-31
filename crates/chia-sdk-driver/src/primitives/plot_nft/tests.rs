use chia_bls::Signature;
use chia_consensus::validation_error::ErrorCode;
use chia_protocol::{Bytes, Coin, Program, SpendBundle};
use chia_puzzles::SINGLETON_LAUNCHER;
use chia_sdk_test::{BlsPair, Simulator, SimulatorError};
use chia_sdk_types::{MAINNET_CONSTANTS, TESTNET11_CONSTANTS, puzzles::pool_reward_parent_id};
use chia_traits::Streamable;
use clvm_traits::{ToClvm, clvm_list, clvm_tuple};
use clvmr::serde::node_from_bytes;
use rstest::rstest;

use crate::{P2SingletonOrDelayedLayer, Puzzle, SingletonLayer, StandardLayer};

use super::*;

const POOL_A: Bytes32 = Bytes32::new([0xaa; 32]);
const POOL_B: Bytes32 = Bytes32::new([0xbb; 32]);
const SELF_TARGET: Bytes32 = Bytes32::new([0x11; 32]);
const DELAYED: Bytes32 = Bytes32::new([0x22; 32]);
const REWARD: u64 = 1_750_000_000_000;
const DELAY: u64 = 1000;

struct Test {
    sim: Simulator,
    ctx: SpendContext,
    owner: BlsPair,
    genesis_challenge: Bytes32,
    next_reward_height: u32,
}

impl Test {
    fn new() -> Self {
        Self {
            sim: Simulator::new(),
            ctx: SpendContext::new(),
            owner: BlsPair::new(42),
            genesis_challenge: TESTNET11_CONSTANTS.genesis_challenge,
            next_reward_height: 100,
        }
    }

    fn launch(&mut self, state: PoolState) -> anyhow::Result<PlotNft> {
        let parent = self.sim.bls(1);

        let (conditions, plot_nft) = Launcher::new(parent.coin.coin_id(), 1).mint_plot_nft(
            &mut self.ctx,
            state,
            DELAY,
            DELAYED,
            self.genesis_challenge,
        )?;

        StandardLayer::new(parent.pk).spend(&mut self.ctx, parent.coin, conditions)?;

        let launcher = self
            .ctx
            .iter()
            .find(|cs| cs.coin.puzzle_hash == SINGLETON_LAUNCHER_HASH.into())
            .unwrap()
            .clone();

        self.sim.spend_coins(self.ctx.take(), &[parent.sk])?;

        // The launcher spend should be enough to get back to the same plot NFT
        let solution = node_from_bytes(&mut self.ctx, &launcher.solution)?;
        let parsed =
            PlotNft::parse_launcher(&self.ctx, launcher.coin, solution, self.genesis_challenge)?;
        assert_eq!(parsed.as_ref(), Some(&plot_nft));

        Ok(plot_nft)
    }

    fn self_pooling_state(&self) -> PoolState {
        PoolState::self_pooling(SELF_TARGET, self.owner.pk)
    }

    fn pool_state(&self, target_puzzle_hash: Bytes32, relative_lock_height: u32) -> PoolState {
        PoolState::farming_to_pool(
            target_puzzle_hash,
            self.owner.pk,
            "https://pool.example.com".to_string(),
            relative_lock_height,
        )
    }

    fn reward(&mut self, plot_nft: &PlotNft, amount: u64) -> Coin {
        let height = self.next_reward_height;
        self.next_reward_height += 1;

        let coin = Coin::new(
            pool_reward_parent_id(plot_nft.info.pool_reward_prefix, height),
            plot_nft.info.p2_puzzle_hash().into(),
            amount,
        );

        self.sim.insert_coin(coin);
        coin
    }

    /// Pushes the pending spends, and checks that syncing from them gives the expected plot NFT.
    fn confirm(&mut self, parent: &PlotNft, child: &PlotNft) -> anyhow::Result<()> {
        self.push()?;
        self.assert_synced(parent, child)
    }

    fn assert_synced(&mut self, parent: &PlotNft, child: &PlotNft) -> anyhow::Result<()> {
        let mut synced = parent.clone();

        while synced.coin != child.coin {
            let coin_spend = self.sim.coin_spend(synced.coin.coin_id()).unwrap();
            let solution = node_from_bytes(&mut self.ctx, &coin_spend.solution)?;
            synced = synced.parse_child(&self.ctx, solution)?.unwrap();
        }

        assert_eq!(&synced, child);
        assert!(self.sim.coin_state(child.coin.coin_id()).is_some());
        Ok(())
    }

    fn push(&mut self) -> Result<(), SimulatorError> {
        let coin_spends = self.ctx.take();
        let result = self
            .sim
            .spend_coins(coin_spends, std::slice::from_ref(&self.owner.sk));
        result.map(|_| ())
    }

    fn pass_blocks(&mut self, blocks: u32) {
        for _ in 0..blocks {
            self.sim.create_block();
        }
    }

    fn paid(&self, parent_coin: Coin, puzzle_hash: Bytes32, amount: u64) -> bool {
        self.sim
            .coin_state(Coin::new(parent_coin.coin_id(), puzzle_hash, amount).coin_id())
            .is_some()
    }
}

fn assert_failed<T: std::fmt::Debug>(result: Result<T, SimulatorError>, code: ErrorCode) {
    match result {
        Err(SimulatorError::Validation(error)) => assert_eq!(error, code),
        other => panic!("expected {code:?}, got {other:?}"),
    }
}

#[test]
fn test_full_lifecycle() -> anyhow::Result<()> {
    let mut t = Test::new();

    // Launching
    let state = t.self_pooling_state();
    let plot_nft = t.launch(state)?;
    assert_eq!(plot_nft.coin.amount, 1);
    assert!(plot_nft.info.pool_state.is_waiting_room());

    // Rewards while self pooling go to the self pooling target
    let reward = t.reward(&plot_nft, REWARD);
    let (_, next) = plot_nft.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&plot_nft, &next)?;
    assert!(t.paid(plot_nft.coin, SELF_TARGET, REWARD));
    let plot_nft = next;

    // Joining a pool has no delay since the relative lock height is 0
    let next = plot_nft.join_pool(
        &mut t.ctx,
        POOL_A,
        "https://pool.example.com".to_string(),
        10,
    )?;
    t.confirm(&plot_nft, &next)?;
    assert_eq!(
        next.info.pool_state.state,
        PoolSingletonState::FarmingToPool
    );
    let plot_nft = next;

    // Rewards while in a pool go to the pool, and the p2 puzzle hash stays the same
    let reward = t.reward(&plot_nft, REWARD);
    let (_, next) = plot_nft.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&plot_nft, &next)?;
    assert!(t.paid(plot_nft.coin, POOL_A, REWARD));
    let plot_nft = next;

    // Leaving keeps the pool's target puzzle hash, URL and relative lock height
    let next = plot_nft.leave_pool(&mut t.ctx)?;
    t.confirm(&plot_nft, &next)?;
    assert_eq!(next.info.pool_state.state, PoolSingletonState::LeavingPool);
    assert_eq!(next.info.pool_state.target_puzzle_hash, POOL_A);
    assert_eq!(next.info.pool_state.relative_lock_height, 10);
    let plot_nft = next;

    // Rewards while leaving still go to the pool
    let reward = t.reward(&plot_nft, REWARD);
    let (_, next) = plot_nft.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&plot_nft, &next)?;
    assert!(t.paid(plot_nft.coin, POOL_A, REWARD));
    let plot_nft = next;

    // Going back to self pooling after waiting
    t.pass_blocks(10);
    let next = plot_nft.self_pool(&mut t.ctx, SELF_TARGET)?;
    t.confirm(&plot_nft, &next)?;
    assert_eq!(next.info.pool_state, t.self_pooling_state());
    let plot_nft = next;

    let reward = t.reward(&plot_nft, REWARD);
    let (_, next) = plot_nft.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&plot_nft, &next)?;
    assert!(t.paid(plot_nft.coin, SELF_TARGET, REWARD));

    Ok(())
}

#[test]
fn test_launch_into_pool_then_switch_pools() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.pool_state(POOL_A, 5);
    let plot_nft = t.launch(state)?;
    assert!(!plot_nft.info.pool_state.is_waiting_room());

    let next = plot_nft.leave_pool(&mut t.ctx)?;
    t.confirm(&plot_nft, &next)?;
    let plot_nft = next;

    t.pass_blocks(5);

    let next = plot_nft.join_pool(
        &mut t.ctx,
        POOL_B,
        "https://other.example.com".to_string(),
        20,
    )?;
    t.confirm(&plot_nft, &next)?;
    assert_eq!(next.info.pool_state.target_puzzle_hash, POOL_B);
    assert_eq!(
        next.info.pool_state.pool_url.as_deref(),
        Some("https://other.example.com")
    );
    assert_eq!(next.info.pool_state.relative_lock_height, 20);

    Ok(())
}

#[test]
fn test_change_self_pooling_target() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.self_pooling_state();
    let plot_nft = t.launch(state)?;

    let next = plot_nft.self_pool(&mut t.ctx, POOL_B)?;
    t.confirm(&plot_nft, &next)?;
    assert_eq!(next.info.pool_state.target_puzzle_hash, POOL_B);

    Ok(())
}

#[rstest]
#[case(5)]
#[case(1000)]
fn test_waiting_room_lock_boundary(#[case] relative_lock_height: u32) -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.pool_state(POOL_A, relative_lock_height);
    let plot_nft = t.launch(state)?;

    let waiting = plot_nft.leave_pool(&mut t.ctx)?;
    t.confirm(&plot_nft, &waiting)?;

    let created_height = t
        .sim
        .coin_state(waiting.coin.coin_id())
        .unwrap()
        .created_height
        .unwrap();

    // Too early, both right away and on the last block before the lock is up
    let _ = waiting.self_pool(&mut t.ctx, SELF_TARGET)?;
    assert_failed(t.push(), ErrorCode::AssertHeightRelativeFailed);

    while t.sim.height() < created_height + relative_lock_height - 1 {
        t.sim.create_block();
    }

    let _ = waiting.join_pool(&mut t.ctx, POOL_B, "https://b.example.com".to_string(), 5)?;
    assert_failed(t.push(), ErrorCode::AssertHeightRelativeFailed);

    t.sim.create_block();
    assert_eq!(t.sim.height(), created_height + relative_lock_height);

    let next = waiting.self_pool(&mut t.ctx, SELF_TARGET)?;
    t.confirm(&waiting, &next)?;

    Ok(())
}

#[test]
fn test_claiming_restarts_lock() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.pool_state(POOL_A, 5);
    let plot_nft = t.launch(state)?;

    let waiting = plot_nft.leave_pool(&mut t.ctx)?;
    t.confirm(&plot_nft, &waiting)?;

    t.pass_blocks(5);

    // Anyone can claim a reward, which stops the owner from leaving for a while
    let reward = t.reward(&waiting, REWARD);
    let (_, claimed) = waiting.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&waiting, &claimed)?;

    let _ = claimed.self_pool(&mut t.ctx, SELF_TARGET)?;
    assert_failed(t.push(), ErrorCode::AssertHeightRelativeFailed);

    t.pass_blocks(5);

    let next = claimed.self_pool(&mut t.ctx, SELF_TARGET)?;
    t.confirm(&claimed, &next)?;

    Ok(())
}

#[test]
fn test_travel_requires_owner_signature() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.pool_state(POOL_A, 5);
    let plot_nft = t.launch(state)?;

    let _ = plot_nft.leave_pool(&mut t.ctx)?;
    let result = t
        .sim
        .new_transaction(SpendBundle::new(t.ctx.take(), Signature::default()));
    assert_failed(result, ErrorCode::BadAggregateSignature);

    let waiting = plot_nft.leave_pool(&mut t.ctx)?;
    t.confirm(&plot_nft, &waiting)?;
    t.pass_blocks(5);

    let _ = waiting.self_pool(&mut t.ctx, SELF_TARGET)?;
    let result = t
        .sim
        .new_transaction(SpendBundle::new(t.ctx.take(), Signature::default()));
    assert_failed(result, ErrorCode::BadAggregateSignature);

    Ok(())
}

#[test]
fn test_claim_many_rewards_with_fee() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.pool_state(POOL_A, 5);
    let plot_nft = t.launch(state)?;

    let rewards = [
        t.reward(&plot_nft, REWARD),
        t.reward(&plot_nft, REWARD),
        t.reward(&plot_nft, REWARD / 2),
    ];

    let (conditions, next) = plot_nft.claim_rewards(&mut t.ctx, &rewards)?;

    // Claiming doesn't need a signature, only the fee does
    let payer = t.sim.bls(100);
    StandardLayer::new(payer.pk).spend(&mut t.ctx, payer.coin, conditions.reserve_fee(100))?;
    t.sim.spend_coins(t.ctx.take(), &[payer.sk])?;

    t.assert_synced(&plot_nft, &next)?;

    for reward in rewards {
        assert!(
            t.sim
                .coin_state(reward.coin_id())
                .unwrap()
                .spent_height
                .is_some()
        );
    }

    // The fee can't go through without the rewards being claimed
    let reward = t.reward(&next, REWARD);
    let (conditions, _) = next.claim_rewards(&mut t.ctx, &[reward])?;
    t.ctx.take();
    let payer = t.sim.bls(100);
    StandardLayer::new(payer.pk).spend(&mut t.ctx, payer.coin, conditions.reserve_fee(100))?;
    assert_failed(
        t.sim.spend_coins(t.ctx.take(), &[payer.sk]),
        ErrorCode::AssertCoinAnnouncementFailed,
    );

    Ok(())
}

#[test]
fn test_cannot_travel_from_coin_created_in_same_block() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.self_pooling_state();
    let plot_nft = t.launch(state)?;

    // The relative height condition isn't allowed on an ephemeral coin, even with a lock of 0,
    // so rewards have to be claimed in an earlier block than the one that joins a pool
    let reward = t.reward(&plot_nft, REWARD);
    let (_, claimed) = plot_nft.claim_rewards(&mut t.ctx, &[reward])?;
    let _ = claimed.join_pool(
        &mut t.ctx,
        POOL_A,
        "https://pool.example.com".to_string(),
        5,
    )?;
    assert_failed(t.push(), ErrorCode::EphemeralRelativeCondition);

    // The two spends work one after the other
    let reward_spends = {
        let (_, claimed) = plot_nft.claim_rewards(&mut t.ctx, &[reward])?;
        t.confirm(&plot_nft, &claimed)?;
        claimed
    };
    let next = reward_spends.join_pool(
        &mut t.ctx,
        POOL_A,
        "https://pool.example.com".to_string(),
        5,
    )?;
    t.confirm(&reward_spends, &next)?;

    Ok(())
}

#[test]
fn test_rewards_go_to_pool_if_not_claimed_before_joining() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.self_pooling_state();
    let plot_nft = t.launch(state)?;

    let reward = t.reward(&plot_nft, REWARD);

    let next = plot_nft.join_pool(
        &mut t.ctx,
        POOL_A,
        "https://pool.example.com".to_string(),
        5,
    )?;
    t.confirm(&plot_nft, &next)?;

    let (_, claimed) = next.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&next, &claimed)?;
    assert!(t.paid(next.coin, POOL_A, REWARD));

    Ok(())
}

#[test]
fn test_claim_reward_from_older_state() -> anyhow::Result<()> {
    let mut t = Test::new();

    // The reward's puzzle hash only depends on the launcher, so it survives state changes
    let state = t.pool_state(POOL_A, 5);
    let plot_nft = t.launch(state)?;

    let reward = t.reward(&plot_nft, REWARD);

    let next = plot_nft.leave_pool(&mut t.ctx)?;
    t.confirm(&plot_nft, &next)?;

    let (_, claimed) = next.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&next, &claimed)?;

    Ok(())
}

#[test]
fn test_invalid_claims() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.pool_state(POOL_A, 5);
    let plot_nft = t.launch(state)?;
    let p2_puzzle_hash = plot_nft.info.p2_puzzle_hash().into();
    let prefix = plot_nft.info.pool_reward_prefix;
    let parent = pool_reward_parent_id(prefix, 100);

    for rewards in [
        vec![],
        // Wrong puzzle hash
        vec![Coin::new(parent, POOL_A, REWARD)],
        // A coin that was just sent to the address
        vec![Coin::new(Bytes32::new([1; 32]), p2_puzzle_hash, REWARD)],
        // Would create a second odd coin, which the singleton doesn't allow
        vec![Coin::new(parent, p2_puzzle_hash, REWARD + 1)],
        // The pool member puzzle would treat this as leaving the pool
        vec![Coin::new(
            pool_reward_parent_id(prefix, 0),
            p2_puzzle_hash,
            REWARD,
        )],
    ] {
        assert!(matches!(
            plot_nft.claim_rewards(&mut t.ctx, &rewards),
            Err(DriverError::InvalidPoolReward(_))
        ));
        t.ctx.take();
    }

    Ok(())
}

#[test]
fn test_odd_reward_fails_on_chain() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.pool_state(POOL_A, 5);
    let plot_nft = t.launch(state)?;

    // Bypasses the check in `claim_rewards`, to make sure it's needed
    let reward = t.reward(&plot_nft, REWARD + 1);

    plot_nft.spend_member(
        &mut t.ctx,
        PoolMemberSolution::Absorb(PoolMemberAbsorbSolution {
            pool_reward_amount: reward.amount,
            pool_reward_height: 100,
        }),
    )?;
    let coin_spend = plot_nft.info.p2_layer().construct_coin_spend(
        &mut t.ctx,
        reward,
        P2SingletonOrDelayedSolution::Claim(P2SingletonClaimSolution {
            singleton_inner_puzzle_hash: plot_nft.info.inner_puzzle_hash().into(),
            my_id: reward.coin_id(),
        }),
    )?;
    t.ctx.insert(coin_spend);

    assert!(t.push().is_err());

    Ok(())
}

#[test]
fn test_invalid_transitions() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.self_pooling_state();
    let self_pooling = t.launch(state)?;

    let state = t.pool_state(POOL_A, 5);
    let farming = t.launch(state)?;

    let url = "https://pool.example.com".to_string();

    // These are checked before any spends are created
    assert!(matches!(
        self_pooling.leave_pool(&mut t.ctx),
        Err(DriverError::InvalidPlotNftTransition(_))
    ));
    assert!(matches!(
        farming.join_pool(&mut t.ctx, POOL_B, url.clone(), 5),
        Err(DriverError::InvalidPlotNftTransition(_))
    ));
    assert!(matches!(
        farming.self_pool(&mut t.ctx, SELF_TARGET),
        Err(DriverError::InvalidPlotNftTransition(_))
    ));
    assert!(matches!(
        self_pooling.self_pool(&mut t.ctx, SELF_TARGET),
        Err(DriverError::InvalidPlotNftTransition(_))
    ));

    for (relative_lock_height, pool_url) in [(4, url.clone()), (1001, url), (5, String::new())] {
        assert!(matches!(
            self_pooling.join_pool(&mut t.ctx, POOL_B, pool_url, relative_lock_height),
            Err(DriverError::InvalidPoolState(_))
        ));
    }

    assert_eq!(t.ctx.iter().count(), 0);

    // A plot NFT in the waiting room can't leave again
    let waiting = farming.leave_pool(&mut t.ctx)?;
    t.ctx.take();
    assert!(matches!(
        waiting.leave_pool(&mut t.ctx),
        Err(DriverError::InvalidPlotNftTransition(_))
    ));

    Ok(())
}

#[test]
fn test_invalid_launches() {
    let mut ctx = SpendContext::new();
    let owner = BlsPair::new(42).pk;
    let genesis = TESTNET11_CONSTANTS.genesis_challenge;
    let parent = Bytes32::new([1; 32]);

    let leaving = PoolState {
        state: PoolSingletonState::LeavingPool,
        ..PoolState::farming_to_pool(POOL_A, owner, "https://a.b".to_string(), 5)
    };
    let self_pooling = PoolState::self_pooling(SELF_TARGET, owner);

    for state in [
        leaving,
        PoolState {
            pool_url: Some("https://a.b".to_string()),
            ..self_pooling.clone()
        },
        PoolState {
            relative_lock_height: 1,
            ..self_pooling.clone()
        },
        PoolState {
            version: POOL_PROTOCOL_VERSION + 1,
            ..self_pooling.clone()
        },
    ] {
        let result =
            Launcher::new(parent, 1).mint_plot_nft(&mut ctx, state, DELAY, DELAYED, genesis);
        assert!(matches!(result, Err(DriverError::InvalidPoolState(_))));
    }

    let result =
        Launcher::new(parent, 2).mint_plot_nft(&mut ctx, self_pooling, DELAY, DELAYED, genesis);
    assert!(matches!(result, Err(DriverError::InvalidPoolState(_))));
}

#[test]
fn test_delayed_claim() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.self_pooling_state();
    let plot_nft = t.launch(state)?;

    // An ordinary payment to the address can't be absorbed, so this is the only way to spend it
    let coin = t.sim.new_coin(plot_nft.info.p2_puzzle_hash().into(), 1234);
    let created_timestamp = t.sim.next_timestamp();
    t.sim.create_block();

    let _ = plot_nft.info.claim_delayed(&mut t.ctx, coin)?;
    assert_failed(t.push(), ErrorCode::AssertSecondsRelativeFailed);

    t.sim.set_next_timestamp(created_timestamp + DELAY - 1)?;
    let _ = plot_nft.info.claim_delayed(&mut t.ctx, coin)?;
    assert_failed(t.push(), ErrorCode::AssertSecondsRelativeFailed);

    t.sim.set_next_timestamp(created_timestamp + DELAY)?;
    let swept = plot_nft.info.claim_delayed(&mut t.ctx, coin)?;
    t.sim.spend_coins(t.ctx.take(), &[])?;
    assert_eq!(swept.puzzle_hash, DELAYED);
    assert!(t.sim.coin_state(swept.coin_id()).is_some());

    // The singleton isn't affected
    let reward = t.reward(&plot_nft, REWARD);
    let (_, next) = plot_nft.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&plot_nft, &next)?;

    assert!(matches!(
        plot_nft
            .info
            .claim_delayed(&mut t.ctx, Coin::new(coin.coin_id(), POOL_A, 1)),
        Err(DriverError::InvalidPoolReward(_))
    ));

    Ok(())
}

/// Absorbing is permissionless, so anyone can get these solutions on-chain.
#[rstest]
#[case(false, false)]
#[case(false, true)]
#[case(true, false)]
#[case(true, true)]
fn test_sync_through_unusual_absorb_solutions(
    #[case] waiting_room: bool,
    #[case] noncanonical_height: bool,
) -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = if waiting_room {
        t.self_pooling_state()
    } else {
        t.pool_state(POOL_A, 5)
    };
    let plot_nft = t.launch(state)?;
    let reward = t.reward(&plot_nft, REWARD);
    let height = t.next_reward_height - 1;

    let height: Vec<u8> = if noncanonical_height {
        vec![0, 0, 0, 0, 0, height.to_be_bytes()[3]]
    } else {
        vec![height.to_be_bytes()[3]]
    };
    let height = Bytes::from(height);
    let garbage = Bytes::from(vec![1, 2, 3]);

    let inner_solution = if waiting_room {
        t.ctx
            .alloc(&clvm_list!((), REWARD, height, garbage.clone()))?
    } else {
        t.ctx.alloc(&clvm_list!(REWARD, height, garbage.clone()))?
    };

    let puzzle = SingletonV1Layer::new(
        plot_nft.info.launcher_id,
        plot_nft.info.waiting_room_layer(),
    );
    let puzzle = if waiting_room {
        puzzle.construct_puzzle(&mut t.ctx)?
    } else {
        SingletonV1Layer::new(plot_nft.info.launcher_id, plot_nft.info.member_layer())
            .construct_puzzle(&mut t.ctx)?
    };

    let solution = t.ctx.alloc(&clvm_tuple!(
        plot_nft.proof,
        plot_nft.coin.amount,
        inner_solution,
        garbage.clone()
    ))?;

    t.ctx
        .spend(plot_nft.coin, crate::Spend::new(puzzle, solution))?;

    let coin_spend = plot_nft.info.p2_layer().construct_coin_spend(
        &mut t.ctx,
        reward,
        P2SingletonOrDelayedSolution::Claim(P2SingletonClaimSolution {
            singleton_inner_puzzle_hash: plot_nft.info.inner_puzzle_hash().into(),
            my_id: reward.coin_id(),
        }),
    )?;
    t.ctx.insert(coin_spend);

    t.push()?;

    let solution = t.sim.solution(plot_nft.coin.coin_id()).unwrap();
    let solution = node_from_bytes(&mut t.ctx, &solution)?;
    let child = plot_nft.parse_child(&t.ctx, solution)?.unwrap();

    assert_eq!(child.info, plot_nft.info);
    assert!(t.sim.coin_state(child.coin.coin_id()).is_some());

    // It's still spendable after that
    let reward = t.reward(&child, REWARD);
    let (_, next) = child.claim_rewards(&mut t.ctx, &[reward])?;
    t.confirm(&child, &next)?;

    Ok(())
}

#[test]
fn test_travel_not_matching_revealed_state() -> anyhow::Result<()> {
    let mut t = Test::new();

    let state = t.self_pooling_state();
    let plot_nft = t.launch(state)?;

    // The owner reveals one state, but goes to a puzzle hash that isn't for that state
    let revealed = t.pool_state(POOL_A, 5);
    let extra_data = t.ctx.alloc(&[revealed.key_value()?])?;
    plot_nft.spend_waiting_room(
        &mut t.ctx,
        PoolWaitingRoomSolution::Travel {
            extra_data,
            destination_puzzle_hash: POOL_B,
        },
    )?;
    let solution = t.ctx.take()[0].solution.clone();
    let solution = node_from_bytes(&mut t.ctx, &solution)?;
    assert_eq!(plot_nft.parse_child(&t.ctx, solution)?, None);

    // No state being revealed at all
    let extra_data = t.ctx.alloc(&())?;
    plot_nft.spend_waiting_room(
        &mut t.ctx,
        PoolWaitingRoomSolution::Travel {
            extra_data,
            destination_puzzle_hash: POOL_B,
        },
    )?;
    let solution = t.ctx.take()[0].solution.clone();
    let solution = node_from_bytes(&mut t.ctx, &solution)?;
    assert_eq!(plot_nft.parse_child(&t.ctx, solution)?, None);

    // The same as the first case, but for the pool member puzzle
    let state = t.pool_state(POOL_A, 5);
    let plot_nft = t.launch(state)?;
    let revealed = t.pool_state(POOL_B, 5);
    let extra_data = t.ctx.alloc(&[revealed.key_value()?])?;
    plot_nft.spend_member(&mut t.ctx, PoolMemberSolution::Escape { extra_data })?;
    let solution = t.ctx.take()[0].solution.clone();
    let solution = node_from_bytes(&mut t.ctx, &solution)?;
    assert_eq!(plot_nft.parse_child(&t.ctx, solution)?, None);

    Ok(())
}

#[test]
fn test_parse_launcher_mismatches() -> anyhow::Result<()> {
    let mut ctx = SpendContext::new();
    let owner = BlsPair::new(42).pk;
    let genesis = TESTNET11_CONSTANTS.genesis_challenge;
    let launcher = Launcher::new(Bytes32::new([1; 32]), 1);
    let launcher_coin = launcher.coin();

    let (_, plot_nft) = launcher.mint_plot_nft(
        &mut ctx,
        PoolState::self_pooling(SELF_TARGET, owner),
        DELAY,
        DELAYED,
        genesis,
    )?;
    let solution = ctx.take().remove(0).solution;
    let solution = node_from_bytes(&mut ctx, &solution)?;

    assert!(PlotNft::parse_launcher(&ctx, launcher_coin, solution, genesis)?.is_some());

    // Another network has different puzzles
    assert!(
        PlotNft::parse_launcher(
            &ctx,
            launcher_coin,
            solution,
            MAINNET_CONSTANTS.genesis_challenge
        )?
        .is_none()
    );

    // Not a launcher coin
    assert!(
        PlotNft::parse_launcher(
            &ctx,
            Coin::new(launcher_coin.parent_coin_info, POOL_A, 1),
            solution,
            genesis
        )?
        .is_none()
    );

    let puzzle_hash = plot_nft.coin.puzzle_hash;
    let state = plot_nft.info.pool_state.key_value()?;
    let t = Bytes::from(b"t".to_vec());
    let h = Bytes::from(b"h".to_vec());

    // The key value list has to match the singleton
    let kv = ctx.alloc(&clvm_list!(
        state.clone(),
        (t.clone(), DELAY + 1),
        (h.clone(), DELAYED)
    ))?;
    let solution = ctx.alloc(&clvm_list!(puzzle_hash, 1, kv))?;
    assert!(PlotNft::parse_launcher(&ctx, launcher_coin, solution, genesis)?.is_none());

    // Anything missing from the key value list, such as for another type of singleton
    for kv in [
        ctx.alloc(&())?,
        ctx.alloc(&clvm_list!(state.clone(), (t.clone(), DELAY)))?,
        ctx.alloc(&clvm_list!(state.clone(), (h.clone(), DELAYED)))?,
        ctx.alloc(&clvm_list!((t.clone(), DELAY), (h.clone(), DELAYED)))?,
    ] {
        let solution = ctx.alloc(&clvm_list!(puzzle_hash, 1, kv))?;
        assert!(PlotNft::parse_launcher(&ctx, launcher_coin, solution, genesis)?.is_none());
    }

    // The order doesn't matter, and neither do unrelated entries
    let kv = ctx.alloc(&clvm_list!(
        (h, DELAYED),
        (Bytes::from(b"x".to_vec()), 1),
        (t, DELAY),
        state
    ))?;
    let solution = ctx.alloc(&clvm_list!(puzzle_hash, 1, kv))?;
    assert_eq!(
        PlotNft::parse_launcher(&ctx, launcher_coin, solution, genesis)?,
        Some(plot_nft)
    );

    Ok(())
}

#[test]
fn test_layers_only_parse_the_v1_singleton() -> anyhow::Result<()> {
    let mut ctx = SpendContext::new();
    let owner = BlsPair::new(42).pk;

    let (_, plot_nft) = Launcher::new(Bytes32::new([1; 32]), 1).mint_plot_nft(
        &mut ctx,
        PoolState::farming_to_pool(POOL_A, owner, "https://a.b".to_string(), 5),
        DELAY,
        DELAYED,
        TESTNET11_CONSTANTS.genesis_challenge,
    )?;

    let layer = SingletonV1Layer::new(plot_nft.info.launcher_id, plot_nft.info.member_layer());
    assert_eq!(layer.tree_hash(), plot_nft.coin.puzzle_hash.into());

    let ptr = layer.construct_puzzle(&mut ctx)?;
    let puzzle = Puzzle::parse(&ctx, ptr);

    assert_eq!(
        SingletonV1Layer::<PoolMemberLayer>::parse_puzzle(&ctx, puzzle)?,
        Some(layer)
    );
    assert_eq!(
        SingletonV1Layer::<PoolWaitingRoomLayer>::parse_puzzle(&ctx, puzzle)?,
        None
    );
    assert_eq!(
        SingletonLayer::<PoolMemberLayer>::parse_puzzle(&ctx, puzzle)?,
        None
    );

    // The same inner puzzle in a v1.1 singleton is a different coin
    let v1_1 = SingletonLayer::new(plot_nft.info.launcher_id, plot_nft.info.member_layer());
    assert_ne!(v1_1.tree_hash(), layer.tree_hash());
    let ptr = v1_1.construct_puzzle(&mut ctx)?;
    let puzzle = Puzzle::parse(&ctx, ptr);
    assert_eq!(
        SingletonV1Layer::<PoolMemberLayer>::parse_puzzle(&ctx, puzzle)?,
        None
    );

    Ok(())
}

#[test]
fn test_pool_state_encoding() -> anyhow::Result<()> {
    let owner = BlsPair::new(42).pk;

    let state = PoolState::farming_to_pool(POOL_A, owner, "abc".to_string(), 1000);

    let mut expected = vec![1, 3];
    expected.extend_from_slice(&POOL_A);
    expected.extend_from_slice(&owner.to_bytes());
    expected.extend_from_slice(&[1, 0, 0, 0, 3]);
    expected.extend_from_slice(b"abc");
    expected.extend_from_slice(&1000u32.to_be_bytes());
    assert_eq!(state.to_bytes()?, expected);
    assert_eq!(PoolState::from_bytes(&expected)?, state);

    // None and an empty URL are different on-chain
    let none = PoolState::self_pooling(SELF_TARGET, owner);
    let empty = PoolState {
        pool_url: Some(String::new()),
        ..none.clone()
    };
    assert_eq!(none.to_bytes()?.len(), 87);
    assert_eq!(empty.to_bytes()?.len(), 91);
    assert_eq!(PoolState::from_bytes(&none.to_bytes()?)?, none);
    assert_eq!(PoolState::from_bytes(&empty.to_bytes()?)?, empty);

    // Reading a state must not fail on anything the wallet wouldn't create
    let mut allocator = Allocator::new();
    let unusual = PoolState {
        relative_lock_height: 5000,
        ..PoolState::farming_to_pool(POOL_A, owner, String::new(), 0)
    };
    let list = [unusual.key_value()?].to_clvm(&mut allocator)?;
    assert_eq!(
        PoolState::from_key_value_list(&allocator, list),
        Some(unusual)
    );

    // Unknown states and trailing data aren't states that can be used
    let mut bytes = none.to_bytes()?;
    bytes.push(0);
    assert!(PoolState::from_bytes(&bytes).is_err());
    bytes.pop();
    bytes[1] = 4;
    assert!(PoolState::from_bytes(&bytes).is_err());

    Ok(())
}

/// The real on-chain history of a plot NFT that has been in several pools.
struct MainnetSpend {
    coin: Coin,
    puzzle: Program,
    solution: Program,
}

fn mainnet_lineage() -> anyhow::Result<(Vec<MainnetSpend>, Vec<MainnetSpend>)> {
    let mut lineage = Vec::new();
    let mut rewards = Vec::new();

    for line in include_str!("mainnet_lineage.txt").lines() {
        if line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split(' ').collect();

        let spend = MainnetSpend {
            coin: Coin::new(
                hex::decode(parts[1])?.try_into()?,
                hex::decode(parts[2])?.try_into()?,
                parts[3].parse()?,
            ),
            puzzle: hex::decode(parts[4])?.into(),
            solution: hex::decode(parts[5])?.into(),
        };

        if parts[0] == "r" {
            rewards.push(spend);
        } else {
            lineage.push(spend);
        }
    }

    Ok((lineage, rewards))
}

#[test]
fn test_mainnet_lineage() -> anyhow::Result<()> {
    use PoolSingletonState::{FarmingToPool, LeavingPool, SelfPooling};

    let (lineage, rewards) = mainnet_lineage()?;
    let genesis_challenge = MAINNET_CONSTANTS.genesis_challenge;

    let mut ctx = SpendContext::new();

    let solution = node_from_bytes(&mut ctx, &lineage[0].solution)?;
    let mut plot_nft =
        PlotNft::parse_launcher(&ctx, lineage[0].coin, solution, genesis_challenge)?.unwrap();

    assert_eq!(lineage[0].coin.coin_id(), plot_nft.info.launcher_id);
    assert_eq!(plot_nft.info.seconds_delay, DEFAULT_P2_SINGLETON_DELAY);
    assert_eq!(
        rewards[0].coin.puzzle_hash,
        plot_nft.info.p2_puzzle_hash().into()
    );
    assert_eq!(lineage[0].puzzle.as_ref(), SINGLETON_LAUNCHER);

    let mut states = Vec::new();
    let mut remaining_rewards = rewards.iter();

    for spend in &lineage[1..] {
        assert_eq!(spend.coin, plot_nft.coin);

        let solution = node_from_bytes(&mut ctx, &spend.solution)?;
        let child = plot_nft.parse_child(&ctx, solution)?.unwrap();

        states.push(child.info.pool_state.state);

        // Building the same transition must give the exact bytes that the reference wallet did
        let rebuilt = if child.info == plot_nft.info {
            let reward = remaining_rewards.next().unwrap();
            let (_, rebuilt) = plot_nft.claim_rewards(&mut ctx, &[reward.coin])?;
            rebuilt
        } else if plot_nft.info.pool_state.is_waiting_room() {
            plot_nft.travel(&mut ctx, child.info.pool_state.clone())?
        } else {
            plot_nft.leave_pool(&mut ctx)?
        };

        assert_eq!(rebuilt, child);

        let coin_spends = ctx.take();
        let singleton_spend = &coin_spends[0];

        assert_eq!(singleton_spend.coin, spend.coin);
        assert_eq!(singleton_spend.puzzle_reveal, spend.puzzle);
        assert_eq!(singleton_spend.solution, spend.solution);

        if let Some(reward_spend) = coin_spends.get(1) {
            let reward = rewards
                .iter()
                .find(|reward| reward.coin == reward_spend.coin)
                .unwrap();
            assert_eq!(reward_spend.puzzle_reveal, reward.puzzle);
            assert_eq!(reward_spend.solution, reward.solution);
        }

        plot_nft = child;
    }

    assert!(remaining_rewards.next().is_none());

    assert_eq!(
        states,
        [
            LeavingPool,
            FarmingToPool,
            LeavingPool,
            SelfPooling,
            FarmingToPool,
            LeavingPool,
            FarmingToPool,
            LeavingPool,
            SelfPooling,
            FarmingToPool,
            FarmingToPool,
            LeavingPool,
            SelfPooling,
            SelfPooling,
        ]
    );

    Ok(())
}

#[test]
fn test_mainnet_layers() -> anyhow::Result<()> {
    let (lineage, rewards) = mainnet_lineage()?;

    let mut ctx = SpendContext::new();
    let mut member = 0;
    let mut waiting_room = 0;

    for spend in &lineage[1..] {
        let puzzle = node_from_bytes(&mut ctx, &spend.puzzle)?;
        let puzzle = Puzzle::parse(&ctx, puzzle);
        let solution = node_from_bytes(&mut ctx, &spend.solution)?;

        if let Some(layer) = SingletonV1Layer::<PoolMemberLayer>::parse_puzzle(&ctx, puzzle)? {
            assert_eq!(layer.tree_hash(), spend.coin.puzzle_hash.into());
            SingletonV1Layer::<PoolMemberLayer>::parse_solution(&ctx, solution)?;
            member += 1;
        } else {
            let layer =
                SingletonV1Layer::<PoolWaitingRoomLayer>::parse_puzzle(&ctx, puzzle)?.unwrap();
            assert_eq!(layer.tree_hash(), spend.coin.puzzle_hash.into());
            SingletonV1Layer::<PoolWaitingRoomLayer>::parse_solution(&ctx, solution)?;
            waiting_room += 1;
        }
    }

    assert!(member > 0);
    assert!(waiting_room > 0);

    for reward in &rewards {
        let puzzle = node_from_bytes(&mut ctx, &reward.puzzle)?;
        let puzzle = Puzzle::parse(&ctx, puzzle);
        let layer = P2SingletonOrDelayedLayer::parse_puzzle(&ctx, puzzle)?.unwrap();
        assert_eq!(layer.tree_hash(), reward.coin.puzzle_hash.into());
        assert_eq!(layer.seconds_delay, DEFAULT_P2_SINGLETON_DELAY);

        let solution = node_from_bytes(&mut ctx, &reward.solution)?;
        assert!(matches!(
            P2SingletonOrDelayedLayer::parse_solution(&ctx, solution)?,
            P2SingletonOrDelayedSolution::Claim(_)
        ));
    }

    Ok(())
}
