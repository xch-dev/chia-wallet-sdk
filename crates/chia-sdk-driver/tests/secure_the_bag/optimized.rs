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

    fn optimized(max_width: usize) -> BagFormat {
        BagFormat::Optimized { max_width }
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
        let bag = SecureTheBag::build(optimized(100), BagAsset::Xch, lines(1))?;
        assert_eq!(bag.depth(), 1);
        assert_eq!(fan_outs(&bag), [1]);

        let bag = SecureTheBag::build(optimized(100), BagAsset::Xch, lines(100))?;
        assert_eq!(bag.depth(), 1);
        assert_eq!(fan_outs(&bag), [100]);

        Ok(())
    }

    /// 101 payments need two levels, so the fan-out drops to 11 and the root creates 10 leaves.
    #[test]
    fn optimized_balances_levels() -> Result<()> {
        let payments = lines(101);
        let bag = SecureTheBag::build(optimized(100), BagAsset::Xch, payments.clone())?;

        assert_eq!(bag.depth(), 2);
        let fan_outs = fan_outs(&bag);
        assert_eq!(fan_outs[..10], [11, 10, 10, 10, 10, 10, 10, 10, 10, 10]);
        assert_eq!(fan_outs[10..], [10]);

        // Leaf k has payment k, so each leaf has one of the 10 largest payments
        for (index, leaf) in bag.nodes().values().take(10).enumerate() {
            let expected: Vec<_> = payments.iter().skip(index).step_by(10).collect();
            let found: Vec<_> = leaf
                .children
                .iter()
                .map(|child| child.payment().context("not a payment"))
                .collect::<Result<_>>()?;
            assert_eq!(found, expected);
        }

        Ok(())
    }

    #[test]
    fn optimized_rejects_invalid_width() {
        assert!(matches!(
            SecureTheBag::build(optimized(0), BagAsset::Xch, lines(1)),
            Err(DriverError::InvalidBagWidth { width: 0, .. })
        ));
        assert!(matches!(
            SecureTheBag::build(optimized(1), BagAsset::Xch, lines(2)),
            Err(DriverError::InvalidBagWidth { width: 1, .. })
        ));
        assert!(SecureTheBag::build(optimized(1), BagAsset::Xch, lines(1)).is_ok());
    }

    #[rstest]
    fn optimized_boundary_sizes(
        #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
        #[values(Clawbacks::None, Clawbacks::Mixed)] clawbacks: Clawbacks,
        #[values(3, 100)] max_width: usize,
    ) -> Result<()> {
        for size in boundary_sizes(max_width)
            .into_iter()
            .filter(|&size| max_width < 100 || size <= max_width + 1)
        {
            let payments = generate(size, Distribution::Uniform, clawbacks, SEED);
            run(optimized(max_width), asset, &payments, SEED)
                .with_context(|| format!("{size} payments"))?;
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
            run(optimized(3), asset, &payments, SEED)
                .with_context(|| format!("{size} payments"))?;
            run(BagFormat::optimized(), asset, &payments, SEED)
                .with_context(|| format!("{size} payments"))?;
        }

        Ok(())
    }

    /// Prints a table that compares widths of the optimized format against the other formats, with
    /// `--ignored --nocapture`.
    #[test]
    #[ignore = "slow, run in release mode"]
    fn optimized_width_report() -> Result<()> {
        let mut formats = vec![BagFormat::legacy(), BagFormat::chip0053()];
        formats.extend([20, 40, 100, 300].map(optimized));

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
                for size in [101, 1001, 10_001, 100_000] {
                    let payments = generate(size, distribution, clawbacks, SEED);

                    for &format in &formats {
                        rows.push(run(format, asset, &payments, SEED)?.row());
                    }
                }
            }
        }

        println!("{}", rows.join("\n"));

        Ok(())
    }
}
