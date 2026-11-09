use anyhow::{Context, Result};
use chia_sdk_driver::{BagAsset, BagFormat, SecureTheBag};
use rstest::rstest;

use crate::harness::{AssetKind, Clawbacks, Distribution, Metrics, boundary_sizes, generate, run};

const SEED: u64 = 1337;

#[rstest]
fn legacy_boundary_sizes(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
    #[values(Clawbacks::None, Clawbacks::Mixed)] clawbacks: Clawbacks,
    #[values(None, Some(3))] leaf_width: Option<usize>,
) -> Result<()> {
    let format = BagFormat::Legacy {
        leaf_width,
        sorted: true,
    };

    // The default leaf width for up to 1000 payments
    let width = leaf_width.unwrap_or(100);

    // Width² ± 1 at the default width is slow in debug builds, so it's covered by an ignored test instead
    for size in boundary_sizes(width)
        .into_iter()
        .filter(|&size| leaf_width.is_some() || size <= width + 1)
    {
        let payments = generate(size, Distribution::Uniform, clawbacks, SEED);
        run(format, asset, &payments, SEED).with_context(|| format!("{size} payments"))?;
    }

    Ok(())
}

/// NFT bulk mints bag a one mojo pre-launcher coin per mint, in the order of the mints.
#[rstest]
fn legacy_unsorted_mints(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
    #[values(None, Some(3))] leaf_width: Option<usize>,
) -> Result<()> {
    let format = BagFormat::Legacy {
        leaf_width,
        sorted: false,
    };
    let width = leaf_width.unwrap_or(100);

    for size in boundary_sizes(width)
        .into_iter()
        .filter(|&size| leaf_width.is_some() || size <= width + 1)
    {
        let mut payments = generate(size, Distribution::Equal, Clawbacks::None, SEED);
        for payment in &mut payments {
            payment.amount = 1;
        }

        run(format, asset, &payments, SEED).with_context(|| format!("{size} payments"))?;

        let bag = SecureTheBag::build(format, BagAsset::Xch, payments.clone())?;
        assert_eq!(bag.payments(), payments);

        if size > 1 {
            payments.reverse();
            let reversed = SecureTheBag::build(format, BagAsset::Xch, payments)?;
            assert_ne!(reversed.root_hash(), bag.root_hash());
        }
    }

    Ok(())
}

#[rstest]
fn legacy_default_width_tiers(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
) -> Result<()> {
    for size in [1000, 1001] {
        let payments = generate(size, Distribution::Uniform, Clawbacks::Mixed, SEED);
        run(BagFormat::legacy(), asset, &payments, SEED)
            .with_context(|| format!("{size} payments"))?;
    }

    Ok(())
}

#[rstest]
fn legacy_distributions(
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
        let format = BagFormat::Legacy {
            leaf_width: Some(3),
            sorted: true,
        };
        run(format, asset, &payments, SEED).with_context(|| format!("{size} payments"))?;
        run(BagFormat::legacy(), asset, &payments, SEED)
            .with_context(|| format!("{size} payments"))?;
    }

    Ok(())
}

#[rstest]
#[ignore = "slow, run in release mode"]
fn legacy_default_width_squared(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
    #[values(Clawbacks::None, Clawbacks::Mixed)] clawbacks: Clawbacks,
    #[values(9_999, 10_000, 10_001)] size: usize,
) -> Result<()> {
    let payments = generate(size, Distribution::Uniform, clawbacks, SEED);
    run(BagFormat::legacy(), asset, &payments, SEED)?;
    Ok(())
}

#[rstest]
#[ignore = "slow, run in release mode"]
fn legacy_large(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
    #[values(100_000, 1_000_000)] size: usize,
) -> Result<()> {
    let payments = generate(size, Distribution::Uniform, Clawbacks::Mixed, SEED);
    let metrics = run(BagFormat::legacy(), asset, &payments, SEED)?;
    println!("{}\n{}", Metrics::header(), metrics.row());
    Ok(())
}

/// Prints a table that compares the formats, with `--ignored --nocapture`.
#[test]
#[ignore = "slow, run in release mode"]
fn comparison_report() -> Result<()> {
    let formats = [BagFormat::legacy()];
    let assets = [AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat];
    let sizes = [1, 2, 99, 101, 1000, 1001, 10_001, 100_000];

    let mut rows = Vec::new();

    for distribution in [
        Distribution::Uniform,
        Distribution::PowerLaw,
        Distribution::MostlyDust,
    ] {
        for clawbacks in [Clawbacks::None, Clawbacks::Mixed] {
            rows.push(format!(
                "\n**{distribution:?} amounts, {clawbacks:?} clawbacks**\n"
            ));
            rows.push(Metrics::header());

            for format in formats {
                for asset in assets {
                    for size in sizes {
                        let payments = generate(size, distribution, clawbacks, SEED);
                        rows.push(run(format, asset, &payments, SEED)?.row());
                    }
                }
            }
        }
    }

    println!("{}", rows.join("\n"));

    Ok(())
}
