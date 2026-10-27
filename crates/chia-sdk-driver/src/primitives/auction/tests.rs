use std::{collections::HashMap, fmt::Debug, num::NonZeroU64};

use anyhow::{Result, anyhow};
use chia_bls::{SecretKey, Signature};
use chia_consensus::{
    consensus_constants::TEST_CONSTANTS, spendbundle_conditions::get_conditions_from_spendbundle,
};
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, SpendBundle};
use chia_puzzle_types::{
    EveProof, Memos, Proof,
    singleton::{LauncherSolution, SingletonSolution},
};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_test::{BlsPairWithCoin, Simulator, sign_transaction};
use chia_sdk_types::{
    Conditions, announcement_id,
    conditions::{AssertMyAmount, CreateCoin, TransferNft},
    puzzles::{
        AUCTION_CANCEL_MESSAGE, AUCTION_CANCEL_MESSAGE_MODE, AuctionBid, AuctionBpsPayment,
        AuctionNftUnlockerSolution, AuctionPayments, AuctionState, AuctionTimings,
        DefaultReserveAmountFromStateProgramArgs, ReserveFinalizerSolution,
    },
};
use clvm_traits::{ToClvm, clvm_list, clvm_quote};
use clvm_utils::ToTreeHash;
use clvmr::{Allocator, NodePtr};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rstest::rstest;

use crate::{
    AUCTION_MAX_BPS, ActionLayer, ActionLayerSolution, Auction, AuctionBidVerifier, AuctionInfo,
    AuctionLaunch, AuctionMemo, AuctionReserve, AuctionSettings, Cat, CatSpend, DriverError,
    Finalizer, HashedPtr, IntermediateLauncher, Launcher, Layer, Nft, NftMint, RoyaltyInfo,
    SingletonInfo, SingletonLayer, Spend, SpendContext, SpendWithConditions, StandardLayer,
    auction_lock_p2_puzzle_hash, calculate_auction_payment, spend_auction_lock,
};

const PREMIUM_PUZZLE_HASH: Bytes32 = Bytes32::new([1; 32]);
const COMMISSION_PUZZLE_HASH: Bytes32 = Bytes32::new([2; 32]);
const ROYALTY_PUZZLE_HASH: Bytes32 = Bytes32::new([3; 32]);
const HIDDEN_PUZZLE_HASH: Bytes32 = Bytes32::new([4; 32]);

const END_TIME: u64 = 1_000;

const CAT_SUPPLY: u64 = 1_000_000_000_000_000;

/// The error for a spend bundle where the auction's puzzle raises during consensus validation.
const PUZZLE_RAISED: &str = "GeneratorRuntimeError";

fn nz(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}

fn flat(minimum_bid: u64, bid_increment: u64) -> AuctionBidVerifier {
    AuctionBidVerifier::Flat {
        minimum_bid: nz(minimum_bid),
        bid_increment: nz(bid_increment),
    }
}

fn percent(minimum_bid: u64, bid_increment_bps: u64) -> AuctionBidVerifier {
    AuctionBidVerifier::Percent {
        minimum_bid: nz(minimum_bid),
        bid_increment_bps: nz(bid_increment_bps),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReserveKind {
    Xch,
    Cat,
    RevocableCat,
}

#[derive(Debug, Clone, Copy)]
struct Config {
    bid_verifier: AuctionBidVerifier,
    timings: AuctionTimings,
    buyers_premium_bps: u64,
    commission_bps: u64,
    royalty_bps: u16,
    reserve: ReserveKind,
    did_owner: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bid_verifier: flat(100, 100),
            timings: AuctionTimings::new(END_TIME, 0),
            buyers_premium_bps: 300,
            commission_bps: 100,
            royalty_bps: 500,
            reserve: ReserveKind::Xch,
            did_owner: false,
        }
    }
}

