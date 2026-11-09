use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    offer::{NotarizedPayment, Payment, SettlementPaymentsSolution},
    singleton::SingletonSolution,
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{
    Conditions, announcement_id,
    conditions::CreateCoin,
    puzzles::{
        AUCTION_CANCEL_MESSAGE, AUCTION_CANCEL_MESSAGE_MODE, AuctionBid, AuctionBidActionSolution,
        AuctionNftUnlockerSolution, AuctionState, DefaultReserveAmountFromStateProgramArgs,
        ReserveFinalizerSolution,
    },
};
use clvm_traits::{FromClvm, clvm_list, clvm_quote};
use clvm_utils::ToTreeHash;
use clvmr::NodePtr;

use crate::{
    ActionLayer, ActionLayerSolution, Cat, CatSpend, DriverError, Finalizer, Layer, Nft,
    SettlementLayer, Singleton, SingletonInfo, SingletonLayer, Spend, SpendContext,
};

mod auction_info;
mod auction_launcher;
mod auction_lock;
mod auction_memo;
mod auction_reserve;
mod auction_settings;

pub use auction_info::*;
pub use auction_launcher::*;
pub use auction_lock::*;
pub use auction_memo::*;
pub use auction_reserve::*;
pub use auction_settings::*;

#[cfg(test)]
mod tests;

/// An English auction for an NFT. See [`AuctionInfo`] for details.
///
/// An auction is launched with [`Launcher::launch_auction`](crate::Launcher::launch_auction), and
/// discovered by other wallets with [`AuctionLaunch`]. Each spend of the auction runs one or more
/// action spends, such as [`Auction::bid_action_spend`], [`Auction::end_action_spend`], or
/// [`Auction::cancel_action_spend`], with [`Auction::spend`].
///
/// # Example
///
/// This launches an auction for a newly minted NFT, places a bid, and ends the auction, which sends
/// the NFT to the winning bidder.
///
/// ```
/// use std::num::NonZeroU64;
///
/// use chia_protocol::{Bytes32, Coin};
/// use chia_puzzle_types::Memos;
/// use chia_sdk_driver::{
///     AuctionBidVerifier, AuctionReserve, AuctionSettings, HashedPtr, Launcher, NftMint,
///     SpendContext, StandardLayer, auction_lock_p2_puzzle_hash,
/// };
/// use chia_sdk_test::Simulator;
/// use chia_sdk_types::{
///     Conditions,
///     puzzles::{AuctionBid, AuctionBpsPayment, AuctionPayments, AuctionTimings},
/// };
///
/// let mut sim = Simulator::new();
/// let mut ctx = SpendContext::new();
///
/// let seller = sim.bls(2);
/// let seller_p2 = StandardLayer::new(seller.pk);
///
/// // Mint an NFT, and lock it with the auction that is about to be launched
/// let (mint_nft, nft) = Launcher::new(seller.coin.coin_id(), 0)
///     .with_singleton_amount(1)
///     .mint_nft(&mut ctx, &NftMint::new(HashedPtr::NIL, seller.puzzle_hash, 300, None))?;
///
/// let launcher = Launcher::new(seller.coin.coin_id(), 1);
/// let lock_puzzle_hash = auction_lock_p2_puzzle_hash(launcher.coin().coin_id());
///
/// let nft = nft.spend_with(
///     &mut ctx,
///     &seller_p2,
///     Conditions::new().create_coin(lock_puzzle_hash, nft.coin.amount, Memos::None),
/// )?;
///
/// // The reserve starts out as an empty coin, which is also locked by the auction
/// let reserve = AuctionReserve::Xch(Coin::new(seller.coin.coin_id(), lock_puzzle_hash, 0));
///
/// let settings = AuctionSettings::new(
///     AuctionBidVerifier::Flat {
///         minimum_bid: NonZeroU64::new(100).unwrap(),
///         bid_increment: NonZeroU64::new(10).unwrap(),
///     },
///     AuctionTimings::new(1_000, 0),
///     AuctionPayments::new(
///         AuctionBpsPayment::new(300, Bytes32::new([1; 32])),
///         AuctionBpsPayment::new(100, Bytes32::new([2; 32])),
///         seller.puzzle_hash,
///     ),
/// );
///
/// let (launch_auction, auction) = launcher.launch_auction(&mut ctx, settings, reserve, &nft)?;
///
/// seller_p2.spend(
///     &mut ctx,
///     seller.coin,
///     mint_nft
///         .extend(launch_auction)
///         .create_coin(lock_puzzle_hash, 0, Memos::None),
/// )?;
///
/// sim.spend_coins(ctx.take(), &[seller.sk])?;
///
/// // The bidder funds the reserve with the bid, buyer's premium, and royalty, and asserts the
/// // bid's announcement so that the funds can't be used for anything else
/// let amount = auction.info.reserve_amount_for_bid(100).unwrap();
/// let bidder = sim.bls(amount);
/// let bid = AuctionBid::new(100, bidder.puzzle_hash);
///
/// let bid_action = auction.bid_action_spend(&mut ctx, bid, false)?;
///
/// StandardLayer::new(bidder.pk).spend(
///     &mut ctx,
///     bidder.coin,
///     Conditions::new().assert_coin_announcement(auction.bid_announcement_id(bid)),
/// )?;
///
/// let auction = auction.spend(&mut ctx, vec![bid_action], vec![])?;
/// sim.spend_coins(ctx.take(), &[bidder.sk])?;
/// assert_eq!(auction.info.state.winning_bid, bid);
///
/// // After the end time, anyone can end the auction and unlock the NFT to the winner
/// sim.set_next_timestamp(1_000)?;
///
/// let end_action = auction.end_action_spend(&mut ctx, &nft)?;
/// let nft = auction.unlock_nft(&mut ctx, &nft)?;
/// let auction = auction.spend(&mut ctx, vec![end_action], vec![])?;
/// sim.spend_coins(ctx.take(), &[])?;
///
/// assert!(auction.info.state.ended);
/// assert_eq!(nft.info.p2_puzzle_hash, bidder.puzzle_hash);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub type Auction = Singleton<AuctionInfo>;

