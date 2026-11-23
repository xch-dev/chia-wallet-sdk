use std::collections::{HashMap, HashSet};

use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::offer::{NotarizedPayment, Payment};
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use chia_sdk_types::{announcement_id, tree_hash_notarized_payment};
use clvm_traits::clvm_list;
use clvm_utils::ToTreeHash;
use clvmr::Allocator;

use super::vault_transaction::child_coin_ids;
use crate::{
    CatInfo, DriverError, Facts, HashedPtr, NftInfo, Offer, OptionInfo, ParsedAsset, Reveals,
    SingletonInfo, VerifiedSpend,
};

/// A revealed notarized payment of a settlement coin whose announcement is asserted, so the transaction is only
/// valid if some settlement coin of the asset makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertedNotarizedPayment {
    /// The asset being paid.
    pub asset: ClearSigningAsset,
    /// The nonce and payments.
    pub notarized_payment: NotarizedPayment,
}

/// A single payment of an [`AssertedNotarizedPayment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertedPayment {
    /// The asset being paid.
    pub asset: ClearSigningAsset,
    /// The nonce of the notarized payment. For NFT royalties, this is the NFT's launcher id.
    pub nonce: Bytes32,
    /// The puzzle hash, amount, and memos of the payment.
    pub payment: Payment,
    /// The royalty of the NFT, if this is a royalty payment of an NFT that's part of the trade.
    ///
    /// It's set if the nonce is the launcher id of an NFT that a verified spend spends, or whose payment
    /// is asserted along with this one, the NFT has a royalty, the payment is of XCH or a CAT to its royalty puzzle
    /// hash, and the amount is at most the royalty of the trade's value in that asset. The value is what the trade
    /// sends to the settlement puzzle in the asset plus the other payments of it that are asserted along with this
    /// one. Since each royalty is a share of an NFT's trade price, which is at most this value, a larger payment
    /// isn't a royalty. The NFT's royalty puzzle hash and basis points come from its verified spend or from the
    /// puzzle hash its payment asserts, so they can't be made up.
    ///
    /// This is a hint for showing the payment, which is reported either way.
    pub royalty_basis_points: Option<u16>,
    /// Whether the nonce ties the payment to this transaction, because it commits to a coin that only this
    /// transaction can spend.
    ///
    /// A settlement coin's announcement only commits to the nonce and the payments, so if two of the vault's
    /// transactions or linked offers assert the same nonce and payments, one payment can satisfy both. If the nonce
    /// is bound, they would both spend the same coin, so they can't both be confirmed. If it isn't, nothing stops a
    /// payment that the transaction receives from also being claimed by another transaction, so it should be shown
    /// with a warning.
    ///
    /// See [`TradeContext::new`] for which nonces are bound. A payment of a [`LinkedOffer`](crate::LinkedOffer) that
    /// the transaction also asserts isn't bound, since one payment can satisfy both if the pre-split coins are spent
    /// in the same bundle. A royalty payment's nonce is the NFT's launcher id, so it's never bound.
    pub nonce_bound: bool,
}

/// What a transaction backs about the payments it asserts, which [`split_asserted_payments`] checks each payment
/// against.
#[derive(Debug, Default, Clone)]
pub struct TradeContext {
    bound_nonces: HashSet<Bytes32>,
    shared_payments: Vec<AssertedNotarizedPayment>,
    nft_royalties: HashMap<Bytes32, NftRoyalty>,
    settlement_amounts: HashMap<FungibleAsset, u128>,
}

/// Where an NFT's royalty is paid, and how much it is.
#[derive(Debug, Clone, Copy)]
struct NftRoyalty {
    puzzle_hash: Bytes32,
    basis_points: u16,
}

/// An asset that an NFT's trade price, and so its royalty, can be paid in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FungibleAsset {
    Xch,
    Cat {
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
    },
}

impl FungibleAsset {
    fn from_asset(asset: ClearSigningAsset) -> Option<Self> {
        match asset {
            ClearSigningAsset::Xch => Some(Self::Xch),
            ClearSigningAsset::Cat {
                asset_id,
                hidden_puzzle_hash,
            } => Some(Self::Cat {
                asset_id,
                hidden_puzzle_hash,
            }),
            ClearSigningAsset::Nft { .. } => None,
        }
    }
}