impl Config {
    fn settings(&self, payout_puzzle_hash: Bytes32) -> AuctionSettings {
        AuctionSettings::new(
            self.bid_verifier,
            self.timings,
            AuctionPayments::new(
                AuctionBpsPayment::new(self.buyers_premium_bps, PREMIUM_PUZZLE_HASH),
                AuctionBpsPayment::new(self.commission_bps, COMMISSION_PUZZLE_HASH),
                payout_puzzle_hash,
            ),
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct Bidder {
    bid: AuctionBid,
    reserve_amount: u64,
}

/// Builds the auction settings from the config, the seller's puzzle hash, and the auction lock
/// puzzle hash.
type SettingsFn = fn(&Config, Bytes32, Bytes32) -> AuctionSettings;

fn default_settings(config: &Config, seller: Bytes32, _: Bytes32) -> AuctionSettings {
    config.settings(seller)
}

/// An auction for a freshly minted NFT, launched and confirmed in a simulator.
struct Harness {
    sim: Simulator,
    ctx: SpendContext,
    config: Config,
    seller: BlsPairWithCoin,
    auction: Auction,
    nft: Nft,
    bank: Option<(BlsPairWithCoin, Cat)>,
}

impl Harness {
    fn new(config: Config) -> Result<Self> {
        Self::with_settings(config, default_settings)
    }

    fn with_settings(config: Config, settings: SettingsFn) -> Result<Self> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let seller = sim.bls(CAT_SUPPLY + 10);
        let seller_p2 = StandardLayer::new(seller.pk);
        let mut funding_coin = seller.coin;

        let did = if config.did_owner {
            let (create_did, did) =
                Launcher::new(funding_coin.coin_id(), 1).create_simple_did(&mut ctx, &seller_p2)?;
            let change = funding_coin.amount - 1;
            seller_p2.spend(
                &mut ctx,
                funding_coin,
                create_did.create_coin(seller.puzzle_hash, change, Memos::None),
            )?;
            sim.spend_coins(ctx.take(), std::slice::from_ref(&seller.sk))?;
            funding_coin = Coin::new(funding_coin.coin_id(), seller.puzzle_hash, change);
            Some(did)
        } else {
            None
        };

        let mut mint = NftMint::new(
            HashedPtr::NIL,
            seller.puzzle_hash,
            config.royalty_bps,
            did.map(|did| {
                TransferNft::new(
                    Some(did.info.launcher_id),
                    Vec::new(),
                    Some(did.info.inner_puzzle_hash().into()),
                )
            }),
        );
        mint.royalty_puzzle_hash = ROYALTY_PUZZLE_HASH;

        let mut conditions = Conditions::new();

        let nft = if let Some(did) = did {
            let (mint_nft, nft) = IntermediateLauncher::new(did.coin.coin_id(), 0, 1)
                .create(&mut ctx)?
                .mint_nft(&mut ctx, &mint)?;
            let _ = did.update(&mut ctx, &seller_p2, mint_nft)?;
            nft
        } else {
            let (mint_nft, nft) = Launcher::new(funding_coin.coin_id(), 0)
                .with_singleton_amount(1)
                .mint_nft(&mut ctx, &mint)?;
            conditions = conditions.extend(mint_nft);
            nft
        };

        let launcher = Launcher::new(funding_coin.coin_id(), 1);
        let lock_puzzle_hash = auction_lock_p2_puzzle_hash(launcher.coin().coin_id());

        let locked_nft = nft.spend_with(
            &mut ctx,
            &seller_p2,
            Conditions::new().create_coin(lock_puzzle_hash, nft.coin.amount, Memos::None),
        )?;

        let mut change = funding_coin.amount - 2;
        let mut bank = None;

        let reserve = match config.reserve {
            ReserveKind::Xch => {
                conditions = conditions.create_coin(lock_puzzle_hash, 0, Memos::None);
                AuctionReserve::Xch(Coin::new(funding_coin.coin_id(), lock_puzzle_hash, 0))
            }
            ReserveKind::Cat | ReserveKind::RevocableCat => {
                let bank_key = sim.bls(0);
                let hidden_puzzle_hash =
                    (config.reserve == ReserveKind::RevocableCat).then_some(HIDDEN_PUZZLE_HASH);

                let (issue, cats) = Cat::single_issuance(
                    &mut ctx,
                    funding_coin.coin_id(),
                    hidden_puzzle_hash,
                    CAT_SUPPLY,
                    Conditions::new()
                        .create_coin(bank_key.puzzle_hash, CAT_SUPPLY, Memos::None)
                        .create_coin(lock_puzzle_hash, 0, Memos::None),
                )?;
                conditions = conditions.extend(issue);
                change -= CAT_SUPPLY;

                let find = |puzzle_hash| {
                    cats.iter()
                        .find(|cat| cat.info.p2_puzzle_hash == puzzle_hash)
                        .copied()
                        .ok_or(anyhow!("missing CAT output"))
                };

                let bank_cat = find(bank_key.puzzle_hash)?;
                bank = Some((bank_key, bank_cat));
                AuctionReserve::Cat(find(lock_puzzle_hash)?)
            }
        };

        let (launch, auction) = launcher.launch_auction(
            &mut ctx,
            settings(&config, seller.puzzle_hash, lock_puzzle_hash),
            reserve,
            &locked_nft,
        )?;

        seller_p2.spend(
            &mut ctx,
            funding_coin,
            conditions
                .extend(launch)
                .create_coin(seller.puzzle_hash, change, Memos::None),
        )?;

        let coin_spends = ctx.take();
        sim.spend_coins(coin_spends.clone(), std::slice::from_ref(&seller.sk))?;
        assert_mempool_valid(&coin_spends)?;

        let launcher_spend = sim
            .coin_spend(auction.info.launcher_id)
            .ok_or(anyhow!("missing launcher spend"))?;
        let launcher_solution = launcher_spend.solution.to_clvm(&mut ctx)?;
        let launch = AuctionLaunch::parse(&ctx, launcher_spend.coin, launcher_solution)
            .ok_or(anyhow!("launcher isn't for an auction"))?;
        assert_eq!(launch.nft_coin_id(), locked_nft.coin.coin_id());
        assert_eq!(launch.into_auction(nft_royalty(&locked_nft))?, auction);

        Ok(Self {
            sim,
            ctx,
            config,
            seller,
            auction,
            nft: locked_nft,
            bank,
        })
    }

    /// Checks that `coin_spends` passes mempool rules, and that the auction's new state can be
    /// parsed from the blockchain.
    fn confirmed(&mut self, coin_spends: &[CoinSpend], auction: &Auction) -> Result<()> {
        assert_mempool_valid(coin_spends)?;

        let coin_spend = self
            .sim
            .coin_spend(self.auction.coin.coin_id())
            .ok_or(anyhow!("missing auction spend"))?;
        let solution = coin_spend.solution.to_clvm(&mut self.ctx)?;
        let parsed = self.auction.parse_child(&mut self.ctx, solution)?;
        assert_eq!(&parsed, auction);

        self.auction = *auction;
        Ok(())
    }

    /// Clears any spends that were left in the context by a failed operation.
    fn cleanup<T>(&mut self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            self.ctx.take();
        }
        result
    }

    fn set_time(&mut self, time: u64) -> Result<()> {
        self.sim.set_next_timestamp(time)?;
        Ok(())
    }

    fn bid(&mut self, amount: u64) -> Result<Bidder> {
        self.bid_with_grace(amount, false)
    }

    fn bid_with_grace(&mut self, amount: u64, grace: bool) -> Result<Bidder> {
        Ok(self.bids(&[(amount, grace)])?[0])
    }

    /// Places every bid in a single spend of the auction. Each bidder funds exactly the reserve
    /// amount for their bid, and asserts the bid's announcement so the funds can't be reused.
    fn bids(&mut self, bids: &[(u64, bool)]) -> Result<Vec<Bidder>> {
        let result = self.try_bids(bids);
        self.cleanup(result)
    }

    fn try_bids(&mut self, bids: &[(u64, bool)]) -> Result<Vec<Bidder>> {
        let mut bidders = Vec::new();
        let mut actions = Vec::new();
        let mut cat_spends = Vec::new();
        let mut keys = Vec::new();

        for &(amount, grace) in bids {
            let reserve_amount = self
                .auction
                .info
                .reserve_amount_for_bid(amount)
                .ok_or(anyhow!("reserve amount overflow"))?;

            let key = if self.bank.is_some() {
                self.sim.bls(0)
            } else {
                self.sim.bls(reserve_amount)
            };

            let bid = AuctionBid::new(amount, key.puzzle_hash);
            actions.push(self.auction.bid_action_spend(&mut self.ctx, bid, grace)?);

            let conditions =
                Conditions::new().assert_coin_announcement(self.auction.bid_announcement_id(bid));
            let p2 = StandardLayer::new(key.pk);

            if self.bank.is_some() {
                let cat = self.fund_cat(key.puzzle_hash, reserve_amount)?;
                let spend = p2.spend_with_conditions(&mut self.ctx, conditions)?;
                cat_spends.push(CatSpend::new(cat, spend));
            } else {
                p2.spend(&mut self.ctx, key.coin, conditions)?;
            }

            keys.push(key.sk.clone());
            bidders.push(Bidder {
                bid,
                reserve_amount,
            });
        }

        let auction = self.auction.spend(&mut self.ctx, actions, cat_spends)?;
        let coin_spends = self.ctx.take();
        self.sim.spend_coins(coin_spends.clone(), &keys)?;
        self.confirmed(&coin_spends, &auction)?;

        Ok(bidders)
    }

    /// Ends the auction and unlocks the NFT, returning the NFT sent to the winning bidder.
    fn end(&mut self) -> Result<Nft> {
        let result = self.try_end();
        self.cleanup(result)
    }

    fn try_end(&mut self) -> Result<Nft> {
        let end_action = self.auction.end_action_spend(&mut self.ctx, &self.nft)?;
        let nft = self.auction.unlock_nft(&mut self.ctx, &self.nft)?;
        let auction = self
            .auction
            .spend(&mut self.ctx, vec![end_action], vec![])?;
        let coin_spends = self.ctx.take();
        self.sim.spend_coins(coin_spends.clone(), &[])?;
        self.confirmed(&coin_spends, &auction)?;
        self.nft = nft;
        Ok(nft)
    }

    /// Cancels the auction, authorized by a coin with the seller's payout puzzle hash, and unlocks
    /// the NFT back to the seller.
    fn cancel(&mut self) -> Result<Nft> {
        let authorizer = self.seller_coin();
        self.cancel_authorized_by(Some(authorizer))
    }

    /// Like [`Harness::cancel`], but the cancel message is sent by `authorizer`'s coin instead, or
    /// not at all.
    fn cancel_authorized_by(&mut self, authorizer: Option<BlsPairWithCoin>) -> Result<Nft> {
        let authorization = match authorizer {
            Some(authorizer) => {
                let conditions = self.auction.cancel_authorization(&mut self.ctx)?;
                Some((authorizer, conditions))
            }
            None => None,
        };
        self.cancel_with_authorization(authorization)
    }

    /// Like [`Harness::cancel`], but the given coin outputs the given conditions in place of the
    /// cancel authorization.
    fn cancel_with_authorization(
        &mut self,
        authorization: Option<(BlsPairWithCoin, Conditions)>,
    ) -> Result<Nft> {
        let result = self.try_cancel(authorization);
        self.cleanup(result)
    }

    fn try_cancel(&mut self, authorization: Option<(BlsPairWithCoin, Conditions)>) -> Result<Nft> {
        let mut keys = Vec::new();

        if let Some((authorizer, conditions)) = authorization {
            StandardLayer::new(authorizer.pk).spend(&mut self.ctx, authorizer.coin, conditions)?;
            keys.push(authorizer.sk);
        }

        let cancel_action = self.auction.cancel_action_spend(&mut self.ctx, &self.nft)?;
        let nft = self.auction.unlock_nft(&mut self.ctx, &self.nft)?;
        let auction = self
            .auction
            .spend(&mut self.ctx, vec![cancel_action], vec![])?;
        let coin_spends = self.ctx.take();
        self.sim.spend_coins(coin_spends.clone(), &keys)?;
        self.confirmed(&coin_spends, &auction)?;
        self.nft = nft;
        Ok(nft)
    }

    /// Spends the auction with `action_spends` like [`Auction::spend`], along with any spends
    /// already in the context, but without running the actions in the driver first. This makes
    /// sure that invalid actions are rejected on chain, and not just by the driver. Only XCH
    /// reserves are supported, and the reserve is recreated with `reserve_amount`.
    fn spend_unchecked(
        &mut self,
        action_spends: Vec<Spend>,
        reserve_amount: u64,
        keys: &[SecretKey],
    ) -> Result<()> {
        let result = self.try_spend_unchecked(action_spends, reserve_amount, keys);
        self.cleanup(result)
    }

    fn try_spend_unchecked(
        &mut self,
        action_spends: Vec<Spend>,
        reserve_amount: u64,
        keys: &[SecretKey],
    ) -> Result<()> {
        let info = self.auction.info;
        let AuctionReserve::Xch(reserve) = info.reserve else {
            return Err(anyhow!("only XCH reserves are supported"));
        };

        let reserve_amount_from_state_program = self
            .ctx
            .alloc_mod::<DefaultReserveAmountFromStateProgramArgs>()?;

        let action_layer = ActionLayer::new(
            info.merkle_tree().root(),
            info.state,
            Finalizer::Reserve {
                reserve_full_puzzle_hash: reserve.puzzle_hash,
                reserve_inner_puzzle_hash: info.reserve.p2_puzzle_hash(),
                reserve_amount_from_state_program,
                hint: info.launcher_id,
            },
        );

        let action_spend_hashes = action_spends
            .iter()
            .map(|spend| self.ctx.tree_hash(spend.puzzle).into())
            .collect::<Vec<_>>();
        let proofs = action_layer
            .get_proofs(&info.action_hashes(), &action_spend_hashes)
            .ok_or(anyhow!("invalid merkle proof"))?;

        let finalizer_solution = self.ctx.alloc(&ReserveFinalizerSolution {
            reserve_parent_id: reserve.parent_coin_info,
        })?;
        let inner_spend = action_layer.construct_spend(
            &mut self.ctx,
            ActionLayerSolution {
                proofs,
                action_spends,
                finalizer_solution,
            },
        )?;
        let coin_spend = SingletonLayer::new(info.launcher_id, inner_spend.puzzle)
            .construct_coin_spend(
                &mut self.ctx,
                self.auction.coin,
                SingletonSolution {
                    lineage_proof: self.auction.proof,
                    amount: self.auction.coin.amount,
                    inner_solution: inner_spend.solution,
                },
            )?;
        self.ctx.insert(coin_spend);

        let p2_puzzle_hash = info.reserve.p2_puzzle_hash();
        let hint = self.ctx.hint(p2_puzzle_hash)?;
        let new_reserve = CreateCoin::new(p2_puzzle_hash, reserve_amount, hint);
        let delegated_puzzle = self.ctx.alloc(&clvm_quote!(vec![new_reserve]))?;
        let reserve_spend = spend_auction_lock(
            &mut self.ctx,
            info.launcher_id,
            info.inner_puzzle_hash(),
            Spend::new(delegated_puzzle, NodePtr::NIL),
        )?;
        self.ctx.spend(reserve, reserve_spend)?;

        // The simulator's signer runs every puzzle, so the auction's spend is left out of signing
        // (it doesn't need a signature) to make sure it's only ever run by consensus validation
        let coin_spends = self.ctx.take();
        let auction_coin_id = self.auction.coin.coin_id();
        let signed = coin_spends
            .iter()
            .filter(|coin_spend| coin_spend.coin.coin_id() != auction_coin_id)
            .cloned()
            .collect::<Vec<_>>();
        let signature = sign_transaction(&signed, keys)?;
        self.sim
            .new_transaction(SpendBundle::new(coin_spends.clone(), signature))?;
        assert_mempool_valid(&coin_spends)?;

        let solution = self
            .sim
            .coin_spend(self.auction.coin.coin_id())
            .ok_or(anyhow!("missing auction spend"))?
            .solution
            .to_clvm(&mut self.ctx)?;
        self.auction = self.auction.parse_child(&mut self.ctx, solution)?;
        Ok(())
    }

    /// A new empty coin owned by the seller, with the auction's payout puzzle hash.
    fn seller_coin(&mut self) -> BlsPairWithCoin {
        let coin = self.sim.new_coin(self.seller.puzzle_hash, 0);
        BlsPairWithCoin {
            coin,
            ..self.seller.clone()
        }
    }

    /// Creates an empty coin locked by the auction, which can be spent as the reserve in place of
    /// the real one while the auction's reserve amount is 0.
    fn decoy_reserve(&mut self) -> Result<AuctionReserve> {
        let lock_puzzle_hash = auction_lock_p2_puzzle_hash(self.auction.info.launcher_id);

        if self.bank.is_some() {
            return Ok(AuctionReserve::Cat(self.fund_cat(lock_puzzle_hash, 0)?));
        }

        let key = self.sim.bls(0);
        StandardLayer::new(key.pk).spend(
            &mut self.ctx,
            key.coin,
            Conditions::new().create_coin(lock_puzzle_hash, 0, Memos::None),
        )?;
        self.sim
            .spend_coins(self.ctx.take(), std::slice::from_ref(&key.sk))?;

        Ok(AuctionReserve::Xch(Coin::new(
            key.coin.coin_id(),
            lock_puzzle_hash,
            0,
        )))
    }

    fn fund_cat(&mut self, puzzle_hash: Bytes32, amount: u64) -> Result<Cat> {
        let (key, cat) = self.bank.clone().ok_or(anyhow!("no CAT bank"))?;
        let change = cat.coin.amount - amount;

        let spend = StandardLayer::new(key.pk).spend_with_conditions(
            &mut self.ctx,
            Conditions::new()
                .create_coin(puzzle_hash, amount, Memos::None)
                .create_coin(key.puzzle_hash, change, Memos::None),
        )?;
        Cat::spend_all(&mut self.ctx, &[CatSpend::new(cat, spend)])?;
        self.sim
            .spend_coins(self.ctx.take(), std::slice::from_ref(&key.sk))?;

        let bank_cat = cat.child(key.puzzle_hash, change);
        self.bank = Some((key, bank_cat));
        Ok(cat.child(puzzle_hash, amount))
    }

    /// A coin created by the reserve coin `reserve` with the given puzzle hash and amount.
    fn reserve_output(reserve: AuctionReserve, puzzle_hash: Bytes32, amount: u64) -> Coin {
        match reserve {
            AuctionReserve::Xch(coin) => Coin::new(coin.coin_id(), puzzle_hash, amount),
            AuctionReserve::Cat(cat) => cat.child(puzzle_hash, amount).coin,
        }
    }

    /// The royalty payment made by the settlement coin that the reserve coin `reserve` creates.
    fn royalty_output(reserve: AuctionReserve, amount: u64) -> Coin {
        match reserve {
            AuctionReserve::Xch(coin) => {
                let settlement = Coin::new(coin.coin_id(), SETTLEMENT_PAYMENT_HASH.into(), amount);
                Coin::new(settlement.coin_id(), ROYALTY_PUZZLE_HASH, amount)
            }
            AuctionReserve::Cat(cat) => {
                cat.child(SETTLEMENT_PAYMENT_HASH.into(), amount)
                    .child(ROYALTY_PUZZLE_HASH, amount)
                    .coin
            }
        }
    }

    /// Asserts that the reserve coin `reserve` paid out a winning bid of `amount` to the seller and
    /// every fee recipient, and created an empty reserve for the ended auction.
    fn assert_settled(&self, reserve: AuctionReserve, amount: u64) {
        let premium = fee(amount, self.config.buyers_premium_bps);
        let commission = fee(amount, self.config.commission_bps);
        let royalty = fee(amount, self.config.royalty_bps.into());
        let payout = amount - commission;

        assert_eq!(reserve.coin().amount, amount + premium + royalty);

        for (puzzle_hash, payment) in [
            (PREMIUM_PUZZLE_HASH, premium),
            (COMMISSION_PUZZLE_HASH, commission),
            (self.seller.puzzle_hash, payout),
        ] {
            let coin = Self::reserve_output(reserve, puzzle_hash, payment);
            if payment > 0 {
                self.assert_unspent(coin);
            } else {
                self.assert_missing(coin);
            }
        }

        if royalty > 0 {
            self.assert_unspent(Self::royalty_output(reserve, royalty));
        } else {
            self.assert_missing(Self::reserve_output(
                reserve,
                SETTLEMENT_PAYMENT_HASH.into(),
                0,
            ));
        }

        let child_reserve = self.auction.info.reserve.coin();
        assert_eq!(child_reserve.parent_coin_info, reserve.coin().coin_id());
        assert_eq!(child_reserve.amount, 0);
        self.assert_unspent(child_reserve);
    }

    fn assert_unspent(&self, coin: Coin) {
        let state = self
            .sim
            .coin_state(coin.coin_id())
            .unwrap_or_else(|| panic!("expected coin {coin:?} to exist"));
        assert!(
            state.spent_height.is_none(),
            "expected coin {coin:?} to be unspent"
        );
    }

    fn assert_missing(&self, coin: Coin) {
        assert!(
            self.sim.coin_state(coin.coin_id()).is_none(),
            "expected coin {coin:?} not to exist"
        );
    }
}

/// The simulator only enforces consensus rules, which are more lenient than the mempool's.
fn assert_mempool_valid(coin_spends: &[CoinSpend]) -> Result<()> {
    let mut allocator = Allocator::new();
    let spend_bundle = SpendBundle::new(coin_spends.to_vec(), Signature::default());
    get_conditions_from_spendbundle(
        &mut allocator,
        &spend_bundle,
        TEST_CONSTANTS.max_block_cost_clvm,
        u32::MAX,
        &TEST_CONSTANTS,
    )
    .map_err(|error| anyhow!("mempool validation failed: {error:?}"))?;
    Ok(())
}

/// The royalty info a wallet would fetch from the NFT on the blockchain.
fn nft_royalty(nft: &Nft) -> RoyaltyInfo {
    RoyaltyInfo::new(
        nft.info.launcher_id,
        nft.info.royalty_puzzle_hash,
        nft.info.royalty_basis_points,
    )
}

fn fee(amount: u64, bps: u64) -> u64 {
    calculate_auction_payment(amount, bps).unwrap()
}

/// Asserts that `result` failed, and that the error message contains `expected`.
fn assert_fails_with<T: Debug>(result: Result<T>, expected: &str) {
    match result {
        Ok(value) => panic!("expected an error containing {expected:?}, got {value:?}"),
        Err(error) => {
            let message = format!("{error:?}");
            assert!(
                message.contains(expected),
                "expected an error containing {expected:?}, got {message}"
            );
        }
    }
}

/// Authorizes a cancel from a new seller coin, and returns the cancel action spend along with the
/// key that must sign the authorization.
fn authorized_cancel(h: &mut Harness) -> Result<(Spend, SecretKey)> {
    let authorizer = h.seller_coin();
    let conditions = h.auction.cancel_authorization(&mut h.ctx)?;
    StandardLayer::new(authorizer.pk).spend(&mut h.ctx, authorizer.coin, conditions)?;
    let cancel = h.auction.cancel_action_spend(&mut h.ctx, &h.nft)?;
    Ok((cancel, authorizer.sk))
}

fn launch_error(config: Config, settings: SettingsFn) -> DriverError {
    match Harness::with_settings(config, settings) {
        Ok(_) => panic!("expected the auction to be rejected"),
        Err(error) => error
            .downcast::<DriverError>()
            .expect("expected a driver error"),
    }
}

fn launcher_spend(h: &Harness, launcher_id: Bytes32) -> Result<CoinSpend> {
    h.sim
        .coin_spend(launcher_id)
        .ok_or(anyhow!("missing launcher spend"))
}

/// Parses the auction launcher with a modified launcher solution, and the given royalty.
fn parse_modified(
    h: &mut Harness,
    royalty: RoyaltyInfo,
    modify: impl FnOnce(&mut LauncherSolution<AuctionMemo>),
) -> Result<Auction, DriverError> {
    let spend = launcher_spend(h, h.auction.info.launcher_id).unwrap();
    let solution = spend.solution.to_clvm(&mut h.ctx)?;
    let mut solution = h.ctx.extract::<LauncherSolution<AuctionMemo>>(solution)?;
    modify(&mut solution);
    let solution = h.ctx.alloc(&solution)?;
    AuctionLaunch::parse(&h.ctx, spend.coin, solution)
        .expect("expected an auction launch")
        .into_auction(royalty)
}

#[test]
fn test_full_auction_pays_everyone() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let first = h.bid(100)?;
    assert_eq!(first.reserve_amount, 100 + 3 + 5);
    assert_eq!(h.auction.info.reserve.coin().amount, first.reserve_amount);

