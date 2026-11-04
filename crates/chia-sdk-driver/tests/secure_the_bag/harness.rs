//! A stress harness that any [`BagFormat`] can reuse.
//!
//! It generates payments, checks the structural invariants of the bag, unrolls it in the [`Simulator`] one node spend
//! at a time, and measures the numbers used to compare formats.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, ensure};
use chia_bls::Signature;
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_puzzle_types::cat::GenesisByCoinIdTailArgs;
use chia_sdk_driver::{
    BagAsset, BagChild, BagCoin, BagFormat, BagPayment, Cat, ClawbackV2, SecureTheBag,
    SpendContext, StandardLayer, spend_bundle_cost,
};
use chia_sdk_test::{Simulator, SimulatorConfig};
use chia_sdk_types::Conditions;
use rand::{RngExt, SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;

/// The most that a single node spend may cost, which is half of the maximum block cost.
pub const NODE_COST_BUDGET: u64 = 5_500_000_000;

/// A node is unlikely to be spent by a recipient if none of the payments it leads to are worth more than this, in
/// mojos of the asset. This is roughly the fee for a large node spend at 5 mojos per cost.
pub const FEE_THRESHOLD: u64 = 100_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Xch,
    Cat,
    RevocableCat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution {
    /// Uniformly random amounts up to a trillion mojos.
    Uniform,
    /// Every payment has the same amount.
    Equal,
    /// A few large payments and a long tail of small ones.
    PowerLaw,
    /// Almost every payment is a single mojo.
    MostlyDust,
    /// Amounts that add up to exactly `u64::MAX`.
    NearMax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clawbacks {
    None,
    /// Every other payment has a clawback, starting with the first.
    Mixed,
}

/// The sizes around the edges of a format's width: 1, 2, width ± 1 and width² ± 1.
pub fn boundary_sizes(width: usize) -> Vec<usize> {
    let squared = width * width;
    let mut sizes = vec![
        1,
        2,
        width.saturating_sub(1),
        width,
        width + 1,
        squared.saturating_sub(1),
        squared,
        squared + 1,
    ];
    sizes.retain(|&size| size > 0);
    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

pub fn generate(
    count: usize,
    distribution: Distribution,
    clawbacks: Clawbacks,
    seed: u64,
) -> Vec<BagPayment> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let count_u64 = count as u64;

    (0..count)
        .map(|index| {
            let index_u64 = index as u64;
            let puzzle_hash = Bytes32::new(rng.random());

            let amount = match distribution {
                Distribution::Uniform => rng.random_range(1..=1_000_000_000_000),
                Distribution::Equal => 1_000_000,
                Distribution::PowerLaw => 1_000_000_000_000 / (index_u64 + 1),
                Distribution::MostlyDust => {
                    if rng.random_range(0..20) == 0 {
                        rng.random_range(1..=1_000_000_000_000)
                    } else {
                        1
                    }
                }
                Distribution::NearMax => {
                    u64::MAX / count_u64 + if index == 0 { u64::MAX % count_u64 } else { 0 }
                }
            };

            if clawbacks == Clawbacks::Mixed && index % 2 == 0 {
                BagPayment::from_clawback(ClawbackV2::new(
                    Bytes32::new(rng.random()),
                    puzzle_hash,
                    rng.random_range(1..=u64::from(u32::MAX)),
                    amount,
                    index % 4 == 0,
                ))
            } else {
                BagPayment::new(puzzle_hash, amount)
            }
        })
        .collect()
}

/// The numbers used to compare bag formats.
#[derive(Debug, Clone)]
pub struct Metrics {
    pub format: BagFormat,
    pub asset: AssetKind,
    pub payments: usize,
    pub spends: usize,
    pub depth: usize,
    pub max_fan_out: usize,
    pub total_cost: u64,
    pub max_cost: u64,
    /// The share of nodes whose largest downstream payment is below [`FEE_THRESHOLD`].
    pub fee_incentive: f64,
}

impl Metrics {
    pub fn header() -> String {
        [
            "| Format | Asset | Payments | Spends | Depth | Max fan-out | Total cost | Max node cost | Nodes below fee threshold |",
            "|---|---|---:|---:|---:|---:|---:|---:|---:|",
        ]
        .join("\n")
    }

    pub fn row(&self) -> String {
        format!(
            "| {:?} | {:?} | {} | {} | {} | {} | {} | {} | {:.1}% |",
            self.format,
            self.asset,
            self.payments,
            self.spends,
            self.depth,
            self.max_fan_out,
            self.total_cost,
            self.max_cost,
            self.fee_incentive * 100.0
        )
    }
}

/// Whether the root of the format depends only on the set of payments, and not their order.
pub fn is_canonical(format: BagFormat) -> bool {
    matches!(
        format,
        BagFormat::Legacy { sorted: true, .. } | BagFormat::Chip0053 { sorted: true, .. }
    )
}

/// Checks every invariant, including a full unroll in the simulator, and returns the metrics.
pub fn run(
    format: BagFormat,
    asset: AssetKind,
    payments: &[BagPayment],
    seed: u64,
) -> Result<Metrics> {
    let (bag, metrics) = simulate(format, asset, payments)?;

    check_structure(&bag, payments)?;

    if is_canonical(format) {
        check_order_invariance(&bag, payments, seed)?;
    }

    Ok(metrics)
}

/// Checks that each payment appears exactly once, that amounts add up at every node, and that fan-out stays in bounds.
pub fn check_structure(bag: &SecureTheBag, payments: &[BagPayment]) -> Result<()> {
    let total = payments
        .iter()
        .map(|payment| u128::from(payment.amount))
        .sum::<u128>();
    ensure!(
        u128::from(bag.total_amount()) == total,
        "total amount mismatch"
    );

    let mut expected: HashMap<Bytes32, BagPayment> = payments
        .iter()
        .map(|payment| (payment.coin_puzzle_hash(), *payment))
        .collect();
    ensure!(
        expected.len() == payments.len(),
        "generated duplicate payments"
    );

    let width = bag.format().width(payments.len());

    let Some(root) = bag.node(bag.root_hash()) else {
        // Only a single payment can be paid out without any nodes
        ensure!(
            bag.nodes().is_empty() && bag.depth() == 0,
            "missing root node"
        );
        let payment = expected
            .remove(&bag.root_hash())
            .context("root is neither a node nor a payment")?;
        ensure!(expected.is_empty(), "payments missing from the bag");
        ensure!(payment.amount == bag.total_amount(), "root amount mismatch");
        return Ok(());
    };

    ensure!(root.amount == bag.total_amount(), "root amount mismatch");
    ensure!(bag.parent(bag.root_hash()).is_none(), "root has a parent");

    let mut visited = HashSet::new();
    let mut stack = vec![(bag.root_hash(), 1)];
    let mut max_depth = 0;

    while let Some((puzzle_hash, depth)) = stack.pop() {
        ensure!(
            visited.insert(puzzle_hash),
            "node {puzzle_hash} is reachable twice"
        );
        max_depth = max_depth.max(depth);

        let node = bag.node(puzzle_hash).context("missing node")?;
        ensure!(
            !node.children.is_empty() && node.children.len() <= width,
            "node has {} children, but the width is {width}",
            node.children.len()
        );

        let sum = node
            .children
            .iter()
            .map(|child| u128::from(child.amount()))
            .sum::<u128>();
        ensure!(sum == u128::from(node.amount), "node amount mismatch");

        for child in &node.children {
            ensure!(
                bag.parent(child.puzzle_hash()) == Some(puzzle_hash),
                "child has the wrong parent"
            );

            match child {
                BagChild::Node { puzzle_hash, .. } => stack.push((*puzzle_hash, depth + 1)),
                BagChild::Payment {
                    puzzle_hash,
                    payment,
                } => {
                    let expected = expected
                        .remove(puzzle_hash)
                        .context("payment is unexpected or appears twice")?;
                    ensure!(expected == *payment, "payment mismatch");
                }
            }
        }
    }

    ensure!(expected.is_empty(), "payments missing from the bag");
    ensure!(
        visited.len() == bag.nodes().len(),
        "unreachable nodes in the bag"
    );
    ensure!(max_depth == bag.depth(), "depth mismatch");

    for payment in payments.iter().take(100) {
        let path = bag
            .path(payment.coin_puzzle_hash())
            .context("missing path to payment")?;
        ensure!(
            path.first() == Some(&bag.root_hash()),
            "path doesn't start at the root"
        );
        ensure!(
            path.last().copied() == bag.parent(payment.coin_puzzle_hash()),
            "path doesn't end at the payment's parent"
        );
    }

    Ok(())
}

/// Checks that the root is the same when the payments are reversed or shuffled.
pub fn check_order_invariance(
    bag: &SecureTheBag,
    payments: &[BagPayment],
    seed: u64,
) -> Result<()> {
    let mut reversed = payments.to_vec();
    reversed.reverse();

    let mut shuffled = payments.to_vec();
    shuffled.shuffle(&mut ChaCha8Rng::seed_from_u64(seed));

    for reordered in [reversed, shuffled] {
        let rebuilt = SecureTheBag::build(bag.format(), bag.asset(), reordered)?;
        ensure!(
            rebuilt.root_hash() == bag.root_hash(),
            "root depends on input order"
        );
    }

    Ok(())
}

/// Funds the bag, then spends each node in its own transaction and checks that every payment coin is created with the
/// right hint.
pub fn simulate(
    format: BagFormat,
    asset: AssetKind,
    payments: &[BagPayment],
) -> Result<(SecureTheBag, Metrics)> {
    let mut sim = Simulator::with_config(SimulatorConfig {
        save_spends: false,
        ..SimulatorConfig::default()
    });

    let total = payments
        .iter()
        .try_fold(0u64, |total, payment| total.checked_add(payment.amount))
        .context("total amount overflow")?;

    let alice = sim.bls(total);

    let asset_id = GenesisByCoinIdTailArgs::curry_tree_hash(alice.coin.coin_id()).into();
    let bag_asset = match asset {
        AssetKind::Xch => BagAsset::Xch,
        AssetKind::Cat => BagAsset::Cat { asset_id },
        AssetKind::RevocableCat => BagAsset::RevocableCat {
            asset_id,
            hidden_puzzle_hash: alice.puzzle_hash,
        },
    };

    let bag = SecureTheBag::build(format, bag_asset, payments.iter().copied())?;

    let mut ctx = SpendContext::new();
    let hint = ctx.hint(bag.root_hash())?;
    let create_root = Conditions::new().create_coin(bag.root_hash(), total, hint);

    let root = if asset == AssetKind::Xch {
        StandardLayer::new(alice.pk).spend(&mut ctx, alice.coin, create_root)?;
        BagCoin::Xch(Coin::new(alice.coin.coin_id(), bag.root_hash(), total))
    } else {
        let hidden_puzzle_hash = (asset == AssetKind::RevocableCat).then_some(alice.puzzle_hash);
        let (issue, cats) = Cat::single_issuance(
            &mut ctx,
            alice.coin.coin_id(),
            hidden_puzzle_hash,
            total,
            create_root,
        )?;
        StandardLayer::new(alice.pk).spend(&mut ctx, alice.coin, issue)?;
        BagCoin::Cat(cats[0])
    };

    ensure!(
        root.p2_puzzle_hash() == bag.root_hash(),
        "root coin mismatch"
    );
    ensure!(
        root.coin().puzzle_hash == bag.root_puzzle_hash(),
        "root puzzle hash mismatch"
    );

    sim.spend_coins(ctx.take(), &[alice.sk])?;

    let width = format.width(payments.len());
    let mut costs = Vec::with_capacity(bag.nodes().len());
    let mut max_fan_out = 0;
    let mut payment_coins = Vec::with_capacity(payments.len());
    let mut stack = vec![root];

    while let Some(coin) = stack.pop() {
        let Some(node) = bag.node(coin.p2_puzzle_hash()) else {
            payment_coins.push(coin);
            continue;
        };

        ensure!(node.children.len() <= width, "fan-out exceeds the width");
        max_fan_out = max_fan_out.max(node.children.len());

        let mut ctx = SpendContext::new();
        let children = bag.spend_node(&mut ctx, coin)?;
        let coin_spends = ctx.take();

        let cost = spend_bundle_cost(&coin_spends)?;
        ensure!(cost <= NODE_COST_BUDGET, "node spend costs {cost}");
        costs.push(cost);

        sim.new_transaction(SpendBundle::new(coin_spends, Signature::default()))?;
        stack.extend(children);
    }

    let mut expected: HashMap<Bytes32, BagPayment> = payments
        .iter()
        .map(|payment| (payment.coin_puzzle_hash(), *payment))
        .collect();

    for coin in payment_coins {
        let payment = expected
            .remove(&coin.p2_puzzle_hash())
            .context("unexpected payment coin")?;
        let coin_id = coin.coin().coin_id();

        ensure!(
            coin.coin().amount == payment.amount,
            "payment amount mismatch"
        );
        ensure!(
            coin.coin().puzzle_hash == bag_asset.puzzle_hash(payment.coin_puzzle_hash()),
            "payment puzzle hash mismatch"
        );
        ensure!(
            sim.coin_state(coin_id)
                .is_some_and(|state| state.spent_height.is_none()),
            "payment coin wasn't created"
        );
        ensure!(
            sim.hinted_coins(payment.puzzle_hash).contains(&coin_id),
            "payment coin isn't hinted to the recipient"
        );
    }

    ensure!(expected.is_empty(), "payment coins missing after unroll");

    let metrics = Metrics {
        format,
        asset,
        payments: payments.len(),
        spends: costs.len(),
        depth: bag.depth(),
        max_fan_out,
        total_cost: costs.iter().sum(),
        max_cost: costs.iter().copied().max().unwrap_or(0),
        fee_incentive: fee_incentive(&bag, FEE_THRESHOLD),
    };

    Ok((bag, metrics))
}

/// The share of nodes whose largest downstream payment is below the threshold.
pub fn fee_incentive(bag: &SecureTheBag, threshold: u64) -> f64 {
    fn largest_payment(
        bag: &SecureTheBag,
        puzzle_hash: Bytes32,
        below: &mut usize,
        threshold: u64,
    ) -> u64 {
        let Some(node) = bag.node(puzzle_hash) else {
            return 0;
        };

        let largest = node
            .children
            .iter()
            .map(|child| match child {
                BagChild::Node { puzzle_hash, .. } => {
                    largest_payment(bag, *puzzle_hash, below, threshold)
                }
                BagChild::Payment { payment, .. } => payment.amount,
            })
            .max()
            .unwrap_or(0);

        if largest < threshold {
            *below += 1;
        }

        largest
    }

    if bag.nodes().is_empty() {
        return 0.0;
    }

    let mut below = 0;
    largest_payment(bag, bag.root_hash(), &mut below, threshold);

    #[allow(clippy::cast_precision_loss)]
    let share = below as f64 / bag.nodes().len() as f64;
    share
}