impl TradeContext {
    /// Creates the context of the payments asserted by a transaction with the verified spends.
    ///
    /// A nonce is bound if it commits to coins that only this transaction can spend: the coins of the verified spends
    /// that receive a message from the vault, and of the spends of their descendants that can only be spent one way.
    /// Another transaction can't send the same coin a message without making the bundle invalid, and a descendant
    /// can't exist without it. A spend that's only verified because the transaction asserts it isn't included if its
    /// coin can be spent another way (a clawback by its other paths, or a revocable CAT by its hidden puzzle), or
    /// isn't a descendant at all, since any number of transactions confirmed together can assert the same spend, and
    /// one of them could spend the coin the other way. This assumes that the conditions the vault commits to don't
    /// themselves receive another message from it, which would let the coin's one spend take a message from a second
    /// transaction too.
    ///
    /// The nonce can commit to one of these coins, or to all of them that aren't created by another verified spend,
    /// which are the coins an offer made by the transaction offers. It commits to a set of coins if it's
    /// [`Offer::nonce`] of their ids, or the tree hash of the coins sorted by id, each as a
    /// `(parent_coin_info puzzle_hash amount)` list, which is how chia-blockchain and Cloud Wallet notarize an offer's
    /// requested payments. A single coin's id is also a bound nonce.
    ///
    /// The NFTs that the verified spends spend are part of the trade, so their royalties can be paid.
    pub fn new(spends: &[VerifiedSpend]) -> Self {
        let child_coin_ids = child_coin_ids(spends);
        let mut input_coins = Vec::new();
        let mut bound_nonces = HashSet::new();

        for spend in exclusive_spends(spends) {
            let coin = spend.asset.coin();

            if !child_coin_ids.contains(&coin.coin_id()) {
                input_coins.push(coin);
            }

            bound_nonces.insert(coin.coin_id());
            bound_nonces.insert(Offer::nonce(vec![coin.coin_id()]));
            bound_nonces.insert(coin_list_nonce(vec![coin]));
        }

        if !input_coins.is_empty() {
            bound_nonces.insert(Offer::nonce(
                input_coins.iter().map(Coin::coin_id).collect(),
            ));
            bound_nonces.insert(coin_list_nonce(input_coins));
        }

        let mut nft_royalties = HashMap::new();

        // An NFT child is created by a spend of the same NFT, so the spends are enough.
        for spend in spends {
            if let ParsedAsset::Nft(nft) = &spend.asset {
                nft_royalties.insert(
                    nft.info.launcher_id,
                    NftRoyalty {
                        puzzle_hash: nft.info.royalty_puzzle_hash,
                        basis_points: nft.info.royalty_basis_points,
                    },
                );
            }
        }

        Self {
            bound_nonces,
            shared_payments: Vec::new(),
            nft_royalties,
            settlement_amounts: HashMap::new(),
        }
    }

    /// Adds an amount of the asset that the trade sends to the settlement puzzle, which is part of the trade's value
    /// that royalties are checked against. NFTs aren't part of the value, so they're ignored.
    pub fn add_settlement_amount(&mut self, asset: ClearSigningAsset, amount: u64) {
        if let Some(asset) = FungibleAsset::from_asset(asset) {
            *self.settlement_amounts.entry(asset).or_default() += u128::from(amount);
        }
    }

    /// Records a payment that's also asserted somewhere else, so that a payment could satisfy both. Its payments
    /// aren't bound, whatever their nonce.
    pub fn add_shared_payment(&mut self, payment: AssertedNotarizedPayment) {
        self.shared_payments.push(payment);
    }

    fn is_bound(&self, payment: &AssertedNotarizedPayment) -> bool {
        self.bound_nonces.contains(&payment.notarized_payment.nonce)
            && !self.shared_payments.contains(payment)
    }
}