    let outbid_reserve = h.auction.info.reserve;
    let second = h.bid(200)?;
    assert_eq!(second.reserve_amount, 200 + 6 + 10);
    h.assert_unspent(Harness::reserve_output(
        outbid_reserve,
        first.bid.puzzle_hash,
        first.reserve_amount,
    ));

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let nft = h.end()?;

    h.assert_unspent(Harness::reserve_output(
        final_reserve,
        PREMIUM_PUZZLE_HASH,
        6,
    ));
    h.assert_unspent(Harness::reserve_output(
        final_reserve,
        COMMISSION_PUZZLE_HASH,
        2,
    ));
    h.assert_unspent(Harness::reserve_output(
        final_reserve,
        h.seller.puzzle_hash,
        198,
    ));
    h.assert_unspent(Harness::royalty_output(final_reserve, 10));
    h.assert_unspent(h.auction.info.reserve.coin());
    assert_eq!(h.auction.info.reserve.coin().amount, 0);

    h.assert_unspent(nft.coin);
    assert_eq!(nft.info.p2_puzzle_hash, second.bid.puzzle_hash);
    assert_eq!(nft.info.current_owner, None);

    assert!(h.auction.info.state.ended);
    assert_eq!(h.auction.info.state.winning_bid, second.bid);

    Ok(())
}

