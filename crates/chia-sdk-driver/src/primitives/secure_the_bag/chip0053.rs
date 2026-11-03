use std::cmp::Reverse;

use crate::DriverError;

use super::BagChild;

/// The default `c`, which is the most payments in a leaf.
const DEFAULT_LEAF_WIDTH: usize = 100;

/// The fan-out that the CHIP's recommended `i` values aim for.
const RECOMMENDED_FAN_OUT: usize = 100;

const MAX_LEVELS: usize = 10;

/// The `i` values of a CHIP-0053 bag, starting with `i1`.
///
/// `i1` is the number of coins the root creates, and each later value is the number of coins that each coin of the
/// level above creates. The last value is a maximum, since the leaves don't always divide evenly between their
/// parents. The CHIP's examples use them this way, although its definition calls them the maximum number of coins at
/// each level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BagFanOuts {
    // Unused values are always zero, so that the derived traits only depend on the used ones
    values: [u16; MAX_LEVELS],
    len: u8,
}

impl BagFanOuts {
    /// The most intermediate levels, which the recommended values never exceed for any number of payments.
    pub const MAX_LEVELS: usize = MAX_LEVELS;

    /// The largest `i` value that the CHIP allows.
    pub const MAX_FAN_OUT: usize = 300;

    /// Fails unless there are between 1 and [`BagFanOuts::MAX_LEVELS`] values, each between 1 and
    /// [`BagFanOuts::MAX_FAN_OUT`].
    pub fn new(fan_outs: &[usize]) -> Result<Self, DriverError> {
        if fan_outs.is_empty()
            || fan_outs.len() > Self::MAX_LEVELS
            || fan_outs
                .iter()
                .any(|&fan_out| fan_out == 0 || fan_out > Self::MAX_FAN_OUT)
        {
            return Err(DriverError::InvalidBagFanOuts);
        }

        let mut values = [0; Self::MAX_LEVELS];

        for (value, &fan_out) in values.iter_mut().zip(fan_outs) {
            *value = u16::try_from(fan_out).expect("fan-outs are at most 300");
        }

        Ok(Self {
            values,
            len: u8::try_from(fan_outs.len()).expect("there are at most 10 levels"),
        })
    }

    /// The `i` values that the CHIP recommends for a number of payments.
    ///
    /// With up to 10,000 payments, `i1` is 100. Otherwise there are `k` levels, where `k` is the smallest with
    /// `d <= 100^(k + 1)`, and each `i` value is the number of payments under its parent divided by `100^(k - j + 1)`,
    /// rounded up, using exact division. These are the CHIP's formulas for up to 100,000,000 payments, extended to
    /// more levels in the same pattern.
    ///
    /// The formulas assume a leaf width of 100. For other leaf widths, the 100 that stands for the leaf width is
    /// replaced by the leaf width, so that the levels always fit the payments. A leaf width of 0 can't build a bag, and
    /// is treated as 1 here.
    pub fn recommended(payment_count: usize, leaf_width: usize) -> Self {
        let d = payment_count as u128;
        let c = leaf_width.max(1) as u128;
        let fan_out = RECOMMENDED_FAN_OUT as u128;

        if d <= c * fan_out {
            return Self::new(&[RECOMMENDED_FAN_OUT]).expect("valid fan-outs");
        }

        // The capacity of k levels is c * 100^k
        let mut levels = 1;
        let mut capacity = c * fan_out;

        while d > capacity {
            levels += 1;
            capacity *= fan_out;
        }

        let mut fan_outs = Vec::with_capacity(levels);
        let mut parents = 1;

        for _ in 0..levels {
            capacity /= fan_out;
            let value = d.div_ceil(parents * capacity);
            fan_outs.push(usize::try_from(value).expect("each value is at most 100"));
            parents *= value;
        }

        Self::new(&fan_outs).expect("valid fan-outs")
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.values[..usize::from(self.len)]
            .iter()
            .map(|&value| usize::from(value))
    }
}

/// The `c` and `i` values for a number of payments, filling in the defaults.
pub(super) fn levels(
    leaf_width: Option<usize>,
    fan_outs: Option<BagFanOuts>,
    payment_count: usize,
) -> (usize, BagFanOuts) {
    let leaf_width = leaf_width.unwrap_or(DEFAULT_LEAF_WIDTH);
    let fan_outs = fan_outs.unwrap_or_else(|| BagFanOuts::recommended(payment_count, leaf_width));
    (leaf_width, fan_outs)
}

/// Sorts by amount, largest first, and then by the puzzle hash of the coin each payment creates.
pub(super) fn sort(children: &mut [BagChild]) {
    children.sort_by_key(|child| (Reverse(child.amount()), child.puzzle_hash()));
}

/// The number of nodes at each level, starting with the leaves and ending with the root.
///
/// There are `n = ceil(d / c)` leaves, and the levels above them have `i1`, `i1 * i2`, and so on nodes. Fails if a
/// node would have no children, or more leaves than the last `i` value.
pub(super) fn level_counts(
    payment_count: usize,
    leaf_width: usize,
    fan_outs: BagFanOuts,
) -> Result<Vec<usize>, DriverError> {
    if leaf_width == 0 {
        return Err(DriverError::InvalidBagWidth {
            width: 0,
            payments: payment_count,
        });
    }

    let leaves = payment_count.div_ceil(leaf_width);
    let fan_outs = fan_outs.iter().collect::<Vec<_>>();
    let (&last, upper) = fan_outs.split_last().expect("there's at least one level");

    let mut counts = vec![1];
    let mut parents = 1usize;

    for &fan_out in upper {
        parents = parents.saturating_mul(fan_out);
        counts.push(parents);
    }

    if parents > leaves || leaves > parents.saturating_mul(last) {
        return Err(DriverError::BagFanOutsMismatch {
            payments: payment_count,
            leaf_width,
        });
    }

    counts.push(leaves);
    counts.reverse();

    Ok(counts)
}

