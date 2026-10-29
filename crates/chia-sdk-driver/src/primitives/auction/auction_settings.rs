use std::num::NonZeroU64;

use chia_sdk_types::{
    Mod,
    puzzles::{
        AuctionFlatBidVerifierArgs, AuctionPayments, AuctionPercentBidVerifierArgs, AuctionTimings,
    },
};
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, ToClvm, ToClvmError};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

/// The settings chosen by the seller when launching an auction, which can't be changed afterward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionSettings {
    pub bid_verifier: AuctionBidVerifier,
    pub timings: AuctionTimings,
    pub payments: AuctionPayments,
}

impl AuctionSettings {
    pub fn new(
        bid_verifier: AuctionBidVerifier,
        timings: AuctionTimings,
        payments: AuctionPayments,
    ) -> Self {
        Self {
            bid_verifier,
            timings,
            payments,
        }
    }
}

/// Determines which bids are accepted, in addition to every bid being strictly higher than the
/// previous one.
///
/// This is represented in CLVM as the list `(kind minimum_bid bid_increment)`, where the kind is `0`
/// for [`Flat`](Self::Flat) and `1` for [`Percent`](Self::Percent). The minimum bid and increment
/// can't be zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuctionBidVerifier {
    /// Each bid must be at least `bid_increment` more than the previous one.
    Flat {
        minimum_bid: NonZeroU64,
        bid_increment: NonZeroU64,
    },
    /// Each bid must be at least `bid_increment_bps` basis points (rounded down) more than the
    /// previous one.
    Percent {
        minimum_bid: NonZeroU64,
        bid_increment_bps: NonZeroU64,
    },
}

impl AuctionBidVerifier {
    pub fn minimum_bid(&self) -> NonZeroU64 {
        match *self {
            Self::Flat { minimum_bid, .. } | Self::Percent { minimum_bid, .. } => minimum_bid,
        }
    }

    pub fn curry_tree_hash(&self) -> TreeHash {
        match *self {
            Self::Flat {
                minimum_bid,
                bid_increment,
            } => AuctionFlatBidVerifierArgs::new(minimum_bid.get(), bid_increment.get())
                .curry_tree_hash(),
            Self::Percent {
                minimum_bid,
                bid_increment_bps,
            } => AuctionPercentBidVerifierArgs::new(minimum_bid.get(), bid_increment_bps.get())
                .curry_tree_hash(),
        }
    }

    pub fn get_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        match *self {
            Self::Flat {
                minimum_bid,
                bid_increment,
            } => ctx.curry(AuctionFlatBidVerifierArgs::new(
                minimum_bid.get(),
                bid_increment.get(),
            )),
            Self::Percent {
                minimum_bid,
                bid_increment_bps,
            } => ctx.curry(AuctionPercentBidVerifierArgs::new(
                minimum_bid.get(),
                bid_increment_bps.get(),
            )),
        }
    }
}

#[derive(ToClvm, FromClvm)]
#[clvm(list)]
#[repr(u8)]
enum RawAuctionBidVerifier {
    Flat {
        minimum_bid: u64,
        bid_increment: u64,
    } = 0,
    Percent {
        minimum_bid: u64,
        bid_increment_bps: u64,
    } = 1,
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for AuctionBidVerifier {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let raw = match *self {
            Self::Flat {
                minimum_bid,
                bid_increment,
            } => RawAuctionBidVerifier::Flat {
                minimum_bid: minimum_bid.get(),
                bid_increment: bid_increment.get(),
            },
            Self::Percent {
                minimum_bid,
                bid_increment_bps,
            } => RawAuctionBidVerifier::Percent {
                minimum_bid: minimum_bid.get(),
                bid_increment_bps: bid_increment_bps.get(),
            },
        };

        raw.to_clvm(encoder)
    }
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for AuctionBidVerifier {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let non_zero = |value| {
            NonZeroU64::new(value).ok_or_else(|| {
                FromClvmError::Custom(
                    "the auction minimum bid and bid increment must be non-zero".to_string(),
                )
            })
        };

        Ok(match RawAuctionBidVerifier::from_clvm(decoder, node)? {
            RawAuctionBidVerifier::Flat {
                minimum_bid,
                bid_increment,
            } => Self::Flat {
                minimum_bid: non_zero(minimum_bid)?,
                bid_increment: non_zero(bid_increment)?,
            },
            RawAuctionBidVerifier::Percent {
                minimum_bid,
                bid_increment_bps,
            } => Self::Percent {
                minimum_bid: non_zero(minimum_bid)?,
                bid_increment_bps: non_zero(bid_increment_bps)?,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use clvm_traits::clvm_list;
    use clvmr::Allocator;

    use super::*;

    #[test]
    fn test_bid_verifier_clvm() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();

        for bid_verifier in [
            AuctionBidVerifier::Flat {
                minimum_bid: NonZeroU64::new(100).unwrap(),
                bid_increment: NonZeroU64::MAX,
            },
            AuctionBidVerifier::Percent {
                minimum_bid: NonZeroU64::MIN,
                bid_increment_bps: NonZeroU64::new(500).unwrap(),
            },
        ] {
            let ptr = bid_verifier.to_clvm(&mut allocator)?;
            assert_eq!(
                AuctionBidVerifier::from_clvm(&allocator, ptr)?,
                bid_verifier
            );
        }

        let ptr = clvm_list!(1, 2, 3).to_clvm(&mut allocator)?;
        assert_eq!(
            AuctionBidVerifier::from_clvm(&allocator, ptr)?,
            AuctionBidVerifier::Percent {
                minimum_bid: NonZeroU64::new(2).unwrap(),
                bid_increment_bps: NonZeroU64::new(3).unwrap(),
            }
        );

        for (kind, minimum_bid, bid_increment) in [(0, 0, 1), (0, 1, 0), (1, 0, 1), (1, 1, 0)] {
            let ptr = clvm_list!(kind, minimum_bid, bid_increment).to_clvm(&mut allocator)?;
            assert!(AuctionBidVerifier::from_clvm(&allocator, ptr).is_err());
        }

        Ok(())
    }
}