#[test]
fn test_percent_bid_verifier() -> Result<()> {
    let mut h = Harness::new(Config {
        bid_verifier: percent(1_000, 1_000),
        ..Config::default()
    })?;

    assert_fails_with(h.bid(999), "raise");
    h.bid(1_000)?;
    assert_fails_with(h.bid(1_099), "raise");
    h.bid(1_100)?;
    assert_fails_with(h.bid(1_209), "raise");
    let winner = h.bid(1_210)?;

    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[rstest]
fn test_no_bids_returns_nft_to_seller(
    #[values(0, 500)] royalty_bps: u16,
    #[values(ReserveKind::Xch, ReserveKind::Cat)] reserve: ReserveKind,
) -> Result<()> {
    let mut h = Harness::new(Config {
        royalty_bps,
        reserve,
        ..Config::default()
    })?;

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let nft = h.end()?;

    h.assert_unspent(nft.coin);
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);
    assert_eq!(nft.info.current_owner, None);
    h.assert_missing(Harness::reserve_output(
        final_reserve,
        h.seller.puzzle_hash,
        0,
    ));

    Ok(())
}

#[test]
fn test_zero_fee_auction() -> Result<()> {
    let mut h = Harness::new(Config {
        buyers_premium_bps: 0,
        commission_bps: 0,
        royalty_bps: 0,
        ..Config::default()
    })?;

    let winner = h.bid(100)?;
    assert_eq!(winner.reserve_amount, 100);

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let nft = h.end()?;

    h.assert_unspent(Harness::reserve_output(
        final_reserve,
        h.seller.puzzle_hash,
        100,
    ));
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn test_commission_of_the_entire_bid() -> Result<()> {
    let mut h = Harness::new(Config {
        commission_bps: 10_000,
        ..Config::default()
    })?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let _ = h.end()?;

    h.assert_unspent(Harness::reserve_output(
        final_reserve,
        COMMISSION_PUZZLE_HASH,
        100,
    ));
    h.assert_missing(Harness::reserve_output(
        final_reserve,
        h.seller.puzzle_hash,
        0,
    ));

    Ok(())
}

#[test]
fn test_multiple_bids_in_one_spend() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let first = h.bid(100)?;
    let reserve_before = h.auction.info.reserve;
    let bidders = h.bids(&[(200, false), (300, false)])?;

    h.assert_unspent(Harness::reserve_output(
        reserve_before,
        first.bid.puzzle_hash,
        first.reserve_amount,
    ));
    h.assert_unspent(Harness::reserve_output(
        reserve_before,
        bidders[0].bid.puzzle_hash,
        bidders[0].reserve_amount,
    ));
    assert_eq!(h.auction.info.state.winning_bid, bidders[1].bid);
    assert_eq!(
        h.auction.info.reserve.coin().amount,
        bidders[1].reserve_amount
    );

    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, bidders[1].bid.puzzle_hash);

    Ok(())
}

#[test]
fn test_nft_amount_must_match() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let winner = h.bid(100)?;
    h.set_time(END_TIME)?;

    // Claim the 1 mojo NFT is worth 3 mojos in both the end action and the unlock
    let wrong_amount = 3u64;
    let ctx = &mut h.ctx;

    let end_puzzle = h.auction.info.construct_end_action(ctx)?;
    let end_solution = ctx.alloc(&clvm_list!(wrong_amount))?;
    let end_action = Spend::new(end_puzzle, end_solution);

    let unlocker = h.auction.info.construct_nft_unlocker(ctx)?;
    let unlocker_solution =
        ctx.alloc(&AuctionNftUnlockerSolution::new(winner.bid, wrong_amount))?;
    let conditions = ctx.run(unlocker, unlocker_solution)?;
    let (assert_my_amount, _) = ctx.extract::<(AssertMyAmount, NodePtr)>(conditions)?;
    assert_eq!(assert_my_amount, AssertMyAmount::new(wrong_amount));
    let delegated_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
    let unlock = spend_auction_lock(
        ctx,
        h.auction.info.launcher_id,
        h.auction.info.inner_puzzle_hash(),
        Spend::new(delegated_puzzle, NodePtr::NIL),
    )?;
    let _ = h.nft.spend(ctx, unlock)?;

    let _ = h.auction.spend(ctx, vec![end_action], vec![])?;
    // The NFT puzzle itself also rejects a CREATE_COIN of 3, so this fails before AssertMyAmount
    // is checked, but the unlocker must not rely on that
    assert!(h.sim.spend_coins(ctx.take(), &[]).is_err());

    let nft = h.end()?;
    assert_eq!(nft.coin.amount, 1);
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[rstest]
fn test_nft_with_did_owner_is_cleared(#[values(0, 500)] royalty_bps: u16) -> Result<()> {
    let mut h = Harness::new(Config {
        royalty_bps,
        did_owner: true,
        ..Config::default()
    })?;
    assert!(h.nft.info.current_owner.is_some());

    let winner = h.bid(100)?;
    h.set_time(END_TIME)?;
    let nft = h.end()?;

    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);
    assert_eq!(nft.info.current_owner, None);

    Ok(())
}

#[test]
fn test_minimum_bid_enforced() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    assert_fails_with(h.bid(99), "raise");
    h.bid(100)?;

    Ok(())
}

#[test]
fn test_flat_increment_enforced() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    h.bid(100)?;
    assert_fails_with(h.bid(199), "raise");
    h.bid(200)?;

    Ok(())
}

#[test]
fn test_bids_must_strictly_increase() -> Result<()> {
    // A 1% increment on a bid of 50 rounds down to 0, so the verifier alone would allow a tie
    let mut h = Harness::new(Config {
        bid_verifier: percent(1, 100),
        ..Config::default()
    })?;

    h.bid(50)?;
    assert_fails_with(h.bid(50), "raise");
    h.bid(51)?;

    Ok(())
}

