use anyhow::{Context, Result, ensure};
use chia_protocol::Bytes32;
use chia_sdk_driver::{
    BagAsset, BagChild, BagFanOuts, BagFormat, BagNode, BagPayment, DriverError, SecureTheBag,
};
use rstest::rstest;

use crate::harness::{AssetKind, Clawbacks, Distribution, Metrics, boundary_sizes, generate, run};

const SEED: u64 = 1337;

/// Payment `k` of `d`, with amounts that are already sorted largest first.
fn line(k: usize, d: usize) -> BagPayment {
    let mut puzzle_hash = [0; 32];
    puzzle_hash[24..].copy_from_slice(&(k as u64).to_be_bytes());
    BagPayment::new(Bytes32::new(puzzle_hash), (d - k) as u64)
}

fn lines(d: usize) -> Vec<BagPayment> {
    (0..d).map(|k| line(k, d)).collect()
}

fn chip0053(leaf_width: Option<usize>, fan_outs: Option<&[usize]>, sorted: bool) -> BagFormat {
    BagFormat::Chip0053 {
        leaf_width,
        fan_outs: fan_outs.map(|fan_outs| BagFanOuts::new(fan_outs).unwrap()),
        sorted,
    }
}

/// Splits the nodes into levels, starting with the leaves, given the number of nodes at each level.
fn levels<'a>(bag: &'a SecureTheBag, counts: &[usize]) -> Result<Vec<Vec<(Bytes32, &'a BagNode)>>> {
    ensure!(
        bag.nodes().len() == counts.iter().sum::<usize>(),
        "expected {} nodes, found {}",
        counts.iter().sum::<usize>(),
        bag.nodes().len()
    );

    let mut nodes = bag.nodes().iter().map(|(hash, node)| (*hash, node));

    Ok(counts
        .iter()
        .map(|&count| nodes.by_ref().take(count).collect())
        .collect())
}

/// Checks that payment `k` is in leaf `k mod n`, and that node `k` of each level is in node `k mod count` of the
/// level above, with children in the order of the level.
fn check_modulo_grouping(bag: &SecureTheBag, counts: &[usize]) -> Result<()> {
    let levels = levels(bag, counts)?;

    let mut below: Vec<BagChild> = bag
        .payments()
        .iter()
        .map(|payment| BagChild::Payment {
            puzzle_hash: payment.coin_puzzle_hash(),
            payment: *payment,
        })
        .collect();

    for (depth, level) in levels.iter().enumerate() {
        for (index, (_, node)) in level.iter().enumerate() {
            let expected: Vec<BagChild> = below
                .iter()
                .skip(index)
                .step_by(level.len())
                .copied()
                .collect();
            ensure!(
                node.children == expected,
                "node {index} at depth {depth} has the wrong children"
            );
        }

        below = level
            .iter()
            .map(|(puzzle_hash, node)| BagChild::Node {
                puzzle_hash: *puzzle_hash,
                amount: node.amount,
            })
            .collect();
    }

    ensure!(
        below
            == [BagChild::Node {
                puzzle_hash: bag.root_hash(),
                amount: bag.total_amount()
            }],
        "the last level isn't the root"
    );

    Ok(())
}

/// The number of children of each node at a level, in the order of the level.
fn fan_outs(level: &[(Bytes32, &BagNode)]) -> Vec<usize> {
    level.iter().map(|(_, node)| node.children.len()).collect()
}

/// Builds a CHIP example with the given `i` values, checks that the recommended values and sorting give the same
/// bag, and returns the bag.
fn example(d: usize, fan_outs: &[usize], counts: &[usize]) -> Result<SecureTheBag> {
    let payments = lines(d);

    let bag = SecureTheBag::build(
        chip0053(Some(100), Some(fan_outs), false),
        BagAsset::Xch,
        payments.clone(),
    )?;
    ensure!(
        bag.payments() == payments,
        "unsorted payments were reordered"
    );
    check_modulo_grouping(&bag, counts)?;

    // The examples use the recommended values, and the lines are already sorted
    let mut reversed = payments;
    reversed.reverse();
    let recommended = SecureTheBag::build(BagFormat::chip0053(), BagAsset::Xch, reversed)?;
    ensure!(
        recommended.root_hash() == bag.root_hash(),
        "the recommended values don't match the example"
    );

    Ok(bag)
}

/// `d = 5000`, `i1 = 100`, `c = 100` and `n = 50`. The root creates 50 leaves of 100 payments each.
#[test]
fn chip0053_example_1() -> Result<()> {
    let bag = example(5000, &[100], &[50, 1])?;
    let levels = levels(&bag, &[50, 1])?;

    assert_eq!(bag.depth(), 2);
    assert_eq!(fan_outs(&levels[1]), [50]);
    assert_eq!(fan_outs(&levels[0]), [100; 50]);

    // Each leaf has one of the 50 largest payments
    for (index, (_, leaf)) in levels[0].iter().enumerate() {
        assert_eq!(leaf.children[0].amount(), 5000 - index as u64);
    }

    Ok(())
}

