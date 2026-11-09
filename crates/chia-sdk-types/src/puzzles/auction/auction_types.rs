use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};

/// A bid on an auction. The bidder's puzzle hash receives the NFT if the bid wins, or a full refund
/// if it's outbid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AuctionBid {
    pub amount: u64,
    #[clvm(rest)]
    pub puzzle_hash: Bytes32,
}

impl AuctionBid {
    pub fn new(amount: u64, puzzle_hash: Bytes32) -> Self {
        Self {
            amount,
            puzzle_hash,
        }
    }
}

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
    pub bps: u64,
    #[clvm(rest)]
    pub puzzle_hash: Bytes32,
}

impl AuctionBpsPayment {
    pub fn new(bps: u64, puzzle_hash: Bytes32) -> Self {
        Self { bps, puzzle_hash }
    }
}

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