#[test]
fn test_outbid_bidder_is_refunded_in_full() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let first = h.bid(1_000)?;
    let reserve = h.auction.info.reserve;
    h.bid(2_000)?;

    let refund = Harness::reserve_output(reserve, first.bid.puzzle_hash, first.reserve_amount);
    h.assert_unspent(refund);
    assert_eq!(refund.amount, 1_000 + 30 + 50);

    Ok(())
}

#[test]
fn test_malformed_bids_rejected() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let ctx = &mut h.ctx;

    let puzzle = h.auction.info.construct_bid_action(ctx)?;
    let truth = ctx.alloc(&(NodePtr::NIL, h.auction.info.state))?;

    let valid = ctx.alloc(&(100u64, Bytes32::new([5; 32])))?;
    let short_puzzle_hash = ctx.alloc(&(100u64, Bytes::new(vec![5; 31])))?;
    let long_puzzle_hash = ctx.alloc(&(100u64, Bytes::new(vec![5; 33])))?;
    let pair_puzzle_hash = ctx.alloc(&(100u64, (Bytes32::new([5; 32]), ())))?;
    let zero_amount = ctx.alloc(&(0u64, Bytes32::new([5; 32])))?;
    let negative_amount = ctx.alloc(&(-100i64, Bytes32::new([5; 32])))?;
    let pair_amount = ctx.alloc(&((100u64, ()), Bytes32::new([5; 32])))?;

    let valid_solution = ctx.alloc(&(truth, (valid, (false, ()))))?;
    assert!(ctx.run(puzzle, valid_solution).is_ok());

    for bid in [
        short_puzzle_hash,
        long_puzzle_hash,
        pair_puzzle_hash,
        zero_amount,
        negative_amount,
        pair_amount,
    ] {
        let solution = ctx.alloc(&(truth, (bid, (false, ()))))?;
        assert!(ctx.run(puzzle, solution).is_err());

        // The driver runs the same puzzle, so it can't build a spend with a malformed bid either
        let action_solution = ctx.alloc(&(bid, (false, ())))?;
        let result = h
            .auction
            .child_state(ctx, &[Spend::new(puzzle, action_solution)]);
        assert!(result.is_err());
    }

    Ok(())
}

#[test]
fn test_non_canonical_bid_amount_is_normalized() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let reserve_amount = h.auction.info.reserve_amount_for_bid(100).unwrap();
    let bidder = h.sim.bls(reserve_amount);

    // 100 with a redundant leading zero byte
    let amount = h.ctx.new_atom(&[0x00, 0x64])?;
    let puzzle_hash = h.ctx.alloc(&bidder.puzzle_hash)?;
    let raw_bid = h.ctx.new_pair(amount, puzzle_hash)?;

    let puzzle = h.auction.info.construct_bid_action(&mut h.ctx)?;
    let solution = h.ctx.alloc(&(raw_bid, (false, ())))?;

    // The announcement is for the canonical bid
    let bid = AuctionBid::new(100, bidder.puzzle_hash);
    StandardLayer::new(bidder.pk).spend(
        &mut h.ctx,
        bidder.coin,
        Conditions::new()
            .assert_coin_announcement(announcement_id(h.auction.coin.coin_id(), bid.tree_hash())),
    )?;

    let auction = h
        .auction
        .spend(&mut h.ctx, vec![Spend::new(puzzle, solution)], vec![])?;
    h.sim.spend_coins(h.ctx.take(), &[bidder.sk])?;
    h.assert_unspent(auction.coin);
    assert_eq!(auction.info.state.winning_bid, bid);
    h.auction = auction;

    // The driver can still follow the auction
    let reserve = h.auction.info.reserve;
    let winner = h.bid(200)?;
    h.assert_unspent(Harness::reserve_output(
        reserve,
        bidder.puzzle_hash,
        reserve_amount,
    ));

    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn test_bid_announcement_commits_to_the_bid() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let reserve_amount = h.auction.info.reserve_amount_for_bid(100).unwrap();
    let bidder = h.sim.bls(reserve_amount);
    let bid = AuctionBid::new(100, bidder.puzzle_hash);
    let action = h.auction.bid_action_spend(&mut h.ctx, bid, false)?;

    let other_bid = AuctionBid::new(101, bidder.puzzle_hash);
    StandardLayer::new(bidder.pk).spend(
        &mut h.ctx,
        bidder.coin,
        Conditions::new().assert_coin_announcement(announcement_id(
            h.auction.coin.coin_id(),
            other_bid.tree_hash(),
        )),
    )?;

    let _ = h.auction.spend(&mut h.ctx, vec![action], vec![])?;
    let result = h.sim.spend_coins(h.ctx.take(), &[bidder.sk]);
    assert_fails_with(
        result.map_err(anyhow::Error::from),
        "AssertCoinAnnouncementFailed",
    );

    Ok(())
}

#[rstest]
fn test_cat_auction_pays_everyone(
    #[values(ReserveKind::Cat, ReserveKind::RevocableCat)] reserve: ReserveKind,
) -> Result<()> {
    let mut h = Harness::new(Config {
        reserve,
        ..Config::default()
    })?;

    // Amounts where every fee rounds down
    let first = h.bid(150)?;
    assert_eq!(first.reserve_amount, 150 + 4 + 7);

    let outbid_reserve = h.auction.info.reserve;
    let second = h.bid(250)?;
    assert_eq!(second.reserve_amount, 250 + 7 + 12);
    h.assert_unspent(Harness::reserve_output(
        outbid_reserve,
        first.bid.puzzle_hash,
        first.reserve_amount,
    ));

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let nft = h.end()?;

    h.assert_settled(final_reserve, 250);
    assert_eq!(nft.info.p2_puzzle_hash, second.bid.puzzle_hash);

    Ok(())
}

#[rstest]
fn test_cat_auction_with_batched_bids(
    #[values(ReserveKind::Cat, ReserveKind::RevocableCat)] reserve: ReserveKind,
) -> Result<()> {
    let mut h = Harness::new(Config {
        reserve,
        ..Config::default()
    })?;

    let reserve_before = h.auction.info.reserve;
    let bidders = h.bids(&[(100, false), (200, false), (300, false)])?;

    for outbid in &bidders[..2] {
        h.assert_unspent(Harness::reserve_output(
            reserve_before,
            outbid.bid.puzzle_hash,
            outbid.reserve_amount,
        ));
    }

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let nft = h.end()?;

    h.assert_settled(final_reserve, 300);
    assert_eq!(nft.info.p2_puzzle_hash, bidders[2].bid.puzzle_hash);

    Ok(())
}

#[rstest]
fn test_cat_auction_without_fees(
    #[values(ReserveKind::Cat, ReserveKind::RevocableCat)] reserve: ReserveKind,
) -> Result<()> {
    let mut h = Harness::new(Config {
        reserve,
        buyers_premium_bps: 0,
        commission_bps: 0,
        royalty_bps: 0,
        ..Config::default()
    })?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let _ = h.end()?;

    h.assert_settled(final_reserve, 100);

    Ok(())
}

#[rstest]
fn test_cancel_returns_nft_to_seller(
    #[values(ReserveKind::Xch, ReserveKind::Cat, ReserveKind::RevocableCat)] reserve: ReserveKind,
) -> Result<()> {
    let mut h = Harness::new(Config {
        reserve,
        ..Config::default()
    })?;

    let reserve_before = h.auction.info.reserve;
    let nft = h.cancel()?;

    h.assert_unspent(nft.coin);
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);
    assert_eq!(nft.info.current_owner, None);

    assert!(h.auction.info.state.ended);
    assert_eq!(h.auction.info.state.reserve_amount, 0);
    assert_eq!(
        h.auction.info.state.winning_bid,
        AuctionBid::new(0, h.seller.puzzle_hash)
    );

    // Nothing is paid out, and the reserve is recreated empty
    h.assert_missing(Harness::reserve_output(
        reserve_before,
        h.seller.puzzle_hash,
        0,
    ));
    let child_reserve = h.auction.info.reserve.coin();
    assert_eq!(
        child_reserve.parent_coin_info,
        reserve_before.coin().coin_id()
    );
    assert_eq!(child_reserve.amount, 0);
    h.assert_unspent(child_reserve);

    // A cancelled auction can't be bid on, ended, or cancelled again
    assert_fails_with(h.bid(100), "raise");
    h.set_time(END_TIME)?;
    assert_fails_with(h.end(), "raise");
    assert_fails_with(h.cancel(), "raise");

    Ok(())
}

#[test]
fn test_cancel_requires_the_sellers_message() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    assert_fails_with(h.cancel_authorized_by(None), "MessageNotSentOrReceived");

    // A message from any puzzle other than the payout puzzle doesn't authorize the cancel
    let stranger = h.sim.bls(0);
    assert_fails_with(
        h.cancel_authorized_by(Some(stranger)),
        "MessageNotSentOrReceived",
    );

    // The auction is untouched, so the seller can still cancel it
    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    Ok(())
}

