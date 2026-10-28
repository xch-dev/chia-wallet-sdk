use std::num::NonZeroU64;

use bindy::{Error, Result};
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{
    self as sdk, AuctionBidVerifier, AuctionInfo, AuctionLaunch, AuctionReserve, AuctionSettings,
    Cat, Launcher, RoyaltyInfo, SingletonInfo,
};
use chia_sdk_types::puzzles::{AuctionBid, AuctionState};
use clvm_utils::TreeHash;

use crate::{AsProgram, AsPtr, CatSpend, Clvm, Nft, Program, Proof, Spend, conditions_to_programs};

pub fn auction_lock_p2_puzzle_hash(launcher_id: Bytes32) -> Result<Bytes32> {
    Ok(sdk::auction_lock_p2_puzzle_hash(launcher_id))
}

pub fn calculate_auction_payment(amount: u64, bps: u64) -> Result<Option<u64>> {
    Ok(sdk::calculate_auction_payment(amount, bps))
}

pub trait AuctionStateExt: Sized {
    fn initial(payout_puzzle_hash: Bytes32) -> Result<Self>;
}

impl AuctionStateExt for AuctionState {
    fn initial(payout_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(Self::initial(payout_puzzle_hash))
    }
}

pub trait AuctionBidVerifierExt: Sized {
    fn flat(minimum_bid: u64, bid_increment: u64) -> Result<Self>;
    fn percent(minimum_bid: u64, bid_increment_bps: u64) -> Result<Self>;
    fn is_flat(&self) -> Result<bool>;
    fn minimum_bid(&self) -> Result<u64>;
    fn bid_increment(&self) -> Result<Option<u64>>;
    fn bid_increment_bps(&self) -> Result<Option<u64>>;
}

impl AuctionBidVerifierExt for AuctionBidVerifier {
    fn flat(minimum_bid: u64, bid_increment: u64) -> Result<Self> {
        Ok(Self::Flat {
            minimum_bid: non_zero(minimum_bid, "minimum_bid")?,
            bid_increment: non_zero(bid_increment, "bid_increment")?,
        })
    }

    fn percent(minimum_bid: u64, bid_increment_bps: u64) -> Result<Self> {
        Ok(Self::Percent {
            minimum_bid: non_zero(minimum_bid, "minimum_bid")?,
            bid_increment_bps: non_zero(bid_increment_bps, "bid_increment_bps")?,
        })
    }

    fn is_flat(&self) -> Result<bool> {
        Ok(matches!(self, Self::Flat { .. }))
    }

    fn minimum_bid(&self) -> Result<u64> {
        Ok(Self::minimum_bid(self).get())
    }

    fn bid_increment(&self) -> Result<Option<u64>> {
        match *self {
            Self::Flat { bid_increment, .. } => Ok(Some(bid_increment.get())),
            Self::Percent { .. } => Ok(None),
        }
    }

    fn bid_increment_bps(&self) -> Result<Option<u64>> {
        match *self {
            Self::Flat { .. } => Ok(None),
            Self::Percent {
                bid_increment_bps, ..
            } => Ok(Some(bid_increment_bps.get())),
        }
    }
}

pub trait AuctionReserveExt: Sized {
    fn xch(coin: Coin) -> Result<Self>;
    fn cat(cat: Cat) -> Result<Self>;
    fn coin(&self) -> Result<Coin>;
    fn as_cat(&self) -> Result<Option<Cat>>;
    fn p2_puzzle_hash(&self) -> Result<Bytes32>;
    fn with_parent_coin_info(&self, parent_coin_info: Bytes32) -> Result<Self>;
    fn child(&self, amount: u64) -> Result<Self>;
    fn settlement_puzzle_hash(&self) -> Result<Bytes32>;
}

impl AuctionReserveExt for AuctionReserve {
    fn xch(coin: Coin) -> Result<Self> {
        Ok(Self::Xch(coin))
    }

    fn cat(cat: Cat) -> Result<Self> {
        Ok(Self::Cat(cat))
    }

    fn coin(&self) -> Result<Coin> {
        Ok(Self::coin(self))
    }

    fn as_cat(&self) -> Result<Option<Cat>> {
        match *self {
            Self::Xch(_) => Ok(None),
            Self::Cat(cat) => Ok(Some(cat)),
        }
    }

    fn p2_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(Self::p2_puzzle_hash(self))
    }

    fn with_parent_coin_info(&self, parent_coin_info: Bytes32) -> Result<Self> {
        Ok(Self::with_parent_coin_info(self, parent_coin_info))
    }

    fn child(&self, amount: u64) -> Result<Self> {
        Ok(Self::child(self, amount))
    }

    fn settlement_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(Self::settlement_puzzle_hash(self))
    }
}