/// `d = 10,001`, `i1 = 2`, `i2 = 51`, `c = 100` and `n = 101`. The root creates 2 coins, which create 51 and 50 of the
/// 101 leaves. Two leaves have 100 payments, and the other 99 have 99.
#[test]
fn chip0053_example_2() -> Result<()> {
    let bag = example(10_001, &[2, 51], &[101, 2, 1])?;
    let levels = levels(&bag, &[101, 2, 1])?;

    assert_eq!(bag.depth(), 3);
    assert_eq!(fan_outs(&levels[2]), [2]);
    assert_eq!(fan_outs(&levels[1]), [51, 50]);

    let leaves = fan_outs(&levels[0]);
    assert_eq!(leaves[..2], [100, 100]);
    assert_eq!(leaves[2..], [99; 99]);

    Ok(())
}

/// `d = 100,000`, `i1 = 10`, `i2 = 100`, `c = 100` and `n = 1000`. The root creates 10 coins, which create 100 leaves
/// each, and each leaf has 100 payments.
#[test]
fn chip0053_example_3() -> Result<()> {
    let bag = example(100_000, &[10, 100], &[1000, 10, 1])?;
    let levels = levels(&bag, &[1000, 10, 1])?;

    assert_eq!(bag.depth(), 3);
    assert_eq!(fan_outs(&levels[2]), [10]);
    assert_eq!(fan_outs(&levels[1]), [100; 10]);
    assert_eq!(fan_outs(&levels[0]), [100; 1000]);

    Ok(())
}

/// With three levels, the leaves are grouped by the `i1 * i2` nodes above them.
#[test]
fn chip0053_three_levels() -> Result<()> {
    let format = chip0053(Some(3), Some(&[2, 3, 4]), false);
    let payments = lines(70);

    let bag = SecureTheBag::build(format, BagAsset::Xch, payments)?;
    check_modulo_grouping(&bag, &[24, 6, 2, 1])?;

    let levels = levels(&bag, &[24, 6, 2, 1])?;
    assert_eq!(fan_outs(&levels[3]), [2]);
    assert_eq!(fan_outs(&levels[2]), [3, 3]);
    assert_eq!(fan_outs(&levels[1]), [4; 6]);
    assert_eq!(fan_outs(&levels[0])[..22], [3; 22]);
    assert_eq!(fan_outs(&levels[0])[22..], [2, 2]);

    Ok(())
}

#[test]
fn chip0053_sorts_by_amount_then_puzzle_hash() -> Result<()> {
    let payment = |byte: u8, amount: u64| BagPayment::new(Bytes32::new([byte; 32]), amount);
    let input = vec![
        payment(1, 5),
        payment(4, 10),
        payment(3, 5),
        payment(2, 10),
        payment(5, 1),
    ];

    let bag = SecureTheBag::build(BagFormat::chip0053(), BagAsset::Xch, input.clone())?;
    assert_eq!(
        bag.payments(),
        [
            payment(2, 10),
            payment(4, 10),
            payment(1, 5),
            payment(3, 5),
            payment(5, 1)
        ]
    );

    let unsorted = SecureTheBag::build(chip0053(None, None, false), BagAsset::Xch, input.clone())?;
    assert_eq!(unsorted.payments(), input);
    assert_ne!(unsorted.root_hash(), bag.root_hash());

    Ok(())
}

/// The root is still a node, which creates a leaf for the payment.
#[test]
fn chip0053_single_payment() -> Result<()> {
    let payments = lines(1);
    let bag = SecureTheBag::build(BagFormat::chip0053(), BagAsset::Xch, payments.clone())?;

    assert_eq!(bag.depth(), 2);
    check_modulo_grouping(&bag, &[1, 1])?;

    let path = bag
        .path(payments[0].coin_puzzle_hash())
        .context("missing path")?;
    assert_eq!(path.len(), 2);
    assert_eq!(path[0], bag.root_hash());

    Ok(())
}

/// Nodes are the same as in the legacy format, so a single leaf has the same puzzle hash in both.
#[test]
fn chip0053_nodes_match_legacy() -> Result<()> {
    let payments = lines(10);

    let chip = SecureTheBag::build(chip0053(None, None, false), BagAsset::Xch, payments.clone())?;
    let legacy = SecureTheBag::build(
        BagFormat::Legacy {
            leaf_width: None,
            sorted: false,
        },
        BagAsset::Xch,
        payments,
    )?;

    let leaf = chip.nodes().first().context("missing leaf")?;
    assert_eq!(*leaf.0, legacy.root_hash());
    assert_eq!(leaf.1, &legacy.nodes()[&legacy.root_hash()]);

    Ok(())
}

