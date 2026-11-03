use chia_protocol::Bytes32;

use crate::{DriverError, Spend, SpendContext};

#[cfg(feature = "experimental-bag-formats")]
use super::optimized;
use super::{BagChild, BagFanOuts, chip0053, legacy};

/// Selects how a bag's payments are ordered and grouped into nodes, and what conditions each node outputs.
///
/// The root hash of a bag depends on the format, so the same format must be used to rebuild a bag.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BagFormat {
    /// The format of bags that Cloud Wallet has already created on chain.
    ///
    /// Payments are sorted by the puzzle hash of the coin they create (unless `sorted` is false), then split into
    /// consecutive chunks of `leaf_width`, and the resulting nodes are chunked the same way level by level until one
    /// remains.
    /// Each node is `(q . conditions)`, starting with a `"$"` coin announcement for fee spends to assert.
    /// A single payment without a clawback isn't bagged at all, so the root is the payment's puzzle hash.
    Legacy {
        /// Overrides the default leaf width, which is 100, 250, 500 or 1000 depending on the number of payments.
        leaf_width: Option<usize>,
        /// Whether payments are sorted. Airdrops are sorted, but NFT bulk mints keep the payments in the order of
        /// the mints, so the root depends on the order of the input.
        sorted: bool,
    },
    /// The format in [CHIP-0053](https://github.com/Chia-Network/chips/pull/183), with a reading for each detail that
    /// the CHIP leaves open.
    ///
    /// In the CHIP's terms, `d` is the number of payments, `c` is `leaf_width`, `i1`, `i2` and so on are `fan_outs`,
    /// and there are `n = ceil(d / c)` leaves.
    ///
    /// - Payment `k`, counting from 0, goes to leaf `k mod n`. Leaf `k` goes to node `k mod (i1 * ... * ij)` of the
    ///   level above it, and so on up to the `i1` nodes that the root creates. The levels under the root therefore
    ///   have `i1`, `i1 * i2`, ... and `n` nodes, and each `i` value is how many children each node of the level above
    ///   has (the last one is a maximum). This reproduces Examples 2 and 3, which the CHIP's definition of `i` as the
    ///   most coins at a level doesn't. With three levels, the leaves are grouped by the number of nodes above them,
    ///   rather than by `i2` as the CHIP's text says.
    /// - Each node creates its children in the order of the level, so payments are created in the order of the input.
    /// - Sorted payments are ordered by amount, largest first, and then by the puzzle hash of the coin each payment
    ///   creates. Unsorted payments keep the order of the input, like the lines of the CHIP's CSV file.
    /// - Each node is `(q . conditions)`, and only creates its children. Each child is hinted to its puzzle hash, or
    ///   for a clawback payment, to the receiver with the clawback memo, as in [`BagFormat::Legacy`]. Unlike
    ///   [`BagFormat::Legacy`], there's no `"$"` coin announcement, since a fee spend can use `ASSERT_CONCURRENT_SPEND`
    ///   on the node's coin ID instead.
    /// - The root is always a node above the `i1` level, even for a single payment.
    /// - Duplicate payments are rejected, like in every format.
    /// - The CHIP's optional URL memo is on the coin that funds the root, so it isn't part of the bag.
    ///
    /// The `i` values default to [`BagFanOuts::recommended`]. Since `c` and the `i` values change the root, a bag that
    /// uses other values can only be rebuilt if they're published along with the payments.
    ///
    /// Building fails if a node would have no children, or more children than `c` or its `i` value.
    Chip0053 {
        /// Overrides `c`, the most payments in a leaf, which is 100 by default.
        leaf_width: Option<usize>,
        /// Overrides the `i` values, which are [`BagFanOuts::recommended`] by default.
        fan_outs: Option<BagFanOuts>,
        /// Whether payments are sorted by amount. Otherwise, the root depends on the order of the input.
        sorted: bool,
    },
    /// An experimental format, only used to compare against [`BagFormat::Chip0053`] before the CHIP is finalized. It
    /// will be folded into [`BagFormat::Chip0053`] or removed, so its roots aren't stable.
    ///
    /// Payments are sorted and spread across each level like [`BagFormat::Chip0053`], and nodes are the same. The
    /// depth is the smallest that fits `max_width` children per node, and every level uses the smallest fan-out that
    /// fits in that depth, so nodes are as cheap to spend as they can be without adding a level. The root creates the
    /// payments directly if there are no more than `max_width` of them.
    #[cfg(feature = "experimental-bag-formats")]
    Optimized {
        /// The most children of a node. This is only a parameter for comparing widths, and the final format will fix
        /// it, so the root only depends on the payments.
        max_width: usize,
    },
}

impl BagFormat {
    /// The legacy format with the default leaf width, with payments sorted.
    pub const fn legacy() -> Self {
        Self::Legacy {
            leaf_width: None,
            sorted: true,
        }
    }

    /// The CHIP-0053 format with the default `c` and `i` values, with payments sorted.
    pub const fn chip0053() -> Self {
        Self::Chip0053 {
            leaf_width: None,
            fan_outs: None,
            sorted: true,
        }
    }

