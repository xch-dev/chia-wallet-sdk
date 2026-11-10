use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};

/// A bid on an auction. The bidder's puzzle hash receives the NFT if the bid wins, or a full refund
/// if it's outbid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionBid {
    /// The amount of the bid, not including the buyer's premium or royalty that the bidder also
    /// locks up.
    pub amount: u64,
    /// The puzzle hash that receives the NFT or the refund.
    #[clvm(rest)]
    pub puzzle_hash: Bytes32,
}

impl AuctionBid {
    /// Creates a new [`AuctionBid`] of `amount` from `puzzle_hash`.
    pub fn new(amount: u64, puzzle_hash: Bytes32) -> Self {
        Self {
            amount,
            puzzle_hash,
        }
    }
}

/// When an auction ends, and how long its grace period for late bids is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionTimings {
    /// The timestamp (in seconds) after which bids can only be placed during the grace period, and
    /// the auction can be ended.
    pub end_time: u64,
    /// If nonzero, a bid can be placed after the end time as long as it's within this many seconds
    /// of the previous spend, and the auction can't be ended until this many seconds after the last
    /// spend.
    #[clvm(rest)]
    pub grace_period: u64,
}

impl AuctionTimings {
    /// Creates new [`AuctionTimings`] from the end time and grace period, in seconds.
    pub fn new(end_time: u64, grace_period: u64) -> Self {
        Self {
            end_time,
            grace_period,
        }
    }
}

/// A fee which is a percentage (in basis points) of the winning bid, paid to a puzzle hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionBpsPayment {
    /// The fee in basis points of the winning bid, rounded down. A fee that rounds to 0 isn't paid.
    pub bps: u64,
    /// The puzzle hash that receives the fee.
    #[clvm(rest)]
    pub puzzle_hash: Bytes32,
}

impl AuctionBpsPayment {
    /// Creates a new [`AuctionBpsPayment`] of `bps` basis points paid to `puzzle_hash`.
    pub fn new(bps: u64, puzzle_hash: Bytes32) -> Self {
        Self { bps, puzzle_hash }
    }
}

/// Where the winning bid is paid out when the auction ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionPayments {
    /// Paid by the winning bidder on top of their bid.
    pub buyers_premium: AuctionBpsPayment,
    /// Taken out of the winning bid.
    pub commission: AuctionBpsPayment,
    /// Receives the winning bid minus the commission, or the NFT if there are no bids. It also
    /// authorizes cancelling the auction before the first bid.
    #[clvm(rest)]
    pub payout_puzzle_hash: Bytes32,
}

impl AuctionPayments {
    /// Creates new [`AuctionPayments`] from the buyer's premium, commission, and payout puzzle
    /// hash.
    pub fn new(
        buyers_premium: AuctionBpsPayment,
        commission: AuctionBpsPayment,
        payout_puzzle_hash: Bytes32,
    ) -> Self {
        Self {
            buyers_premium,
            commission,
            payout_puzzle_hash,
        }
    }
}

/// The state of the auction's action layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionState {
    /// The amount locked in the reserve coin, which is what the winning bidder locked up.
    pub reserve_amount: u64,
    /// Whether the auction has been ended or cancelled, after which it can't be bid on, ended, or
    /// cancelled again.
    pub ended: bool,
    /// Until the first bid, this has an amount of 0 and the payout puzzle hash.
    #[clvm(rest)]
    pub winning_bid: AuctionBid,
}

impl AuctionState {
    /// The state of a newly launched auction, which has no bids.
    pub fn initial(payout_puzzle_hash: Bytes32) -> Self {
        Self {
            reserve_amount: 0,
            ended: false,
            winning_bid: AuctionBid::new(0, payout_puzzle_hash),
        }
    }
}