#[test]
fn test_cannot_cancel_after_a_bid() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let winner = h.bid(100)?;

    assert_fails_with(h.cancel(), "raise");

    // The bid is still live and wins the auction
    h.set_time(END_TIME)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn test_cannot_cancel_after_the_auction_ended() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    h.set_time(END_TIME)?;
    let nft = h.end()?;

    assert_fails_with(h.cancel(), "raise");
    h.assert_unspent(nft.coin);

    Ok(())
}

#[test]
fn test_cancel_has_no_timing_restrictions() -> Result<()> {
    // Well before the end time, and with a grace period that would otherwise delay the end
    let mut h = Harness::new(Config {
        timings: AuctionTimings::new(END_TIME, 500),
        ..Config::default()
    })?;
    assert!(h.sim.next_timestamp() < END_TIME);

    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    Ok(())
}

#[rstest]
fn test_parse_child_follows_a_cancel(
    #[values(ReserveKind::Xch, ReserveKind::Cat, ReserveKind::RevocableCat)] reserve: ReserveKind,
) -> Result<()> {
    let mut h = Harness::new(Config {
        reserve,
        ..Config::default()
    })?;

    let before = h.auction;
    let _ = h.cancel()?;

    // The harness already compares the parsed child to the spent one, but check it explicitly
    let coin_spend = h
        .sim
        .coin_spend(before.coin.coin_id())
        .ok_or(anyhow!("missing auction spend"))?;
    let solution = coin_spend.solution.to_clvm(&mut h.ctx)?;
    let parsed = before.parse_child(&mut h.ctx, solution)?;

    assert_eq!(parsed, h.auction);
    assert!(parsed.info.state.ended);
    assert_eq!(parsed.info.state.reserve_amount, 0);
    h.assert_unspent(parsed.coin);
    h.assert_unspent(parsed.info.reserve.coin());

    Ok(())
}

#[test]
fn test_cancel_rejects_other_authorizations() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let auction_coin = h.auction.coin;

    let wrong_authorizations: [fn(&mut Harness, Coin) -> Result<Conditions>; 4] = [
        // Sent to the auction's puzzle hash rather than its coin
        |h, auction_coin| {
            let receiver = h.ctx.alloc(&auction_coin.puzzle_hash)?;
            Ok(Conditions::new().send_message(
                0b010_010,
                AUCTION_CANCEL_MESSAGE.to_vec().into(),
                vec![receiver],
            ))
        },
        // Sent to a different coin
        |h, _| {
            let receiver = h.ctx.alloc(&h.nft.coin.coin_id())?;
            Ok(Conditions::new().send_message(
                AUCTION_CANCEL_MESSAGE_MODE,
                AUCTION_CANCEL_MESSAGE.to_vec().into(),
                vec![receiver],
            ))
        },
        // Committing to the sender's coin rather than its puzzle
        |h, auction_coin| {
            let receiver = h.ctx.alloc(&auction_coin.coin_id())?;
            Ok(Conditions::new().send_message(
                0b111_111,
                AUCTION_CANCEL_MESSAGE.to_vec().into(),
                vec![receiver],
            ))
        },
        // The wrong message
        |h, auction_coin| {
            let receiver = h.ctx.alloc(&auction_coin.coin_id())?;
            Ok(Conditions::new().send_message(
                AUCTION_CANCEL_MESSAGE_MODE,
                b"cancel".to_vec().into(),
                vec![receiver],
            ))
        },
    ];

    for wrong_authorization in wrong_authorizations {
        let authorizer = h.seller_coin();
        let conditions = wrong_authorization(&mut h, auction_coin)?;
        assert_fails_with(
            h.cancel_with_authorization(Some((authorizer, conditions))),
            "MessageNotSentOrReceived",
        );
    }

    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    Ok(())
}

#[test]
fn test_unchecked_cancel_without_bids_succeeds() -> Result<()> {
    // Makes sure that the unchecked spends below are only rejected because of the cancel action
    let mut h = Harness::new(Config::default())?;
    let (cancel, key) = authorized_cancel(&mut h)?;
    let nft = h.auction.unlock_nft(&mut h.ctx, &h.nft)?;
    h.spend_unchecked(vec![cancel], 0, &[key])?;

    h.assert_unspent(nft.coin);
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);
    assert!(h.auction.info.state.ended);

    Ok(())
}

#[rstest]
fn test_cancel_after_a_bid_is_rejected_on_chain(#[values(0, 500)] royalty_bps: u16) -> Result<()> {
    // Without a royalty, the NFT's transfer doesn't assert a royalty payment, so nothing but the
    // cancel action's own check stops a cancel from sending the NFT to the bidder and burning the
    // reserve as fees
    let mut h = Harness::new(Config {
        royalty_bps,
        ..Config::default()
    })?;
    let winner = h.bid(100)?;

    let (cancel, key) = authorized_cancel(&mut h)?;
    let _ = h.auction.unlock_nft(&mut h.ctx, &h.nft)?;
    let result = h.spend_unchecked(vec![cancel], 0, &[key]);
    assert_fails_with(result, PUZZLE_RAISED);

    assert_eq!(h.auction.info.state.winning_bid, winner.bid);
    h.assert_unspent(h.auction.info.reserve.coin());

    Ok(())
}

#[test]
fn test_cancel_after_the_end_is_rejected_on_chain() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    h.set_time(END_TIME)?;
    let _ = h.end()?;

    // The NFT has already been unlocked, so only the cancel is attempted
    let (cancel, key) = authorized_cancel(&mut h)?;
    let result = h.spend_unchecked(vec![cancel], 0, &[key]);
    assert_fails_with(result, PUZZLE_RAISED);

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OtherAction {
    Bid,
    End,
}

#[rstest]
fn test_cancel_cannot_be_combined_with_other_actions(
    #[values(OtherAction::Bid, OtherAction::End)] other: OtherAction,
    #[values(false, true)] cancel_first: bool,
) -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    // The bid is never funded, since the spend is rejected before its announcement matters
    let other_spend = match other {
        OtherAction::Bid => {
            let bid = AuctionBid::new(100, Bytes32::new([5; 32]));
            h.auction.bid_action_spend(&mut h.ctx, bid, false)?
        }
        OtherAction::End => {
            h.set_time(END_TIME)?;
            h.auction.end_action_spend(&mut h.ctx, &h.nft)?
        }
    };

    let (cancel, key) = authorized_cancel(&mut h)?;
    let _ = h.auction.unlock_nft(&mut h.ctx, &h.nft)?;

    let actions = if cancel_first {
        vec![cancel, other_spend]
    } else {
        vec![other_spend, cancel]
    };
    let result = h.spend_unchecked(actions, 0, &[key]);
    assert_fails_with(result, PUZZLE_RAISED);

    Ok(())
}

#[test]
fn test_two_cancels_in_one_spend_are_rejected() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    // Each cancel has its own authorization, so only the second cancel's check can reject it
    let (first, first_key) = authorized_cancel(&mut h)?;
    let (second, second_key) = authorized_cancel(&mut h)?;
    let _ = h.auction.unlock_nft(&mut h.ctx, &h.nft)?;
    let result = h.spend_unchecked(vec![first, second], 0, &[first_key, second_key]);
    assert_fails_with(result, PUZZLE_RAISED);

    Ok(())
}

#[rstest]
fn test_cancel_can_spend_a_stand_in_reserve(
    #[values(ReserveKind::Xch, ReserveKind::Cat, ReserveKind::RevocableCat)] reserve: ReserveKind,
) -> Result<()> {
    // If the launcher's reserve is missing, the seller can create an empty coin locked by the
    // auction and spend it as the reserve instead. For CAT reserves, this needs a coin of the same
    // CAT.
    let mut h = Harness::new(Config {
        reserve,
        ..Config::default()
    })?;

    let tracked = h.auction.info.reserve;
    let stand_in = h.decoy_reserve()?;
    h.auction.info.reserve = stand_in;

    // The harness checks that parse_child follows the stand-in reserve
    let nft = h.cancel()?;
    assert_eq!(nft.info.p2_puzzle_hash, h.seller.puzzle_hash);

    h.assert_unspent(tracked.coin());
    let child_reserve = h.auction.info.reserve.coin();
    assert_eq!(child_reserve.parent_coin_info, stand_in.coin().coin_id());
    h.assert_unspent(child_reserve);

    Ok(())
}

#[rstest]
fn test_parses_every_reserve_kind(
    #[values(ReserveKind::Xch, ReserveKind::Cat, ReserveKind::RevocableCat)] reserve: ReserveKind,
) -> Result<()> {
    // The harness parses every launch and spend, so this just covers each reserve kind end to end
    let mut h = Harness::new(Config {
        reserve,
        ..Config::default()
    })?;
    h.bids(&[(100, false), (200, false)])?;
    h.set_time(END_TIME)?;
    let _ = h.end()?;

    Ok(())
}

