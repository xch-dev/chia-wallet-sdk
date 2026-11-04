use anyhow::Result;
use chia_sdk_driver::{BagFanOuts, BagFormat};

use crate::harness::{AssetKind, Clawbacks, Distribution, generate, simulate};

const SEED: u64 = 1337;

/// Prints the cost of spending a node with each number of children, with `--ignored --nocapture`.
///
/// Each bag is a root with a single leaf, so the most expensive spend is the leaf.
#[test]
#[ignore = "slow, run in release mode"]
fn node_cost_by_width() -> Result<()> {
    println!("| Asset | Clawbacks | Children | Node cost |");
    println!("|---|---|---:|---:|");

    for asset in [AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat] {
        for clawbacks in [Clawbacks::None, Clawbacks::All] {
            for width in [1, 2, 10, 50, 100, 200, 300, 500, 1000] {
                let format = BagFormat::Chip0053 {
                    leaf_width: Some(width),
                    fan_outs: Some(BagFanOuts::new(&[1])?),
                    sorted: true,
                };
                let payments = generate(width, Distribution::Uniform, clawbacks, SEED);
                let (_, metrics) = simulate(format, asset, &payments)?;
                println!(
                    "| {asset:?} | {clawbacks:?} | {width} | {} |",
                    metrics.max_cost
                );
            }
        }
    }

    Ok(())
}

#[cfg(feature = "experimental-bag-formats")]
mod experimental {
    use anyhow::Context;
    use chia_protocol::Bytes32;
    use chia_sdk_driver::{BagAsset, BagPayment, DriverError, SecureTheBag};
    use rstest::rstest;

    use crate::harness::{Metrics, boundary_sizes, run};

    use super::*;

    fn optimized(leaf_width: usize, upper_fan_out: usize, balanced: bool) -> BagFormat {
        BagFormat::Optimized {
            leaf_width,
            upper_fan_out,
            balanced,
        }
    }

    fn lines(d: usize) -> Vec<BagPayment> {
        (0..d)
            .map(|k| {
                let mut puzzle_hash = [0; 32];
                puzzle_hash[24..].copy_from_slice(&(k as u64).to_be_bytes());
                BagPayment::new(Bytes32::new(puzzle_hash), (d - k) as u64)
            })
            .collect()
    }

    /// The number of children of each node, level by level from the leaves.
    fn fan_outs(bag: &SecureTheBag) -> Vec<usize> {
        bag.nodes()
            .values()
            .map(|node| node.children.len())
            .collect()
    }

    #[test]
    fn optimized_root_creates_small_bags_directly() -> Result<()> {
        let bag = SecureTheBag::build(BagFormat::optimized(), BagAsset::Xch, lines(1))?;
        assert_eq!(bag.depth(), 1);
        assert_eq!(fan_outs(&bag), [1]);

        let bag = SecureTheBag::build(BagFormat::optimized(), BagAsset::Xch, lines(100))?;
        assert_eq!(bag.depth(), 1);
        assert_eq!(fan_outs(&bag), [100]);

        Ok(())
    }

    /// Payment `k` goes to leaf `k mod n`, so each leaf has one of the largest payments.
    #[test]
    fn optimized_spreads_payments() -> Result<()> {
        let payments = lines(101);
        let bag = SecureTheBag::build(BagFormat::optimized(), BagAsset::Xch, payments.clone())?;

        assert_eq!(bag.depth(), 2);
        assert_eq!(fan_outs(&bag), [51, 50, 2]);

        for (index, leaf) in bag.nodes().values().take(2).enumerate() {
            let expected: Vec<_> = payments.iter().skip(index).step_by(2).collect();
            let found: Vec<_> = leaf
                .children
                .iter()
                .map(|child| child.payment().context("not a payment"))
                .collect::<Result<_>>()?;
            assert_eq!(found, expected);
        }

        Ok(())
    }

    /// Balanced levels shrink the nodes above the leaves, without adding a level.
    #[test]
    fn optimized_balances_upper_levels() -> Result<()> {
        let payments = lines(1000);

        let full = SecureTheBag::build(optimized(10, 100, false), BagAsset::Xch, payments.clone())?;
        assert_eq!(full.depth(), 2);
        assert_eq!(fan_outs(&full)[100..], [100]);

        let balanced = SecureTheBag::build(optimized(10, 100, true), BagAsset::Xch, payments)?;
        assert_eq!(balanced.depth(), 2);
        assert_eq!(fan_outs(&balanced)[100..], [100]);

        let deeper = SecureTheBag::build(optimized(10, 50, true), BagAsset::Xch, lines(1000))?;
        assert_eq!(deeper.depth(), 3);
        assert_eq!(fan_outs(&deeper)[100..], [10; 11]);

        Ok(())
    }

