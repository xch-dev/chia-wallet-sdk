//! An experimental format, used to compare against CHIP-0053 before it's finalized.

/// The most children of a node by default.
pub(super) const DEFAULT_MAX_WIDTH: usize = 100;

/// The smallest number of levels whose nodes, with up to `max_width` children each, hold every payment.
fn depth(payment_count: usize, max_width: usize) -> usize {
    let mut depth = 1;
    let mut capacity = max_width;

    while capacity < payment_count {
        depth += 1;
        capacity = capacity.saturating_mul(max_width);
    }

    depth
}

/// The smallest fan-out that holds every payment in this many levels.
fn fan_out(payment_count: usize, depth: usize) -> usize {
    if depth == 1 {
        return payment_count;
    }

    let depth = u32::try_from(depth).expect("the depth is small");
    let mut fan_out = 1usize;

    while fan_out
        .checked_pow(depth)
        .is_some_and(|capacity| capacity < payment_count)
    {
        fan_out += 1;
    }

    fan_out
}

/// The number of nodes at each level, starting with the leaves and ending with the root.
///
/// The depth is the smallest that fits `max_width`, and every level uses the smallest fan-out `f` that fits in that
/// depth, so level `t` has `ceil(d / f^(t + 1))` nodes and each node has `f` children or one fewer. The payments are
/// spread across the nodes of each level with the same modulo as CHIP-0053, so node `k` of every level holds the
/// `k`-th largest payment.
pub(super) fn level_counts(payment_count: usize, max_width: usize) -> Vec<usize> {
    let depth = depth(payment_count, max_width);
    let fan_out = fan_out(payment_count, depth);

    let mut counts = Vec::with_capacity(depth);
    let mut count = payment_count;

    for _ in 0..depth {
        count = count.div_ceil(fan_out);
        counts.push(count);
    }

    debug_assert_eq!(counts.last(), Some(&1));

    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_counts() {
        assert_eq!(level_counts(1, 100), [1]);
        assert_eq!(level_counts(100, 100), [1]);
        assert_eq!(level_counts(101, 100), [10, 1]);
        assert_eq!(level_counts(10_000, 100), [100, 1]);
        assert_eq!(level_counts(10_001, 100), [455, 21, 1]);
        assert_eq!(level_counts(1_000_000, 100), [10_000, 100, 1]);
        assert_eq!(level_counts(1_000_000, 30), [62_500, 3907, 245, 16, 1]);
    }

    #[test]
    fn test_fan_out_fits() {
        for max_width in [2, 3, 30, 100] {
            for payment_count in [1, 2, 3, 99, 100, 101, 9999, 10_000, 10_001, 1_000_003] {
                let counts = level_counts(payment_count, max_width);
                let mut below = payment_count;

                for &count in &counts {
                    assert!(below.div_ceil(count) <= max_width);
                    below = count;
                }

                assert_eq!(below, 1);
            }
        }
    }
}