#[test]
fn chip0053_rejects_levels_that_dont_fit() {
    // 101 leaves don't fit under a root with an i1 of 100
    assert!(matches!(
        SecureTheBag::build(
            chip0053(Some(100), Some(&[100]), true),
            BagAsset::Xch,
            lines(10_001)
        ),
        Err(DriverError::BagFanOutsMismatch {
            payments: 10_001,
            leaf_width: 100
        })
    ));

    // One leaf can't fill two nodes above it
    assert!(matches!(
        SecureTheBag::build(
            chip0053(None, Some(&[2, 51]), true),
            BagAsset::Xch,
            lines(100)
        ),
        Err(DriverError::BagFanOutsMismatch { .. })
    ));

    assert!(matches!(
        SecureTheBag::build(chip0053(Some(0), None, true), BagAsset::Xch, lines(1)),
        Err(DriverError::InvalidBagWidth { width: 0, .. })
    ));
}

#[test]
fn chip0053_width_and_fan_outs() {
    let format = BagFormat::chip0053();
    assert_eq!(format.width(10_001), 100);
    assert_eq!(
        format
            .fan_outs(10_001)
            .map(|fan_outs| fan_outs.iter().collect::<Vec<_>>()),
        Some(vec![2, 51])
    );

    let format = chip0053(Some(3), Some(&[2, 5]), true);
    assert_eq!(format.width(30), 5);

    assert_eq!(BagFormat::legacy().fan_outs(10_001), None);
}

/// The smallest and largest number of payments that fit `c = 3` with the given `i` values.
fn fitting_sizes(fan_outs: &[usize]) -> Vec<usize> {
    let (last, upper) = fan_outs.split_last().unwrap();
    let parents: usize = upper.iter().product();
    let min = 3 * (parents - 1) + 1;
    let max = 3 * parents * last;
    vec![min, min + 1, max - 1, max]
}

#[rstest]
fn chip0053_boundary_sizes(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
    #[values(Clawbacks::None, Clawbacks::Mixed)] clawbacks: Clawbacks,
) -> Result<()> {
    let mut cases = Vec::new();

    // The recommended values with the default leaf width, where 10,000 ± 1 is covered by an ignored test instead
    cases.extend(
        boundary_sizes(100)
            .into_iter()
            .filter(|&size| size <= 101)
            .map(|size| (BagFormat::chip0053(), size)),
    );

    // The recommended values with a leaf width of 3 switch to two levels after 300 payments
    for size in [1, 2, 3, 4, 299, 300, 301] {
        cases.push((chip0053(Some(3), None, true), size));
    }

    // Explicit values with two and three levels
    for fan_outs in [&[2, 3][..], &[2, 2, 3]] {
        for size in fitting_sizes(fan_outs) {
            cases.push((chip0053(Some(3), Some(fan_outs), true), size));
        }
    }

    for (format, size) in cases {
        let payments = generate(size, Distribution::Uniform, clawbacks, SEED);
        run(format, asset, &payments, SEED)
            .with_context(|| format!("{format:?}, {size} payments"))?;
    }

    Ok(())
}

#[rstest]
fn chip0053_distributions(
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
        run(chip0053(Some(3), None, true), asset, &payments, SEED)
            .with_context(|| format!("{size} payments"))?;
        run(BagFormat::chip0053(), asset, &payments, SEED)
            .with_context(|| format!("{size} payments"))?;
    }

    Ok(())
}

/// Unsorted bags keep the order of the input, so the root depends on it.
#[rstest]
fn chip0053_unsorted(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
) -> Result<()> {
    let format = chip0053(Some(3), Some(&[2, 3]), false);

    for size in fitting_sizes(&[2, 3]) {
        let mut payments = generate(size, Distribution::Uniform, Clawbacks::Mixed, SEED);
        run(format, asset, &payments, SEED).with_context(|| format!("{size} payments"))?;

        let bag = SecureTheBag::build(format, BagAsset::Xch, payments.clone())?;
        assert_eq!(bag.payments(), payments);

        payments.reverse();
        let reversed = SecureTheBag::build(format, BagAsset::Xch, payments)?;
        assert_ne!(reversed.root_hash(), bag.root_hash());
    }

    Ok(())
}

#[rstest]
#[ignore = "slow, run in release mode"]
fn chip0053_default_width_squared(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
    #[values(Clawbacks::None, Clawbacks::Mixed)] clawbacks: Clawbacks,
    #[values(9_999, 10_000, 10_001)] size: usize,
) -> Result<()> {
    let payments = generate(size, Distribution::Uniform, clawbacks, SEED);
    run(BagFormat::chip0053(), asset, &payments, SEED)?;
    Ok(())
}

#[rstest]
#[ignore = "slow, run in release mode"]
fn chip0053_large(
    #[values(AssetKind::Xch, AssetKind::Cat, AssetKind::RevocableCat)] asset: AssetKind,
    #[values(100_000, 1_000_000)] size: usize,
) -> Result<()> {
    let payments = generate(size, Distribution::Uniform, Clawbacks::Mixed, SEED);
    let metrics = run(BagFormat::chip0053(), asset, &payments, SEED)?;
    println!("{}\n{}", Metrics::header(), metrics.row());
    Ok(())
}