    #[test]
    fn optimized_rejects_invalid_width() {
        assert!(matches!(
            SecureTheBag::build(optimized(0, 100, true), BagAsset::Xch, lines(1)),
            Err(DriverError::InvalidBagWidth { width: 0, .. })
        ));
        assert!(matches!(
            SecureTheBag::build(optimized(2, 1, true), BagAsset::Xch, lines(3)),
            Err(DriverError::InvalidBagWidth { width: 1, .. })
        ));
        assert!(SecureTheBag::build(optimized(2, 1, true), BagAsset::Xch, lines(2)).is_ok());
    }

    #[rstest]
    fn optimized_boundary_sizes(
        #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
        #[values(Clawbacks::None, Clawbacks::Mixed)] clawbacks: Clawbacks,
        #[values((3, 3, false), (3, 3, true), (3, 2, true), (100, 100, true))] config: (
            usize,
            usize,
            bool,
        ),
    ) -> Result<()> {
        let (leaf_width, upper_fan_out, balanced) = config;
        let format = optimized(leaf_width, upper_fan_out, balanced);

        for size in boundary_sizes(leaf_width)
            .into_iter()
            .filter(|&size| leaf_width < 100 || size <= leaf_width + 1)
        {
            let payments = generate(size, Distribution::Uniform, clawbacks, SEED);
            run(format, asset, &payments, SEED).with_context(|| format!("{size} payments"))?;
        }

        Ok(())
    }

    #[rstest]
    fn optimized_distributions(
        #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
        #[values(
            Distribution::Uniform,
            Distribution::Equal,
            Distribution::PowerLaw,
            Distribution::MostlyDust,
            Distribution::NearMax
        )]
        distribution: Distribution,
        #[values(Clawbacks::None, Clawbacks::Mixed)] clawbacks: Clawbacks,
    ) -> Result<()> {
        for size in [1, 2, 10, 101] {
            let payments = generate(size, distribution, clawbacks, SEED);
            run(optimized(3, 3, true), asset, &payments, SEED)
                .with_context(|| format!("{size} payments"))?;
            run(BagFormat::optimized(), asset, &payments, SEED)
                .with_context(|| format!("{size} payments"))?;
        }

        Ok(())
    }

    /// Prints a table that compares the leading optimized shapes against the other formats, with
    /// `--ignored --nocapture`.
    #[test]
    #[ignore = "slow, run in release mode"]
    fn optimized_width_report() -> Result<()> {
        let formats = [
            BagFormat::legacy(),
            BagFormat::chip0053(),
            optimized(100, 100, false),
            optimized(100, 100, true),
            optimized(100, 10, false),
            optimized(200, 200, true),
        ];

        let mut rows = Vec::new();

        for (distribution, clawbacks) in [
            (Distribution::Uniform, Clawbacks::None),
            (Distribution::MostlyDust, Clawbacks::Mixed),
        ] {
            rows.push(format!(
                "\n**{distribution:?} amounts, {clawbacks:?} clawbacks**\n"
            ));
            rows.push(Metrics::header());

            for asset in [AssetKind::Xch, AssetKind::RevocableCat] {
                for size in [1, 101, 1001, 10_001, 100_000] {
                    let payments = generate(size, distribution, clawbacks, SEED);

                    for format in formats {
                        rows.push(run(format, asset, &payments, SEED)?.row());
                    }
                }
            }
        }

        println!("{}", rows.join("\n"));

        Ok(())
    }
}

/// Prints the cost of a node with 100 `CREATE_COIN`s, with and without a hint, with `--ignored --nocapture`.
#[test]
#[ignore = "only prints a measurement"]
fn hint_cost() -> Result<()> {
    use chia_protocol::{Bytes32, Coin, CoinSpend};
    use chia_puzzle_types::Memos;
    use chia_sdk_driver::{SpendContext, spend_bundle_cost};
    use chia_sdk_types::{Condition, conditions::CreateCoin};
    use clvm_traits::clvm_quote;
    use clvmr::NodePtr;

    for hinted in [false, true] {
        let mut ctx = SpendContext::new();
        let mut conditions = Vec::new();

        for i in 0..100u8 {
            let puzzle_hash = Bytes32::new([i; 32]);
            let memos = if hinted {
                Memos::Some(ctx.alloc(&[puzzle_hash])?)
            } else {
                Memos::None
            };
            conditions.push(Condition::<NodePtr>::CreateCoin(CreateCoin::new(
                puzzle_hash,
                1_000_000_000,
                memos,
            )));
        }

        let puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        let coin = Coin::new(
            Bytes32::default(),
            ctx.tree_hash(puzzle).into(),
            100_000_000_000,
        );
        let coin_spend =
            CoinSpend::new(coin, ctx.serialize(&puzzle)?, ctx.serialize(&NodePtr::NIL)?);
        println!(
            "hinted: {hinted}, cost of 100 children: {}",
            spend_bundle_cost(&[coin_spend])?
        );
    }

    Ok(())
}