    /// The experimental format with the default maximum width.
    #[cfg(feature = "experimental-bag-formats")]
    pub const fn optimized() -> Self {
        Self::Optimized {
            max_width: optimized::DEFAULT_MAX_WIDTH,
        }
    }

    /// The maximum number of children of a node in a bag with this many payments.
    pub fn width(&self, payment_count: usize) -> usize {
        match self {
            Self::Legacy { leaf_width, .. } => {
                leaf_width.unwrap_or_else(|| legacy::default_leaf_width(payment_count))
            }
            Self::Chip0053 {
                leaf_width,
                fan_outs,
                ..
            } => {
                let (leaf_width, fan_outs) =
                    chip0053::levels(*leaf_width, *fan_outs, payment_count);
                fan_outs.iter().fold(leaf_width, usize::max)
            }
            #[cfg(feature = "experimental-bag-formats")]
            Self::Optimized { max_width } => *max_width,
        }
    }

    /// The `i` values of a CHIP-0053 bag with this many payments, or [`None`] for other formats.
    pub fn fan_outs(&self, payment_count: usize) -> Option<BagFanOuts> {
        match self {
            Self::Legacy { .. } => None,
            #[cfg(feature = "experimental-bag-formats")]
            Self::Optimized { .. } => None,
            Self::Chip0053 {
                leaf_width,
                fan_outs,
                ..
            } => Some(chip0053::levels(*leaf_width, *fan_outs, payment_count).1),
        }
    }

    /// Checks that the format can build a bag with this many payments, and returns how it groups each level.
    pub(super) fn grouping(&self, payment_count: usize) -> Result<Grouping, DriverError> {
        match self {
            Self::Legacy { .. } => {
                let width = self.width(payment_count);

                // A width of one never reduces the level, unless there's only one payment to begin with
                if width == 0 || (width == 1 && payment_count > 1) {
                    return Err(DriverError::InvalidBagWidth {
                        width,
                        payments: payment_count,
                    });
                }

                Ok(Grouping::Chunks { width })
            }
            Self::Chip0053 {
                leaf_width,
                fan_outs,
                ..
            } => {
                let (leaf_width, fan_outs) =
                    chip0053::levels(*leaf_width, *fan_outs, payment_count);

                Ok(Grouping::Modulo {
                    counts: chip0053::level_counts(payment_count, leaf_width, fan_outs)?,
                })
            }
            #[cfg(feature = "experimental-bag-formats")]
            Self::Optimized { max_width } => {
                // A width of one never reduces the level, unless there's only one payment to begin with
                if *max_width == 0 || (*max_width == 1 && payment_count > 1) {
                    return Err(DriverError::InvalidBagWidth {
                        width: *max_width,
                        payments: payment_count,
                    });
                }

                Ok(Grouping::Modulo {
                    counts: optimized::level_counts(payment_count, *max_width),
                })
            }
        }
    }

    pub(super) fn sort(&self, payments: &mut [BagChild]) {
        match self {
            Self::Legacy { sorted, .. } => {
                if *sorted {
                    legacy::sort(payments);
                }
            }
            Self::Chip0053 { sorted, .. } => {
                if *sorted {
                    chip0053::sort(payments);
                }
            }
            #[cfg(feature = "experimental-bag-formats")]
            Self::Optimized { .. } => chip0053::sort(payments),
        }
    }

    pub(super) fn node_puzzle_hash(&self, children: &[BagChild]) -> Bytes32 {
        match self {
            Self::Legacy { .. } => legacy::node_puzzle_hash(children, true),
            Self::Chip0053 { .. } => legacy::node_puzzle_hash(children, false),
            #[cfg(feature = "experimental-bag-formats")]
            Self::Optimized { .. } => legacy::node_puzzle_hash(children, false),
        }
    }

    pub(super) fn node_spend(
        &self,
        ctx: &mut SpendContext,
        children: &[BagChild],
    ) -> Result<Spend, DriverError> {
        match self {
            Self::Legacy { .. } => legacy::node_spend(ctx, children, true),
            Self::Chip0053 { .. } => legacy::node_spend(ctx, children, false),
            #[cfg(feature = "experimental-bag-formats")]
            Self::Optimized { .. } => legacy::node_spend(ctx, children, false),
        }
    }
}

/// How a format groups each level of a bag into the nodes of the level above.
#[derive(Debug, Clone)]
pub(super) enum Grouping {
    /// Consecutive chunks of the width, until a single coin remains.
    Chunks { width: usize },
    /// Coin `k` of a level goes to node `k mod count` of the level above, with the count for each level from the
    /// leaves up to the root.
    Modulo { counts: Vec<usize> },
}

impl Grouping {
    /// Groups the coins of a level into the nodes of the level above, by their index in the level.
    ///
    /// The payments are at depth 0, after they're sorted. Each group is the children of one node, in the order they're
    /// created, and the nodes of the level above are in the order of the groups. Returns [`None`] once the level is
    /// the root.
    pub(super) fn group(&self, depth: usize, level: &[BagChild]) -> Option<Vec<Vec<usize>>> {
        match self {
            Self::Chunks { width } => legacy::group(level, *width),
            Self::Modulo { counts } => counts
                .get(depth)
                .map(|&count| chip0053::group(level.len(), count)),
        }
    }
}
