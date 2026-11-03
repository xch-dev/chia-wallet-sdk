//! An experimental format, used to compare against CHIP-0053 before it's finalized.

/// The default leaf width and upper fan-out.
pub(super) const DEFAULT_WIDTH: usize = 100;

/// The number of nodes at each level, starting with the leaves and ending with the root.
///
/// There are `ceil(d / leaf_width)` leaves, unless there are no more payments than `leaf_width`, in which case the root
/// creates the payments directly. Above the leaves, a full level has `ceil(previous / upper_fan_out)` nodes, and a
/// balanced bag uses the fewest levels with at most `upper_fan_out` children, with the smallest fan-out that fits.
pub(super) fn level_counts(
    payment_count: usize,
    leaf_width: usize,
    upper_fan_out: usize,
    balanced: bool,
) -> Vec<usize> {
    let leaves = payment_count.div_ceil(leaf_width);

    if leaves == 1 {
        return vec![1];
    }

    let fan_out = if balanced {
        smallest_fan_out(leaves, upper_fan_out)
    } else {
        upper_fan_out
    };

    let mut counts = vec![leaves];
    let mut count = leaves;

    while count > 1 {
        count = count.div_ceil(fan_out);
        counts.push(count);
    }

    counts
}

/// The smallest fan-out that fits `count` nodes under the root in the fewest levels with at most `max_fan_out`
/// children each.
fn smallest_fan_out(count: usize, max_fan_out: usize) -> usize {
    let mut levels = 1;
    let mut capacity = max_fan_out;

    while capacity < count {
        levels += 1;
        capacity = capacity.saturating_mul(max_fan_out);
    }

    if levels == 1 {
        return count;
    }

    (2..=max_fan_out)
        .find(|fan_out| {
            fan_out
                .checked_pow(levels)
                .is_none_or(|capacity| capacity >= count)
        })
        .unwrap_or(max_fan_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_counts() {
        assert_eq!(level_counts(1, 100, 100, false), [1]);
        assert_eq!(level_counts(100, 100, 100, false), [1]);
        assert_eq!(level_counts(101, 100, 100, false), [2, 1]);
        assert_eq!(level_counts(10_001, 100, 100, false), [101, 2, 1]);
        assert_eq!(level_counts(100_000, 100, 100, false), [1000, 10, 1]);
        assert_eq!(
            level_counts(1_000_001, 100, 100, false),
            [10_001, 101, 2, 1]
        );

        // Balanced levels shrink the nodes above the leaves without adding a level
        assert_eq!(level_counts(10_001, 100, 100, true), [101, 10, 1]);
        assert_eq!(level_counts(100_000, 100, 100, true), [1000, 32, 1]);
        assert_eq!(level_counts(1_000_000, 100, 100, true), [10_000, 100, 1]);

        // A smaller upper fan-out adds levels
        assert_eq!(level_counts(100_000, 100, 10, false), [1000, 100, 10, 1]);
    }

    #[test]
    fn test_fan_outs_fit() {
        for (leaf_width, upper_fan_out) in [(2, 2), (3, 3), (100, 10), (100, 100), (200, 200)] {
            for balanced in [false, true] {
                for payment_count in [1, 2, 3, 99, 100, 101, 9999, 10_000, 10_001, 1_000_003] {
                    let counts = level_counts(payment_count, leaf_width, upper_fan_out, balanced);
                    let mut below = payment_count;

                    for (depth, &count) in counts.iter().enumerate() {
                        let width = if depth == 0 {
                            leaf_width
                        } else {
                            upper_fan_out
                        };
                        assert!(below.div_ceil(count) <= width);
                        assert!(count <= below);
                        below = count;
                    }

                    assert_eq!(below, 1);
                }
            }
        }
    }
}