/// The opcode that actions use to output conditions which are forwarded to the reserve coin.
const RESERVE_CONDITION_OPCODE: i64 = -42;

/// The result of running an auction's actions, as the action layer and reserve finalizer will.
#[derive(Debug, Clone)]
pub struct AuctionChildState {
    /// The state of the auction after the actions have run.
    pub state: AuctionState,
    /// The conditions output by the reserve coin, including the creation of the new reserve.
    pub reserve_conditions: Vec<NodePtr>,
    /// The amount of the ephemeral settlement coin that pays the NFT royalty, if one is created.
    pub royalty_amount: Option<u64>,
}

impl Auction {
    /// The action spend that places `bid`. The bidder must fund the reserve with
    /// [`AuctionInfo::reserve_amount_for_bid`], typically by spending coins in the same spend bundle
    /// which assert [`Auction::bid_announcement_id`].
    ///
    /// If `grace` is true, the bid must be placed within the grace period after the previous
    /// spend, rather than before the end time.
    pub fn bid_action_spend(
        &self,
        ctx: &mut SpendContext,
        bid: AuctionBid,
        grace: bool,
    ) -> Result<Spend, DriverError> {
        let puzzle = self.info.construct_bid_action(ctx)?;
        let solution = ctx.alloc(&AuctionBidActionSolution::new(bid, grace))?;
        Ok(Spend::new(puzzle, solution))
    }

    /// The coin announcement that the auction coin creates when `bid` is placed. Bidders should
    /// assert it in the spends that fund the reserve, so that their funds can't be used for any
    /// other bid.
    pub fn bid_announcement_id(&self, bid: AuctionBid) -> Bytes32 {
        announcement_id(self.coin.coin_id(), bid.tree_hash())
    }

    /// The action spend that ends the auction. `nft` is the locked NFT, which must be unlocked with
    /// [`Auction::unlock_nft`] in the same spend bundle.
    pub fn end_action_spend(
        &self,
        ctx: &mut SpendContext,
        nft: &Nft,
    ) -> Result<Spend, DriverError> {
        let puzzle = self.info.construct_end_action(ctx)?;
        // The end action passes its solution to the unlocker after the winning bid
        let solution = ctx.alloc(&clvm_list!(nft.coin.amount))?;
        Ok(Spend::new(puzzle, solution))
    }

    /// The action spend that cancels the auction before the first bid, which sends the NFT back to
    /// the payout puzzle hash. `nft` is the locked NFT, which must be unlocked with
    /// [`Auction::unlock_nft`] in the same spend bundle. A coin with the payout puzzle hash must
    /// also output the conditions from [`Auction::cancel_authorization`] in the same spend bundle.
    pub fn cancel_action_spend(
        &self,
        ctx: &mut SpendContext,
        nft: &Nft,
    ) -> Result<Spend, DriverError> {
        let puzzle = self.info.construct_cancel_action(ctx)?;
        // Like the end action, the cancel action passes its solution to the unlocker
        let solution = ctx.alloc(&clvm_list!(nft.coin.amount))?;
        Ok(Spend::new(puzzle, solution))
    }

    /// The conditions that a coin with the auction's payout puzzle hash must output to authorize
    /// cancelling this auction coin. They only authorize a cancel of this exact coin, so they can't
    /// be reused once the auction has been spent.
    pub fn cancel_authorization(&self, ctx: &mut SpendContext) -> Result<Conditions, DriverError> {
        let receiver = ctx.alloc(&self.coin.coin_id())?;
        Ok(Conditions::new().send_message(
            AUCTION_CANCEL_MESSAGE_MODE,
            AUCTION_CANCEL_MESSAGE.to_vec().into(),
            vec![receiver],
        ))
    }