pub trait AuctionInfoExt {
    fn royalty_bps(&self) -> Result<u64>;
    fn reserve_amount_for_bid(&self, bid_amount: u64) -> Result<Option<u64>>;
    fn bid_action_hash(&self) -> Result<TreeHash>;
    fn end_action_hash(&self) -> Result<TreeHash>;
    fn cancel_action_hash(&self) -> Result<TreeHash>;
    fn nft_unlocker_hash(&self) -> Result<TreeHash>;
    fn inner_puzzle_hash(&self) -> Result<TreeHash>;
    fn puzzle_hash(&self) -> Result<TreeHash>;
}

impl AuctionInfoExt for AuctionInfo {
    fn royalty_bps(&self) -> Result<u64> {
        Ok(Self::royalty_bps(self))
    }

    fn reserve_amount_for_bid(&self, bid_amount: u64) -> Result<Option<u64>> {
        Ok(Self::reserve_amount_for_bid(self, bid_amount))
    }

    fn bid_action_hash(&self) -> Result<TreeHash> {
        Ok(Self::bid_action_hash(self))
    }

    fn end_action_hash(&self) -> Result<TreeHash> {
        Ok(Self::end_action_hash(self))
    }

    fn cancel_action_hash(&self) -> Result<TreeHash> {
        Ok(Self::cancel_action_hash(self))
    }

    fn nft_unlocker_hash(&self) -> Result<TreeHash> {
        Ok(Self::nft_unlocker_hash(self))
    }

    fn inner_puzzle_hash(&self) -> Result<TreeHash> {
        Ok(SingletonInfo::inner_puzzle_hash(self))
    }

    fn puzzle_hash(&self) -> Result<TreeHash> {
        Ok(SingletonInfo::puzzle_hash(self))
    }
}

#[derive(Clone)]
pub struct Auction {
    pub coin: Coin,
    pub proof: Proof,
    pub info: AuctionInfo,
}

impl Auction {
    fn to_sdk(&self) -> sdk::Auction {
        sdk::Auction::new(self.coin, self.proof.clone().into(), self.info)
    }

    /// The action spend that places `bid`. The bidder must fund the reserve with
    /// `AuctionInfo.reserveAmountForBid`, typically by spending coins in the same spend bundle
    /// which assert the coin announcement of the bid's tree hash from the auction coin.
    pub fn bid_action_spend(&self, clvm: Clvm, bid: AuctionBid, grace: bool) -> Result<Spend> {
        let mut ctx = clvm.0.lock().unwrap();
        let spend = self.to_sdk().bid_action_spend(&mut ctx, bid, grace)?;
        Ok(spend.as_program(&clvm.0))
    }

    /// The coin announcement that the auction coin creates when `bid` is placed, which bidders
    /// should assert in the spends that fund the reserve.
    pub fn bid_announcement_id(&self, bid: AuctionBid) -> Result<Bytes32> {
        Ok(self.to_sdk().bid_announcement_id(bid))
    }

    /// The action spend that ends the auction. The locked NFT must be unlocked with `unlockNft` in
    /// the same spend bundle.
    pub fn end_action_spend(&self, clvm: Clvm, nft: Nft) -> Result<Spend> {
        let mut ctx = clvm.0.lock().unwrap();
        let nft = nft.as_ptr(&ctx);
        let spend = self.to_sdk().end_action_spend(&mut ctx, &nft)?;
        Ok(spend.as_program(&clvm.0))
    }

    /// The action spend that cancels the auction before the first bid. The locked NFT must be
    /// unlocked with `unlockNft` in the same spend bundle, and a coin with the payout puzzle hash
    /// must output the conditions from `cancelAuthorization`.
    pub fn cancel_action_spend(&self, clvm: Clvm, nft: Nft) -> Result<Spend> {
        let mut ctx = clvm.0.lock().unwrap();
        let nft = nft.as_ptr(&ctx);
        let spend = self.to_sdk().cancel_action_spend(&mut ctx, &nft)?;
        Ok(spend.as_program(&clvm.0))
    }

    /// The conditions that a coin with the auction's payout puzzle hash must output to authorize
    /// cancelling this auction coin.
    pub fn cancel_authorization(&self, clvm: Clvm) -> Result<Vec<Program>> {
        let mut ctx = clvm.0.lock().unwrap();
        let conditions = self.to_sdk().cancel_authorization(&mut ctx)?;
        conditions_to_programs(&mut ctx, &clvm.0, conditions)
    }

    /// Spends the locked NFT to the winning bidder, or back to the payout puzzle hash if there are
    /// no bids. This must be in the same spend bundle as the end or cancel action.
    pub fn unlock_nft(&self, clvm: Clvm, nft: Nft) -> Result<Nft> {
        let mut ctx = clvm.0.lock().unwrap();
        let nft = nft.as_ptr(&ctx);
        Ok(self
            .to_sdk()
            .unlock_nft(&mut ctx, &nft)?
            .as_program(&clvm.0))
    }