/// The verified spends that only this transaction can spend, in no particular order. These are the spends that receive
/// a message from the vault, and the spends of their descendants that can only happen the way they're shown.
fn exclusive_spends(spends: &[VerifiedSpend]) -> Vec<&VerifiedSpend> {
    let spends_by_coin_id: HashMap<Bytes32, &VerifiedSpend> = spends
        .iter()
        .map(|spend| (spend.asset.coin().coin_id(), spend))
        .collect();

    let mut exclusive_coin_ids = HashSet::new();
    let mut exclusive_spends = Vec::new();
    let mut stack: Vec<&VerifiedSpend> = spends
        .iter()
        .filter(|spend| spend.custody.receives_message())
        .collect();

    while let Some(spend) = stack.pop() {
        if !exclusive_coin_ids.insert(spend.asset.coin().coin_id()) {
            continue;
        }

        exclusive_spends.push(spend);

        for child in &spend.children {
            if let Some(&child_spend) = spends_by_coin_id.get(&child.asset.coin().coin_id())
                && has_one_path(child_spend)
            {
                stack.push(child_spend);
            }
        }
    }

    exclusive_spends
}

/// Whether the coin can only be spent the way the verified spend shows. A spend that receives a message from the vault
/// can't be replaced without leaving the message unreceived. Any other verified spend has conditions fixed by its
/// puzzle, which can only be spent another way if it's a clawback (by its other paths) or a revocable CAT (by its
/// hidden puzzle).
fn has_one_path(spend: &VerifiedSpend) -> bool {
    spend.custody.receives_message()
        || (spend.clawback.is_none()
            && !matches!(&spend.asset, ParsedAsset::Cat(cat) if cat.info.hidden_puzzle_hash.is_some()))
}

/// The nonce that chia-blockchain and Cloud Wallet notarize an offer's requested payments with: the tree hash of the
/// offered coins sorted by id, each as a `(parent_coin_info puzzle_hash amount)` list. Unlike [`Offer::nonce`], it
/// hashes the coins rather than their ids, but it commits to the same coins.
fn coin_list_nonce(mut coins: Vec<Coin>) -> Bytes32 {
    coins.sort_by_key(Coin::coin_id);

    coins
        .iter()
        .map(|coin| clvm_list!(coin.parent_coin_info, coin.puzzle_hash, coin.amount))
        .collect::<Vec<_>>()
        .tree_hash()
        .into()
}

/// Asserted payments, split by whether they're paid to the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitAssertedPayments {
    /// Payments to one of the vault's known p2 puzzle hashes.
    pub received_payments: Vec<AssertedPayment>,
    /// Payments to any other puzzle hash.
    pub external_payments: Vec<AssertedPayment>,
}

/// An asset, as identified for clear signing. Unlike an asset id alone, this includes every detail that affects
/// the asset's puzzle hash, so two assets with the same id but different details are different.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearSigningAsset {
    /// XCH.
    Xch,
    /// A CAT.
    Cat {
        /// The asset id, which is the tree hash of the TAIL.
        asset_id: Bytes32,
        /// The hidden puzzle hash of the revocation layer, or `None` if the CAT isn't revocable. A revocable CAT
        /// can be taken back by its issuer.
        hidden_puzzle_hash: Option<Bytes32>,
    },
    /// An NFT. Its current owner isn't included, since it doesn't control the NFT.
    Nft {
        /// The launcher id.
        launcher_id: Bytes32,
        /// The metadata.
        metadata: HashedPtr,
        /// The puzzle hash of the metadata updater.
        metadata_updater_puzzle_hash: Bytes32,
        /// The puzzle hash that royalties are paid to.
        royalty_puzzle_hash: Bytes32,
        /// The royalty percentage, where 100 basis points is 1%.
        royalty_basis_points: u16,
    },
}