    /// Spends the locked NFT to the winning bidder, or back to the payout puzzle hash if there are
    /// no bids, and returns the NFT that is created. This must be in the same spend bundle as the
    /// end or cancel action, and `self` must be the auction before it's ended or cancelled.
    pub fn unlock_nft(&self, ctx: &mut SpendContext, nft: &Nft) -> Result<Nft, DriverError> {
        let unlocker = self.info.construct_nft_unlocker(ctx)?;
        let unlocker_solution = ctx.alloc(&AuctionNftUnlockerSolution::new(
            self.info.state.winning_bid,
            nft.coin.amount,
        ))?;
        let conditions = ctx.run(unlocker, unlocker_solution)?;
        let delegated_puzzle = ctx.alloc(&clvm_quote!(conditions))?;

        let spend = spend_auction_lock(
            ctx,
            self.info.launcher_id,
            self.info.inner_puzzle_hash(),
            Spend::new(delegated_puzzle, NodePtr::NIL),
        )?;

        nft.spend(ctx, spend)
    }

    /// Runs the action spends to determine the next state and the reserve coin's conditions,
    /// exactly as the action layer and reserve finalizer will on chain.
    pub fn child_state(
        &self,
        ctx: &mut SpendContext,
        action_spends: &[Spend],
    ) -> Result<AuctionChildState, DriverError> {
        let mut state = self.info.state;
        let mut truth = ctx.alloc(&(NodePtr::NIL, state))?;
        let mut reserve_conditions = Vec::new();
        let mut royalty_amount = None;

        for action_spend in action_spends {
            let solution = ctx.alloc(&(truth, action_spend.solution))?;
            let output = ctx.run(action_spend.puzzle, solution)?;
            let (new_truth, conditions) = ctx.extract::<(NodePtr, Vec<NodePtr>)>(output)?;
            let (_, new_state) = ctx.extract::<(NodePtr, AuctionState)>(new_truth)?;

            // The reserve finalizer collects each action's reserve conditions in reverse order
            let mut action_reserve_conditions = Vec::new();

            for condition in conditions.into_iter().rev() {
                let (opcode, condition) = ctx.extract::<(i64, NodePtr)>(condition)?;

                if opcode == RESERVE_CONDITION_OPCODE {
                    action_reserve_conditions.push(condition);
                }
            }

            // Only the end action pays the royalty, and the cancel action never creates any
            // reserve conditions. A bid refund can also be sent to the settlement puzzle hash, if
            // that's the puzzle hash the bidder chose.
            if new_state.ended && !state.ended {
                for &condition in &action_reserve_conditions {
                    if let Ok(create_coin) = ctx.extract::<CreateCoin<NodePtr>>(condition)
                        && create_coin.puzzle_hash == SETTLEMENT_PAYMENT_HASH.into()
                    {
                        royalty_amount = Some(create_coin.amount);
                    }
                }
            }

            reserve_conditions.extend(action_reserve_conditions);
            truth = new_truth;
            state = new_state;
        }

        let p2_puzzle_hash = self.info.reserve.p2_puzzle_hash();
        let hint = ctx.hint(p2_puzzle_hash)?;
        let new_reserve =
            ctx.alloc(&CreateCoin::new(p2_puzzle_hash, state.reserve_amount, hint))?;
        reserve_conditions.insert(0, new_reserve);

        Ok(AuctionChildState {
            state,
            reserve_conditions,
            royalty_amount,
        })
    }