#[test]
fn test_parse_ignores_other_launchers() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let spend = launcher_spend(&h, h.nft.info.launcher_id)?;
    let solution = spend.solution.to_clvm(&mut h.ctx)?;
    assert_eq!(AuctionLaunch::parse(&h.ctx, spend.coin, solution), None);

    // The right memo, but not spent by a launcher
    let spend = launcher_spend(&h, h.auction.info.launcher_id)?;
    let solution = spend.solution.to_clvm(&mut h.ctx)?;
    let not_a_launcher = Coin::new(
        spend.coin.parent_coin_info,
        h.seller.puzzle_hash,
        spend.coin.amount,
    );
    assert_eq!(AuctionLaunch::parse(&h.ctx, not_a_launcher, solution), None);

    Ok(())
}

#[test]
fn test_parse_rejects_memos_that_dont_match_the_singleton() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let royalty = nft_royalty(&h.nft);

    let result = parse_modified(&mut h, royalty, |solution| {
        solution.key_value_list.settings.timings.end_time += 1;
    });
    assert!(matches!(
        result,
        Err(DriverError::AuctionPuzzleHashMismatch)
    ));

    let result = parse_modified(&mut h, royalty, |solution| {
        solution.key_value_list.nft_coin_id = Bytes32::new([7; 32]);
    });
    assert!(matches!(
        result,
        Err(DriverError::AuctionPuzzleHashMismatch)
    ));

    Ok(())
}

#[test]
fn test_parse_rejects_royalties_that_dont_match_the_singleton() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    // The royalty basis points are curried into the auction, so an NFT with a different royalty
    // than the one the auction was launched with is rejected
    let mut royalty = nft_royalty(&h.nft);
    royalty.basis_points += 1;
    let result = parse_modified(&mut h, royalty, |_| {});
    assert!(matches!(
        result,
        Err(DriverError::AuctionPuzzleHashMismatch)
    ));

    Ok(())
}

#[test]
fn test_parse_rejects_singleton_amounts_other_than_1() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let royalty = nft_royalty(&h.nft);

    for amount in [2, 3] {
        let result = parse_modified(&mut h, royalty, |solution| solution.amount = amount);
        assert!(
            matches!(result, Err(DriverError::InvalidAuctionSingletonAmount(a)) if a == amount),
            "{result:?}"
        );
    }

    Ok(())
}

#[test]
fn test_parse_rejects_invalid_auctions() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    let royalty = nft_royalty(&h.nft);

    // Even if the singleton was launched with these settings, the auction is invalid
    let result = parse_modified(&mut h, royalty, |solution| {
        solution
            .key_value_list
            .settings
            .payments
            .commission
            .puzzle_hash = PREMIUM_PUZZLE_HASH;
    });
    assert!(matches!(
        result,
        Err(DriverError::DuplicateAuctionPaymentPuzzleHash(_))
    ));

    Ok(())
}

#[rstest]
fn test_parse_follows_the_reserve_spent_by_someone_else(
    #[values(ReserveKind::Xch, ReserveKind::Cat, ReserveKind::RevocableCat)] reserve: ReserveKind,
) -> Result<()> {
    let mut h = Harness::new(Config {
        reserve,
        ..Config::default()
    })?;
    let tracked = h.auction;

    // Someone else bids while spending a decoy as the reserve, so the tracked reserve is left
    // behind
    h.auction.info.reserve = h.decoy_reserve()?;
    let _ = h.bid(100)?;
    h.assert_unspent(tracked.info.reserve.coin());

    let spend = h
        .sim
        .coin_spend(tracked.coin.coin_id())
        .ok_or(anyhow!("missing auction spend"))?;
    let solution = spend.solution.to_clvm(&mut h.ctx)?;
    assert_eq!(tracked.parse_child(&mut h.ctx, solution)?, h.auction);

    // The auction continues from the decoy's lineage
    let _ = h.bid(200)?;
    h.set_time(END_TIME)?;
    let _ = h.end()?;

    Ok(())
}

#[test]
fn test_rejects_fees_over_100_percent() {
    let configs = [
        Config {
            buyers_premium_bps: 10_001,
            ..Config::default()
        },
        Config {
            commission_bps: 10_001,
            ..Config::default()
        },
        Config {
            royalty_bps: 10_001,
            ..Config::default()
        },
    ];

    for config in configs {
        let error = launch_error(config, default_settings);
        assert!(
            matches!(error, DriverError::AuctionBpsTooHigh { bps: 10_001, .. }),
            "{error}"
        );
    }
}

#[test]
fn test_allows_fees_of_100_percent() -> Result<()> {
    let mut h = Harness::new(Config {
        buyers_premium_bps: 10_000,
        commission_bps: 10_000,
        royalty_bps: 10_000,
        ..Config::default()
    })?;

    let winner = h.bid(100)?;
    assert_eq!(winner.reserve_amount, 300);

    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let _ = h.end()?;
    h.assert_settled(final_reserve, 100);

    Ok(())
}

#[test]
fn test_rejects_duplicate_payment_puzzle_hashes() {
    let settings: [SettingsFn; 3] = [
        |config, seller, _| {
            let mut settings = config.settings(seller);
            settings.payments.buyers_premium.puzzle_hash = COMMISSION_PUZZLE_HASH;
            settings
        },
        |config, _, _| config.settings(PREMIUM_PUZZLE_HASH),
        |config, _, _| config.settings(COMMISSION_PUZZLE_HASH),
    ];

    for settings in settings {
        let error = launch_error(Config::default(), settings);
        assert!(
            matches!(error, DriverError::DuplicateAuctionPaymentPuzzleHash(_)),
            "{error}"
        );
    }
}

#[test]
fn test_allows_shared_puzzle_hash_for_unpaid_fees() -> Result<()> {
    let config = Config {
        buyers_premium_bps: 0,
        commission_bps: 0,
        ..Config::default()
    };

    let mut h = Harness::with_settings(config, |config, seller, _| {
        let mut settings = config.settings(seller);
        settings.payments.buyers_premium.puzzle_hash = seller;
        settings.payments.commission.puzzle_hash = seller;
        settings
    })?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let final_reserve = h.auction.info.reserve;
    let _ = h.end()?;
    h.assert_unspent(Harness::reserve_output(
        final_reserve,
        h.seller.puzzle_hash,
        100,
    ));

    Ok(())
}

#[test]
fn test_rejects_reserved_payment_puzzle_hashes() {
    let error = launch_error(Config::default(), |config, _, _| {
        config.settings(SETTLEMENT_PAYMENT_HASH.into())
    });
    assert!(
        matches!(error, DriverError::ReservedAuctionPaymentPuzzleHash(_)),
        "{error}"
    );

    let error = launch_error(Config::default(), |config, _, lock_puzzle_hash| {
        config.settings(lock_puzzle_hash)
    });
    assert!(
        matches!(error, DriverError::ReservedAuctionPaymentPuzzleHash(_)),
        "{error}"
    );

    let error = launch_error(
        Config {
            reserve: ReserveKind::Cat,
            ..Config::default()
        },
        |config, seller, lock_puzzle_hash| {
            let mut settings = config.settings(seller);
            settings.payments.commission.puzzle_hash = lock_puzzle_hash;
            settings
        },
    );
    assert!(
        matches!(error, DriverError::ReservedAuctionPaymentPuzzleHash(_)),
        "{error}"
    );
}

#[test]
fn test_launch_rejects_singleton_amounts_other_than_1() -> Result<()> {
    // The reserve finalizer always recreates the singleton with an amount of 1
    let mut h = Harness::new(Config::default())?;
    let launcher = Launcher::new(h.seller.coin.coin_id(), 3);
    let info = h.auction.info;

    let result = launcher.launch_auction(&mut h.ctx, info.settings, info.reserve, &h.nft);
    assert!(
        matches!(result, Err(DriverError::InvalidAuctionSingletonAmount(3))),
        "{result:?}"
    );

    Ok(())
}

#[test]
fn test_bps_math_does_not_overflow() -> Result<()> {
    assert_eq!(
        calculate_auction_payment(u64::MAX, AUCTION_MAX_BPS),
        Some(u64::MAX)
    );
    assert_eq!(
        calculate_auction_payment(u64::MAX, 5_000),
        Some(u64::MAX / 2)
    );
    assert_eq!(
        calculate_auction_payment(u64::MAX, AUCTION_MAX_BPS + 1),
        None
    );
    assert_eq!(calculate_auction_payment(19, 500), Some(0));
    assert_eq!(calculate_auction_payment(20, 500), Some(1));

    let h = Harness::new(Config::default())?;
    assert_eq!(h.auction.info.reserve_amount_for_bid(u64::MAX), None);
    assert_eq!(h.auction.info.reserve_amount_for_bid(100), Some(108));

    Ok(())
}

