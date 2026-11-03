use chia_protocol::Bytes32;

use crate::{DriverError, Spend, SpendContext};

use super::{BagChild, legacy};

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
}

impl BagFormat {
    /// The legacy format with the default leaf width, with payments sorted.
    pub const fn legacy() -> Self {
        Self::Legacy {
            leaf_width: None,
            sorted: true,
        }
    }

    /// The maximum number of children of a node in a bag with this many payments.
    pub fn width(&self, payment_count: usize) -> usize {
        match self {
            Self::Legacy { leaf_width, .. } => {
                leaf_width.unwrap_or_else(|| legacy::default_leaf_width(payment_count))
            }
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
        }
    }

    pub(super) fn sort(&self, payments: &mut [BagChild]) {
        match self {
            Self::Legacy { sorted, .. } => {
                if *sorted {
                    legacy::sort(payments);
                }
            }
        }
    }

    pub(super) fn node_puzzle_hash(&self, children: &[BagChild]) -> Bytes32 {
        match self {
            Self::Legacy { .. } => legacy::node_puzzle_hash(children),
        }
    }

    pub(super) fn node_spend(
        &self,
        ctx: &mut SpendContext,
        children: &[BagChild],
    ) -> Result<Spend, DriverError> {
        match self {
            Self::Legacy { .. } => legacy::node_spend(ctx, children),
        }
    }
}

/// How a format groups each level of a bag into the nodes of the level above.
#[derive(Debug, Clone)]
pub(super) enum Grouping {
    /// Consecutive chunks of the width, until a single coin remains.
    Chunks { width: usize },
}

impl Grouping {
    /// Groups the coins of a level into the nodes of the level above, by their index in the level.
    ///
    /// The payments are at depth 0, after they're sorted. Each group is the children of one node, in the order they're
    /// created, and the nodes of the level above are in the order of the groups. Returns [`None`] once the level is
    /// the root.
    pub(super) fn group(&self, _depth: usize, level: &[BagChild]) -> Option<Vec<Vec<usize>>> {
        match self {
            Self::Chunks { width } => legacy::group(level, *width),
        }
    }
}