/// Splits each payment of the asserted notarized payments into received payments (to one of the `p2_puzzle_hashes`)
/// and external payments (to anything else), checking them against the [`TradeContext`] of the transaction.
pub fn split_asserted_payments(
    asserted_payments: &[AssertedNotarizedPayment],
    p2_puzzle_hashes: &HashSet<Bytes32>,
    trade: &TradeContext,
) -> SplitAssertedPayments {
    // An NFT whose payment is asserted is also part of the trade. The settlement coin's puzzle hash commits to its
    // royalty, so the payment can only be made by the NFT with that royalty.
    let mut nft_royalties = trade.nft_royalties.clone();

    for asserted_notarized_payment in asserted_payments {
        if let ClearSigningAsset::Nft {
            launcher_id,
            royalty_puzzle_hash,
            royalty_basis_points,
            ..
        } = asserted_notarized_payment.asset
        {
            nft_royalties.entry(launcher_id).or_insert(NftRoyalty {
                puzzle_hash: royalty_puzzle_hash,
                basis_points: royalty_basis_points,
            });
        }
    }

    // The basis points of the royalty, if the payment could be one, before its amount is checked.
    let royalty_candidate =
        |asserted_notarized_payment: &AssertedNotarizedPayment, payment: &Payment| -> Option<u16> {
            let royalty = nft_royalties.get(&asserted_notarized_payment.notarized_payment.nonce)?;

            (FungibleAsset::from_asset(asserted_notarized_payment.asset).is_some()
                && royalty.basis_points > 0
                && payment.puzzle_hash == royalty.puzzle_hash)
                .then_some(royalty.basis_points)
        };

    // The value of the trade in each asset, which is what royalties are a share of.
    let mut trade_values = trade.settlement_amounts.clone();

    for asserted_notarized_payment in asserted_payments {
        let Some(asset) = FungibleAsset::from_asset(asserted_notarized_payment.asset) else {
            continue;
        };

        for payment in &asserted_notarized_payment.notarized_payment.payments {
            if royalty_candidate(asserted_notarized_payment, payment).is_none() {
                *trade_values.entry(asset).or_default() += u128::from(payment.amount);
            }
        }
    }

    let mut received_payments = Vec::new();
    let mut external_payments = Vec::new();

    for asserted_notarized_payment in asserted_payments {
        let nonce_bound = trade.is_bound(asserted_notarized_payment);
        let trade_value = FungibleAsset::from_asset(asserted_notarized_payment.asset)
            .and_then(|asset| trade_values.get(&asset))
            .copied()
            .unwrap_or_default();

        for payment in &asserted_notarized_payment.notarized_payment.payments {
            let royalty_basis_points = royalty_candidate(asserted_notarized_payment, payment)
                .filter(|&basis_points| {
                    u128::from(payment.amount) <= trade_value * u128::from(basis_points) / 10_000
                });

            let asserted_payment = AssertedPayment {
                asset: asserted_notarized_payment.asset,
                nonce: asserted_notarized_payment.notarized_payment.nonce,
                payment: payment.clone(),
                royalty_basis_points,
                nonce_bound,
            };

            if p2_puzzle_hashes.contains(&asserted_payment.payment.puzzle_hash) {
                received_payments.push(asserted_payment);
            } else {
                external_payments.push(asserted_payment);
            }
        }
    }

    SplitAssertedPayments {
        received_payments,
        external_payments,
    }
}