    /// Spends the auction with the given action spends, along with the reserve coin and the
    /// settlement coin that pays the NFT royalty (if there is one). Returns the child auction.
    ///
    /// For CAT reserves, `other_cat_spends` are spent in the same CAT ring as the reserve, which is
    /// typically used to fund a bid. They must be empty for XCH reserves.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        action_spends: Vec<Spend>,
        mut other_cat_spends: Vec<CatSpend>,
    ) -> Result<Self, DriverError> {
        if matches!(self.info.reserve, AuctionReserve::Xch(_)) && !other_cat_spends.is_empty() {
            return Err(DriverError::UnexpectedAuctionCatSpends);
        }

        let AuctionChildState {
            state,
            reserve_conditions,
            royalty_amount,
        } = self.child_state(ctx, &action_spends)?;

        let royalty_payment = if let Some(amount) = royalty_amount {
            let royalty_puzzle_hash = self.info.nft_royalty.puzzle_hash;
            Some((
                amount,
                NotarizedPayment::new(
                    self.info.nft_royalty.launcher_id,
                    vec![Payment::new(
                        royalty_puzzle_hash,
                        amount,
                        ctx.hint(royalty_puzzle_hash)?,
                    )],
                ),
            ))
        } else {
            None
        };

        let reserve_amount_from_state_program =
            ctx.alloc_mod::<DefaultReserveAmountFromStateProgramArgs>()?;

        let action_layer = ActionLayer::new(
            self.info.merkle_tree().root(),
            self.info.state,
            Finalizer::Reserve {
                reserve_full_puzzle_hash: self.info.reserve.coin().puzzle_hash,
                reserve_inner_puzzle_hash: self.info.reserve.p2_puzzle_hash(),
                reserve_amount_from_state_program,
                hint: self.info.launcher_id,
            },
        );

        let action_spend_hashes = action_spends
            .iter()
            .map(|spend| ctx.tree_hash(spend.puzzle).into())
            .collect::<Vec<_>>();

        let proofs = action_layer
            .get_proofs(&self.info.action_hashes(), &action_spend_hashes)
            .ok_or(DriverError::InvalidMerkleProof)?;

        let finalizer_solution = ctx.alloc(&ReserveFinalizerSolution {
            reserve_parent_id: self.info.reserve.coin().parent_coin_info,
        })?;

        let inner_spend = action_layer.construct_spend(
            ctx,
            ActionLayerSolution {
                proofs,
                action_spends,
                finalizer_solution,
            },
        )?;

        let coin_spend = SingletonLayer::new(self.info.launcher_id, inner_spend.puzzle)
            .construct_coin_spend(
                ctx,
                self.coin,
                SingletonSolution {
                    lineage_proof: self.proof,
                    amount: self.coin.amount,
                    inner_solution: inner_spend.solution,
                },
            )?;

        ctx.insert(coin_spend);

        let delegated_puzzle = ctx.alloc(&clvm_quote!(reserve_conditions))?;

        let reserve_spend = spend_auction_lock(
            ctx,
            self.info.launcher_id,
            self.info.inner_puzzle_hash(),
            Spend::new(delegated_puzzle, NodePtr::NIL),
        )?;

        match self.info.reserve {
            AuctionReserve::Xch(coin) => {
                ctx.spend(coin, reserve_spend)?;

                if let Some((amount, royalty_payment)) = royalty_payment {
                    let settlement_coin =
                        Coin::new(coin.coin_id(), SETTLEMENT_PAYMENT_HASH.into(), amount);
                    let settlement_spend = SettlementLayer.construct_coin_spend(
                        ctx,
                        settlement_coin,
                        SettlementPaymentsSolution::new(vec![royalty_payment]),
                    )?;
                    ctx.insert(settlement_spend);
                }
            }
            AuctionReserve::Cat(cat) => {
                if let Some((amount, royalty_payment)) = royalty_payment {
                    let settlement_spend = SettlementLayer.construct_spend(
                        ctx,
                        SettlementPaymentsSolution::new(vec![royalty_payment]),
                    )?;
                    other_cat_spends.push(CatSpend::new(
                        cat.child(SETTLEMENT_PAYMENT_HASH.into(), amount),
                        settlement_spend,
                    ));
                }

                other_cat_spends.push(CatSpend::new(cat, reserve_spend));
                Cat::spend_all(ctx, &other_cat_spends)?;
            }
        }

        Ok(self.child(self.info.reserve, state))
    }

    /// Parses the child of this auction, given the solution that this auction coin was spent with
    /// on the blockchain.
    ///
    /// The solution must come from a confirmed spend, since the action puzzles in it are run
    /// without checking that they're the auction's.
    pub fn parse_child(
        &self,
        ctx: &mut SpendContext,
        solution: NodePtr,
    ) -> Result<Self, DriverError> {
        let singleton_solution = SingletonSolution::<NodePtr>::from_clvm(ctx, solution)?;
        let action_layer_solution = ActionLayer::<AuctionState, NodePtr>::parse_solution(
            ctx,
            singleton_solution.inner_solution,
        )?;
        let finalizer_solution =
            ctx.extract::<ReserveFinalizerSolution>(action_layer_solution.finalizer_solution)?;

        // Whoever spent the auction chose which coin to spend as the reserve, so it may not be the
        // reserve that was being tracked
        let spent_reserve = self
            .info
            .reserve
            .with_parent_coin_info(finalizer_solution.reserve_parent_id);

        let child = self.child_state(ctx, &action_layer_solution.action_spends)?;

        Ok(self.child(spent_reserve, child.state))
    }

    fn child(&self, spent_reserve: AuctionReserve, state: AuctionState) -> Self {
        self.child_with(
            AuctionInfo {
                state,
                reserve: spent_reserve.child(state.reserve_amount),
                ..self.info
            },
            AUCTION_SINGLETON_AMOUNT,
        )
    }
}
