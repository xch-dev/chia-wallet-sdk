use std::collections::HashSet;

use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::LineageProof;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    MerkleTree, Mod,
    puzzles::{
        ActionLayerArgs, AuctionBidActionArgs, AuctionCancelActionArgs, AuctionEndActionArgs,
        AuctionNftUnlockerArgs, AuctionState,
        RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
        ReserveFinalizer2ndCurryArgs,
    },
};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::NodePtr;

use crate::{
    AuctionMemo, AuctionReserve, AuctionReserveMemo, AuctionSettings, Cat, CatInfo, DriverError,
    RoyaltyInfo, SingletonInfo, SpendContext, auction_lock_p2_puzzle_hash,
};

/// The maximum basis points of each fee, which is 100%.
pub const AUCTION_MAX_BPS: u64 = 10_000;

/// The reserve finalizer always recreates the auction singleton with an amount of 1, so auctions
/// must be launched with it too.
pub const AUCTION_SINGLETON_AMOUNT: u64 = 1;

/// Calculates a fee of `bps` basis points of `amount`, rounded down to match the puzzles. Returns
/// [`None`] if the result doesn't fit in a [`u64`], which can only happen if `bps` is greater than
/// [`AUCTION_MAX_BPS`].
pub fn calculate_auction_payment(amount: u64, bps: u64) -> Option<u64> {
    u64::try_from(u128::from(amount) * u128::from(bps) / u128::from(AUCTION_MAX_BPS)).ok()
}

/// Contains all of the information needed to construct the inner puzzle of an auction.
///
/// An auction is a singleton built on the action layer, which locks up an NFT until it ends and
/// allows bids to be placed on it in XCH or a CAT. Each bid locks up the bid amount (plus the
/// buyer's premium and NFT royalty) in the reserve coin and refunds the previous bidder. When the
/// auction ends, the NFT is sent to the winning bidder and the winning bid is paid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuctionInfo {
    pub launcher_id: Bytes32,
    pub settings: AuctionSettings,
    /// The coin id of the locked NFT, which is sent to the winning bidder when the auction ends.
    pub nft_coin_id: Bytes32,
    /// The royalty info of the locked NFT, which isn't stored in the launcher memo, so must be
    /// fetched from the NFT on the blockchain.
    pub nft_royalty: RoyaltyInfo,
    pub state: AuctionState,
    pub reserve: AuctionReserve,
}

impl AuctionInfo {
    pub fn new(
        launcher_id: Bytes32,
        settings: AuctionSettings,
        nft_coin_id: Bytes32,
        nft_royalty: RoyaltyInfo,
        state: AuctionState,
        reserve: AuctionReserve,
    ) -> Self {
        Self {
            launcher_id,
            settings,
            nft_coin_id,
            nft_royalty,
            state,
            reserve,
        }
    }

    /// Reconstructs a newly launched auction from its launcher memo, the royalty info of the NFT at
    /// the memo's NFT coin id, and the lineage proof of the reserve. The lineage proof is only used
    /// for CAT reserves, and comes from the spend of the reserve's parent.
    pub fn from_memo(
        launcher_id: Bytes32,
        memo: AuctionMemo,
        nft_royalty: RoyaltyInfo,
        reserve_lineage_proof: Option<LineageProof>,
    ) -> Self {
        let lock_puzzle_hash = auction_lock_p2_puzzle_hash(launcher_id);

        let reserve = match memo.reserve {
            AuctionReserveMemo::Xch { parent_coin_info } => {
                AuctionReserve::Xch(Coin::new(parent_coin_info, lock_puzzle_hash, 0))
            }
            AuctionReserveMemo::Cat {
                parent_coin_info,
                asset_id,
                hidden_puzzle_hash,
            } => {
                let info = CatInfo::new(asset_id, hidden_puzzle_hash, lock_puzzle_hash);
                let coin = Coin::new(parent_coin_info, info.puzzle_hash().into(), 0);
                AuctionReserve::Cat(Cat::new(coin, reserve_lineage_proof, info))
            }
        };

        Self::new(
            launcher_id,
            memo.settings,
            memo.nft_coin_id,
            nft_royalty,
            AuctionState::initial(memo.settings.payments.payout_puzzle_hash),
            reserve,
        )
    }

    /// The launcher memo for this auction, which is only meaningful before its first spend.
    pub fn memo(&self) -> AuctionMemo {
        let reserve = match self.reserve {
            AuctionReserve::Xch(coin) => AuctionReserveMemo::Xch {
                parent_coin_info: coin.parent_coin_info,
            },
            AuctionReserve::Cat(cat) => AuctionReserveMemo::Cat {
                parent_coin_info: cat.coin.parent_coin_info,
                asset_id: cat.info.asset_id,
                hidden_puzzle_hash: cat.info.hidden_puzzle_hash,
            },
        };

        AuctionMemo::new(self.settings, self.nft_coin_id, reserve)
    }

    pub fn royalty_bps(&self) -> u64 {
        self.nft_royalty.basis_points.into()
    }

    /// The total amount a bidder must lock up for a bid of `bid_amount`, which is the bid plus the
    /// buyer's premium and NFT royalty. It's refunded in full if the bid is outbid. Returns [`None`]
    /// if the amount doesn't fit in a [`u64`].
    pub fn reserve_amount_for_bid(&self, bid_amount: u64) -> Option<u64> {
        let premium =
            calculate_auction_payment(bid_amount, self.settings.payments.buyers_premium.bps)?;
        let royalty = calculate_auction_payment(bid_amount, self.royalty_bps())?;
        bid_amount.checked_add(premium)?.checked_add(royalty)
    }