/// Coin `k` of the level goes to node `k mod count` of the level above, in the order of the level.
pub(super) fn group(level_len: usize, count: usize) -> Vec<Vec<usize>> {
    // Cloning a vector doesn't keep its capacity, so each group is allocated separately
    let mut groups: Vec<Vec<usize>> = (0..count)
        .map(|_| Vec::with_capacity(level_len.div_ceil(count)))
        .collect();

    for index in 0..level_len {
        groups[index % count].push(index);
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recommended(payment_count: usize) -> Vec<usize> {
        BagFanOuts::recommended(payment_count, DEFAULT_LEAF_WIDTH)
            .iter()
            .collect()
    }

    #[test]
    fn test_recommended_fan_outs() {
        // Up to 10,000 payments, i1 is 100
        assert_eq!(recommended(1), [100]);
        assert_eq!(recommended(5000), [100]);
        assert_eq!(recommended(10_000), [100]);

        // i1 = ceil(d / 10,000) and i2 = ceil((d / i1) / 100), as in Examples 2 and 3
        assert_eq!(recommended(10_001), [2, 51]);
        assert_eq!(recommended(100_000), [10, 100]);
        assert_eq!(recommended(1_000_000), [100, 100]);

        // i1 = ceil(d / 1,000,000), i2 = ceil((d / i1) / 10,000) and i3 = ceil(((d / i1) / i2) / 100)
        assert_eq!(recommended(1_000_001), [2, 51, 99]);
        assert_eq!(recommended(55_555_555), [56, 100, 100]);
        assert_eq!(recommended(100_000_000), [100, 100, 100]);

        // The same pattern continues past the CHIP's formulas
        assert_eq!(recommended(100_000_001), [2, 51, 99, 100]);
        assert_eq!(BagFanOuts::recommended(usize::MAX, 1).iter().len(), 10);
    }

    #[test]
    fn test_recommended_fan_outs_fit() {
        let mut sizes = vec![1, 2, 99, 100, 101, 12_345];
        for power in [2, 4, 6, 8, 10, 12] {
            let size = 10usize.pow(power);
            sizes.extend([size - 1, size, size + 1, size * 3 + 7]);
        }

        for leaf_width in [1, 2, 3, 100, 101, 1000] {
            for &size in &sizes {
                let fan_outs = BagFanOuts::recommended(size, leaf_width);
                let counts = level_counts(size, leaf_width, fan_outs).unwrap();
                assert_eq!(counts.len(), fan_outs.iter().len() + 1);
                assert!(fan_outs.iter().all(|fan_out| fan_out <= 100));
            }
        }
    }

    #[test]
    fn test_level_counts() {
        let fan_outs = |values: &[usize]| BagFanOuts::new(values).unwrap();

        assert_eq!(
            level_counts(5000, 100, fan_outs(&[100])).unwrap(),
            vec![50, 1]
        );
        assert_eq!(
            level_counts(10_001, 100, fan_outs(&[2, 51])).unwrap(),
            vec![101, 2, 1]
        );
        assert_eq!(
            level_counts(100_000, 100, fan_outs(&[10, 100])).unwrap(),
            vec![1000, 10, 1]
        );
        assert_eq!(
            level_counts(1_000_001, 100, fan_outs(&[2, 51, 99])).unwrap(),
            vec![10_001, 102, 2, 1]
        );

        // A single payment still has a leaf under the root
        assert_eq!(level_counts(1, 100, fan_outs(&[100])).unwrap(), vec![1, 1]);

        // Too many leaves for the last fan-out
        assert!(matches!(
            level_counts(10_001, 100, fan_outs(&[100])),
            Err(DriverError::BagFanOutsMismatch {
                payments: 10_001,
                leaf_width: 100
            })
        ));
        assert!(level_counts(10_201, 100, fan_outs(&[2, 51])).is_err());

        // A node above the leaves would have no children
        assert!(level_counts(100, 100, fan_outs(&[2, 51])).is_err());

        assert!(matches!(
            level_counts(1, 0, fan_outs(&[100])),
            Err(DriverError::InvalidBagWidth { width: 0, .. })
        ));
    }

    #[test]
    fn test_rejects_invalid_fan_outs() {
        assert!(BagFanOuts::new(&[]).is_err());
        assert!(BagFanOuts::new(&[0]).is_err());
        assert!(BagFanOuts::new(&[301]).is_err());
        assert!(BagFanOuts::new(&[1; 11]).is_err());
        assert!(BagFanOuts::new(&[300; 10]).is_ok());
    }

    #[test]
    fn test_group_by_modulo() {
        assert_eq!(group(7, 3), vec![vec![0, 3, 6], vec![1, 4], vec![2, 5]]);
        assert_eq!(group(2, 1), vec![vec![0, 1]]);
    }
}