#[test]
fn test_driver_fee_math_matches_puzzles() -> Result<()> {
    const PAYOUT_PUZZLE_HASH: Bytes32 = Bytes32::new([3; 32]);
    const BIDDER_PUZZLE_HASH: Bytes32 = Bytes32::new([4; 32]);

    let mut rng = ChaCha8Rng::seed_from_u64(1337);

    for _ in 0..256 {
        // Small enough that the reserve amount always fits in a u64
        let amount = rng.random_range(1..=u64::MAX / 3);
        let premium_bps = rng.random_range(0..=AUCTION_MAX_BPS);
        let commission_bps = rng.random_range(0..=AUCTION_MAX_BPS);
        let royalty_bps = rng.random_range(0..=10_000u16);

        let mut ctx = SpendContext::new();

        // An auction that is never launched, since only its puzzles are run
        let launcher_id = Bytes32::new([5; 32]);
        let settings = AuctionSettings::new(
            flat(1, 1),
            AuctionTimings::new(1_000, 0),
            AuctionPayments::new(
                AuctionBpsPayment::new(premium_bps, PREMIUM_PUZZLE_HASH),
                AuctionBpsPayment::new(commission_bps, COMMISSION_PUZZLE_HASH),
                PAYOUT_PUZZLE_HASH,
            ),
        );
        let reserve = Coin::new(
            Bytes32::new([6; 32]),
            auction_lock_p2_puzzle_hash(launcher_id),
            0,
        );
        let info = AuctionInfo::new(
            launcher_id,
            settings,
            Bytes32::new([7; 32]),
            RoyaltyInfo::new(Bytes32::new([8; 32]), Bytes32::new([9; 32]), royalty_bps),
            AuctionState::initial(PAYOUT_PUZZLE_HASH),
            AuctionReserve::Xch(reserve),
        );
        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: Bytes32::default(),
            parent_amount: 1,
        });
        let mut auction = Auction::new(Coin::new(launcher_id, Bytes32::default(), 1), proof, info);

        let bid = AuctionBid::new(amount, BIDDER_PUZZLE_HASH);
        let bid_action = auction.bid_action_spend(&mut ctx, bid, false)?;
        let child = auction.child_state(&mut ctx, &[bid_action])?;

        let reserve_amount = auction.info.reserve_amount_for_bid(amount).unwrap();
        assert_eq!(child.state.reserve_amount, reserve_amount);

        auction.info.state = child.state;
        let end_puzzle = auction.info.construct_end_action(&mut ctx)?;
        let end_solution = ctx.alloc(&clvm_list!(1u64))?;
        let child = auction.child_state(&mut ctx, &[Spend::new(end_puzzle, end_solution)])?;

        let mut payments = HashMap::new();
        for condition in child.reserve_conditions {
            let create_coin = ctx.extract::<CreateCoin<NodePtr>>(condition)?;
            assert!(
                payments
                    .insert(create_coin.puzzle_hash, create_coin.amount)
                    .is_none()
            );
        }

        let premium = fee(amount, premium_bps);
        let commission = fee(amount, commission_bps);
        let royalty = fee(amount, royalty_bps.into());

        let expected = [
            (PREMIUM_PUZZLE_HASH, premium),
            (COMMISSION_PUZZLE_HASH, commission),
            (PAYOUT_PUZZLE_HASH, amount - commission),
            (SETTLEMENT_PAYMENT_HASH.into(), royalty),
        ];

        for (puzzle_hash, expected) in expected {
            assert_eq!(payments.remove(&puzzle_hash).unwrap_or(0), expected);
        }

        // Only the empty reserve for the ended auction is left
        assert_eq!(
            payments.remove(&auction.info.reserve.p2_puzzle_hash()),
            Some(0)
        );
        assert!(payments.is_empty());

        assert_eq!(child.royalty_amount, (royalty > 0).then_some(royalty));
        assert_eq!(premium + royalty + amount, reserve_amount);
    }

    Ok(())
}

const ROYALTY_BIDS: [u64; 9] = [
    1,
    19,
    20,
    21,
    150,
    9_999,
    10_000,
    123_456_789,
    100_000_000_000_000,
];

#[rstest]
#[case(500, ReserveKind::Xch)]
#[case(500, ReserveKind::Cat)]
#[case(500, ReserveKind::RevocableCat)]
#[case(1, ReserveKind::Xch)]
#[case(1, ReserveKind::Cat)]
#[case(10_000, ReserveKind::Xch)]
#[case(10_000, ReserveKind::Cat)]
#[case(0, ReserveKind::Xch)]
#[case(0, ReserveKind::Cat)]
fn test_royalties(#[case] royalty_bps: u16, #[case] reserve: ReserveKind) -> Result<()> {
    for amount in ROYALTY_BIDS {
        let mut h = Harness::new(Config {
            bid_verifier: flat(1, 1),
            royalty_bps,
            reserve,
            ..Config::default()
        })?;

        let winner = h.bid(amount)?;
        assert_eq!(
            winner.reserve_amount,
            amount + fee(amount, 300) + fee(amount, royalty_bps.into())
        );

        h.set_time(END_TIME)?;
        let final_reserve = h.auction.info.reserve;
        let nft = h.end()?;

        h.assert_settled(final_reserve, amount);
        assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);
    }

    Ok(())
}

#[test]
fn test_refund_to_settlement_puzzle_hash_is_not_a_royalty() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    let reserve_amount = h.auction.info.reserve_amount_for_bid(100).unwrap();
    let bidder = h.sim.bls(reserve_amount);
    let bid = AuctionBid::new(100, SETTLEMENT_PAYMENT_HASH.into());
    let bid_action = h.auction.bid_action_spend(&mut h.ctx, bid, false)?;
    StandardLayer::new(bidder.pk).spend(
        &mut h.ctx,
        bidder.coin,
        Conditions::new()
            .assert_coin_announcement(announcement_id(h.auction.coin.coin_id(), bid.tree_hash())),
    )?;
    let auction = h.auction.spend(&mut h.ctx, vec![bid_action], vec![])?;
    h.sim.spend_coins(h.ctx.take(), &[bidder.sk])?;
    h.auction = auction;

    let reserve_before = h.auction.info.reserve;
    h.bid(200)?;

    // The driver must leave the refund alone instead of paying it to the royalty address
    h.assert_unspent(Harness::reserve_output(
        reserve_before,
        SETTLEMENT_PAYMENT_HASH.into(),
        reserve_amount,
    ));

    Ok(())
}

fn with_grace(grace_period: u64) -> Config {
    Config {
        timings: AuctionTimings::new(END_TIME, grace_period),
        ..Config::default()
    }
}

#[test]
fn test_bids_rejected_after_end_time() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    h.set_time(END_TIME - 1)?;
    h.bid(100)?;

    h.set_time(END_TIME)?;
    assert_fails_with(h.bid(200), "AssertBeforeSecondsAbsoluteFailed");

    // Without a grace period, asking for the grace period falls back to the end time
    assert_fails_with(
        h.bid_with_grace(200, true),
        "AssertBeforeSecondsAbsoluteFailed",
    );

    Ok(())
}

#[test]
fn test_end_rejected_before_end_time() -> Result<()> {
    let mut h = Harness::new(Config::default())?;
    h.bid(100)?;

    h.set_time(END_TIME - 1)?;
    assert_fails_with(h.end(), "AssertSecondsAbsoluteFailed");

    h.set_time(END_TIME)?;
    let _ = h.end()?;

    Ok(())
}

#[test]
fn test_grace_period_extends_bidding() -> Result<()> {
    let mut h = Harness::new(with_grace(100))?;

    h.set_time(950)?;
    h.bid(100)?;

    // After the end time, only grace bids within 100 seconds of the last bid are allowed
    h.set_time(1_020)?;
    assert_fails_with(h.bid(200), "AssertBeforeSecondsAbsoluteFailed");
    let winner = h.bid_with_grace(200, true)?;

    // The auction can't end until the grace period after the last bid has passed
    h.set_time(1_100)?;
    assert_fails_with(h.end(), "AssertSecondsRelativeFailed");

    h.set_time(1_120)?;
    let nft = h.end()?;
    assert_eq!(nft.info.p2_puzzle_hash, winner.bid.puzzle_hash);

    Ok(())
}

#[test]
fn test_grace_window_closes() -> Result<()> {
    let mut h = Harness::new(with_grace(100))?;

    h.set_time(900)?;
    h.bid(100)?;

    h.set_time(1_000)?;
    assert_fails_with(
        h.bid_with_grace(200, true),
        "AssertBeforeSecondsRelativeFailed",
    );

    let _ = h.end()?;

    Ok(())
}

#[test]
fn test_bids_rejected_after_the_auction_ends() -> Result<()> {
    let mut h = Harness::new(with_grace(100))?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let _ = h.end()?;
    assert!(h.auction.info.state.ended);

    // The grace window relative to the end spend is still open, so only the ended flag stops this
    h.set_time(END_TIME + 10)?;
    assert_fails_with(h.bid_with_grace(200, true), "raise");

    Ok(())
}

#[test]
fn test_auction_can_only_end_once() -> Result<()> {
    let mut h = Harness::new(Config::default())?;

    h.bid(100)?;
    h.set_time(END_TIME)?;
    let _ = h.end()?;

    let end_action = h.auction.end_action_spend(&mut h.ctx, &h.nft)?;
    let result = h.auction.child_state(&mut h.ctx, &[end_action]);
    assert_fails_with(result.map_err(anyhow::Error::from), "raise");

    Ok(())
}