    fn bid_action_args<V>(&self, bid_verifier: V) -> AuctionBidActionArgs<V> {
        AuctionBidActionArgs::new(
            bid_verifier,
            self.settings.timings,
            self.settings.payments.buyers_premium.bps,
            self.royalty_bps(),
        )
    }

    pub fn bid_action_hash(&self) -> TreeHash {
        self.bid_action_args(self.settings.bid_verifier.curry_tree_hash())
            .curry_tree_hash()
    }

    pub fn construct_bid_action(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let bid_verifier = self.settings.bid_verifier.get_puzzle(ctx)?;
        ctx.curry(self.bid_action_args(bid_verifier))
    }

    fn nft_unlocker_args(&self) -> AuctionNftUnlockerArgs {
        AuctionNftUnlockerArgs::new(self.reserve.settlement_puzzle_hash(), self.royalty_bps())
    }

    pub fn nft_unlocker_hash(&self) -> TreeHash {
        self.nft_unlocker_args().curry_tree_hash()
    }

    pub fn construct_nft_unlocker(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        ctx.curry(self.nft_unlocker_args())
    }

    fn end_action_args<U>(&self, unlocker: U) -> AuctionEndActionArgs<U> {
        AuctionEndActionArgs::new(
            unlocker,
            self.settings.timings,
            self.settings.payments,
            self.royalty_bps(),
            SETTLEMENT_PAYMENT_HASH.into(),
            self.nft_coin_id,
        )
    }

    pub fn end_action_hash(&self) -> TreeHash {
        self.end_action_args(self.nft_unlocker_hash())
            .curry_tree_hash()
    }

    pub fn construct_end_action(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let unlocker = self.construct_nft_unlocker(ctx)?;
        ctx.curry(self.end_action_args(unlocker))
    }

    fn cancel_action_args<U>(&self, unlocker: U) -> AuctionCancelActionArgs<U> {
        AuctionCancelActionArgs::new(
            unlocker,
            self.settings.payments.payout_puzzle_hash,
            self.nft_coin_id,
        )
    }

    pub fn cancel_action_hash(&self) -> TreeHash {
        self.cancel_action_args(self.nft_unlocker_hash())
            .curry_tree_hash()
    }

    pub fn construct_cancel_action(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let unlocker = self.construct_nft_unlocker(ctx)?;
        ctx.curry(self.cancel_action_args(unlocker))
    }

    /// The puzzle hashes of the bid, end, and cancel actions, which are the leaves of the action
    /// layer's merkle tree.
    pub fn action_hashes(&self) -> [Bytes32; 3] {
        [
            self.bid_action_hash().into(),
            self.end_action_hash().into(),
            self.cancel_action_hash().into(),
        ]
    }

    pub fn merkle_tree(&self) -> MerkleTree {
        MerkleTree::new(&self.action_hashes())
    }

    /// Rejects auctions that are misconfigured in a way the puzzles can't prevent. Things that can't
    /// be checked offline, such as whether the NFT and reserve are locked by the auction, must be
    /// checked separately.
    pub(crate) fn validate(&self) -> Result<(), DriverError> {
        let payments = self.settings.payments;

        for (name, bps) in [
            ("buyer's premium", payments.buyers_premium.bps),
            ("commission", payments.commission.bps),
            ("NFT royalty", self.royalty_bps()),
        ] {
            if bps > AUCTION_MAX_BPS {
                return Err(DriverError::AuctionBpsTooHigh { name, bps });
            }
        }

        let lock_puzzle_hash = auction_lock_p2_puzzle_hash(self.launcher_id);

        // The end action creates every payment from the reserve coin, so two payments with the
        // same puzzle hash could be identical coins, which would make the auction impossible to end
        let mut puzzle_hashes = HashSet::new();

        for (is_paid, puzzle_hash) in [
            (
                payments.buyers_premium.bps > 0,
                payments.buyers_premium.puzzle_hash,
            ),
            (payments.commission.bps > 0, payments.commission.puzzle_hash),
            (true, payments.payout_puzzle_hash),
        ] {
            if !is_paid {
                continue;
            }

            if puzzle_hash == SETTLEMENT_PAYMENT_HASH.into() || puzzle_hash == lock_puzzle_hash {
                return Err(DriverError::ReservedAuctionPaymentPuzzleHash(puzzle_hash));
            }

            if !puzzle_hashes.insert(puzzle_hash) {
                return Err(DriverError::DuplicateAuctionPaymentPuzzleHash(puzzle_hash));
            }
        }

        Ok(())
    }
}

impl SingletonInfo for AuctionInfo {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        ActionLayerArgs::curry_tree_hash(
            ReserveFinalizer2ndCurryArgs::curry_tree_hash(
                self.reserve.coin().puzzle_hash,
                self.reserve.p2_puzzle_hash(),
                RESERVE_FINALIZER_DEFAULT_RESERVE_AMOUNT_FROM_STATE_PROGRAM_HASH,
                self.launcher_id,
            ),
            self.merkle_tree().root(),
            self.state.tree_hash(),
        )
    }
}