/// The revealed requested payments whose puzzle announcements are asserted in the [`Facts`], deduplicated by
/// announcement id (and for NFTs, by payment, since only one coin of an NFT can exist).
///
/// Requested payments are revealed by settlement spends in the [`Reveals`]. A payment is only included if the
/// announcement of a settlement coin of its asset (XCH, or a CAT or NFT whose details were revealed) is asserted,
/// which means the transaction requires the payment to be made. An NFT payment is matched for each current owner that
/// a settlement spend of the NFT was revealed with, since the settlement coin's puzzle hash commits to it (the owner
/// isn't part of the [`ClearSigningAsset`]).
///
/// # Errors
///
/// Returns [`DriverError::UnsupportedAssertedOptionPayment`] if a payment of an option contract is asserted, since an
/// option contract can't be represented as a [`ClearSigningAsset`].
pub fn parse_asserted_requested_payments(
    reveals: &Reveals,
    facts: &Facts,
    allocator: &Allocator,
) -> Result<Vec<AssertedNotarizedPayment>, DriverError> {
    let mut payments = Vec::new();
    let mut seen_announcements = HashSet::new();

    let requested_payments = reveals.requested_payments();
    let asset_info = reveals.asset_info();

    for notarized_payment in &requested_payments.xch {
        let hash = tree_hash_notarized_payment(allocator, notarized_payment);
        let announcement_id = announcement_id(SETTLEMENT_PAYMENT_HASH.into(), hash);

        let payment = AssertedNotarizedPayment {
            asset: ClearSigningAsset::Xch,
            notarized_payment: notarized_payment.clone(),
        };

        if facts.is_puzzle_announcement_asserted(announcement_id)
            && seen_announcements.insert(announcement_id)
        {
            payments.push(payment);
        }
    }

    for (&asset_id, notarized_payments) in &requested_payments.cats {
        for notarized_payment in notarized_payments {
            let Some(info) = asset_info.cat(asset_id) else {
                continue;
            };

            let hash = tree_hash_notarized_payment(allocator, notarized_payment);
            let puzzle_hash = CatInfo::new(
                asset_id,
                info.hidden_puzzle_hash,
                SETTLEMENT_PAYMENT_HASH.into(),
            )
            .puzzle_hash();
            let announcement_id = announcement_id(puzzle_hash.into(), hash);

            let payment = AssertedNotarizedPayment {
                asset: ClearSigningAsset::Cat {
                    asset_id,
                    hidden_puzzle_hash: info.hidden_puzzle_hash,
                },
                notarized_payment: notarized_payment.clone(),
            };

            if facts.is_puzzle_announcement_asserted(announcement_id)
                && seen_announcements.insert(announcement_id)
            {
                payments.push(payment);
            }
        }
    }

    for (&launcher_id, notarized_payments) in &requested_payments.nfts {
        let Some(info) = asset_info.nft(launcher_id) else {
            continue;
        };

        // The settlement coin's puzzle hash commits to the NFT's current owner, which is whatever the requested
        // payments were revealed with. Each owner is a different settlement coin, whose announcement is checked
        // separately.
        let settlement_puzzle_hashes: Vec<Bytes32> = reveals
            .nft_owners(launcher_id)
            .map(|current_owner| {
                NftInfo::new(
                    launcher_id,
                    info.metadata,
                    info.metadata_updater_puzzle_hash,
                    current_owner,
                    info.royalty_puzzle_hash,
                    info.royalty_basis_points,
                    SETTLEMENT_PAYMENT_HASH.into(),
                )
                .puzzle_hash()
                .into()
            })
            .collect();

        // Only one coin of the NFT can exist, so the same payment can't be made by more than one of its settlement
        // coins, and it's only reported once.
        let mut seen_payments = HashSet::new();

        for notarized_payment in notarized_payments {
            let hash = tree_hash_notarized_payment(allocator, notarized_payment);

            let is_asserted = settlement_puzzle_hashes.iter().any(|&puzzle_hash| {
                let announcement_id = announcement_id(puzzle_hash, hash);
                facts.is_puzzle_announcement_asserted(announcement_id)
            });

            if is_asserted && seen_payments.insert(hash) {
                payments.push(AssertedNotarizedPayment {
                    asset: ClearSigningAsset::Nft {
                        launcher_id,
                        metadata: info.metadata,
                        metadata_updater_puzzle_hash: info.metadata_updater_puzzle_hash,
                        royalty_puzzle_hash: info.royalty_puzzle_hash,
                        royalty_basis_points: info.royalty_basis_points,
                    },
                    notarized_payment: notarized_payment.clone(),
                });
            }
        }
    }

    // An option contract can't be represented as a `ClearSigningAsset`, so an asserted payment of one can't be
    // reported. It's an error rather than being left out, since the transaction requires the payment to be made.
    for (&launcher_id, notarized_payments) in &requested_payments.options {
        let Some(info) = asset_info.option(launcher_id) else {
            continue;
        };

        let puzzle_hash = OptionInfo::new(
            launcher_id,
            info.underlying_coin_id,
            info.underlying_delegated_puzzle_hash,
            SETTLEMENT_PAYMENT_HASH.into(),
        )
        .puzzle_hash();

        for notarized_payment in notarized_payments {
            let hash = tree_hash_notarized_payment(allocator, notarized_payment);
            let announcement_id = announcement_id(puzzle_hash.into(), hash);

            if facts.is_puzzle_announcement_asserted(announcement_id) {
                return Err(DriverError::UnsupportedAssertedOptionPayment);
            }
        }
    }

    Ok(payments)
}