    /// Runs the action spends to determine the next state and the reserve coin's conditions.
    pub fn child_state(&self, clvm: Clvm, action_spends: Vec<Spend>) -> Result<AuctionChildState> {
        let mut ctx = clvm.0.lock().unwrap();
        let action_spends = action_spends
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let child_state = self.to_sdk().child_state(&mut ctx, &action_spends)?;

        Ok(AuctionChildState {
            state: child_state.state,
            reserve_conditions: child_state
                .reserve_conditions
                .into_iter()
                .map(|condition| Program(clvm.0.clone(), condition))
                .collect(),
            royalty_amount: child_state.royalty_amount,
        })
    }

    /// Spends the auction with the given action spends, along with the reserve coin and the
    /// settlement coin that pays the NFT royalty. Returns the child auction. For CAT reserves,
    /// `otherCatSpends` are spent in the same CAT ring as the reserve.
    pub fn spend(
        &self,
        clvm: Clvm,
        action_spends: Vec<Spend>,
        other_cat_spends: Vec<CatSpend>,
    ) -> Result<Auction> {
        let mut ctx = clvm.0.lock().unwrap();

        Ok(self
            .to_sdk()
            .spend(
                &mut ctx,
                action_spends.into_iter().map(Into::into).collect(),
                other_cat_spends.into_iter().map(Into::into).collect(),
            )?
            .into())
    }

    /// Parses the child of this auction, given the solution that this auction coin was spent with
    /// on the blockchain. The solution must come from a confirmed spend.
    pub fn parse_child(&self, clvm: Clvm, solution: Program) -> Result<Auction> {
        let mut ctx = clvm.0.lock().unwrap();
        Ok(self.to_sdk().parse_child(&mut ctx, solution.1)?.into())
    }
}

impl From<sdk::Auction> for Auction {
    fn from(value: sdk::Auction) -> Self {
        Self {
            coin: value.coin,
            proof: value.proof.into(),
            info: value.info,
        }
    }
}

#[derive(Clone)]
pub struct AuctionChildState {
    pub state: AuctionState,
    pub reserve_conditions: Vec<Program>,
    pub royalty_amount: Option<u64>,
}

#[derive(Clone)]
pub struct LaunchedAuction {
    pub auction: Auction,
    pub parent_conditions: Vec<Program>,
}

pub trait AuctionLaunchExt: Sized {
    fn parse(launcher_coin: Coin, launcher_solution: Program) -> Result<Option<Self>>;
    fn launcher_coin(&self) -> Result<Coin>;
    fn settings(&self) -> Result<AuctionSettings>;
    fn nft_coin_id(&self) -> Result<Bytes32>;
    fn singleton_puzzle_hash(&self) -> Result<Bytes32>;
    fn singleton_amount(&self) -> Result<u64>;
    #[allow(clippy::wrong_self_convention)]
    fn into_auction(&self, nft_royalty: RoyaltyInfo) -> Result<Auction>;
}

impl AuctionLaunchExt for AuctionLaunch {
    fn parse(launcher_coin: Coin, launcher_solution: Program) -> Result<Option<Self>> {
        let ctx = launcher_solution.0.lock().unwrap();
        Ok(Self::parse(&ctx, launcher_coin, launcher_solution.1))
    }

    fn launcher_coin(&self) -> Result<Coin> {
        Ok(self.launcher_coin)
    }

    fn settings(&self) -> Result<AuctionSettings> {
        Ok(self.memo.settings)
    }

    fn nft_coin_id(&self) -> Result<Bytes32> {
        Ok(Self::nft_coin_id(self))
    }

    fn singleton_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.singleton_puzzle_hash)
    }

    fn singleton_amount(&self) -> Result<u64> {
        Ok(self.singleton_amount)
    }

    fn into_auction(&self, nft_royalty: RoyaltyInfo) -> Result<Auction> {
        Ok(Self::into_auction(*self, nft_royalty)?.into())
    }
}

impl Clvm {
    /// Launches an auction for `nft`, which must already be locked by the auction lock puzzle hash
    /// of the launcher id. The launcher coin is created by `parentCoinId` with an amount of 1, and
    /// the parent must output the returned conditions.
    pub fn launch_auction(
        &self,
        parent_coin_id: Bytes32,
        settings: AuctionSettings,
        reserve: AuctionReserve,
        nft: Nft,
    ) -> Result<LaunchedAuction> {
        let mut ctx = self.0.lock().unwrap();
        let nft = nft.as_ptr(&ctx);

        let (conditions, auction) = Launcher::new(parent_coin_id, 1)
            .with_singleton_amount(sdk::AUCTION_SINGLETON_AMOUNT)
            .launch_auction(&mut ctx, settings, reserve, &nft)?;

        Ok(LaunchedAuction {
            auction: auction.into(),
            parent_conditions: conditions_to_programs(&mut ctx, &self.0, conditions)?,
        })
    }
}

fn non_zero(value: u64, name: &str) -> Result<NonZeroU64> {
    NonZeroU64::new(value).ok_or_else(|| Error::Custom(format!("{name} must be non-zero")))
}
