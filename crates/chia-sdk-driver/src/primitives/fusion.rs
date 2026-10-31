use std::collections::HashMap;

use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    EveProof, LineageProof, Memos, Proof,
    nft::NftRoyaltyTransferPuzzleArgs,
    offer::{NotarizedPayment, Payment},
    singleton::SingletonSolution,
};
use chia_puzzles::{
    NFT_METADATA_UPDATER_DEFAULT_HASH, SETTLEMENT_PAYMENT_HASH, SINGLETON_LAUNCHER_HASH,
};
use chia_sdk_types::{
    Conditions,
    conditions::TransferNft,
    puzzles::{
        FUSION_HINT, FusionNftToLock, FusionNftToUnlock, FusionSingletonSolution, P2FusionSolution,
    },
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr, SExp};

use crate::{
    DriverError, FusionLayer, Launcher, Layer, Nft, NftInfo, Offer, P2FusionLayer, Puzzle,
    Singleton, SingletonInfo, SingletonLayer, SpendContext, SpendWithConditions,
};

#[cfg(test)]
mod tests;

/// A fusion singleton from CHIP-0021, which swaps the NFTs on one side for the NFTs on the other.
///
/// The singleton has no state of its own. It's deployed with [`Launcher::launch_fusion`], and then
/// side A is locked with [`Fusion::lock_nft`]. Anyone holding the NFTs on the other side can then
/// swap them for the locked ones with an offer, which is taken with [`Fusion::take_offer`].
pub type Fusion = Singleton<FusionLayer>;

/// The odd amount of a fusion singleton.
pub const FUSION_SINGLETON_AMOUNT: u64 = 1;

/// The amount of an NFT that can be swapped, since the puzzle only commits to payments of 1 mojo.
const FUSION_NFT_AMOUNT: u64 = 1;

/// The position of the inner solution in a singleton solution.
const SINGLETON_INNER_SOLUTION_INDEX: usize = 2;

/// The position of `unlock_a` in a [`FusionSingletonSolution`].
const FUSION_UNLOCK_A_INDEX: usize = 5;

/// Which way a swap goes. The puzzle only knows about swapping the two sides, so upgrading an NFT
/// is the same operation as fusing, with one NFT on each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionDirection {
    /// Locks side B to unlock side A.
    Fuse,
    /// Locks side A to unlock side B.
    Defuse,
}

impl FusionDirection {
    pub fn from_unlock_a(unlock_a: bool) -> Self {
        if unlock_a { Self::Fuse } else { Self::Defuse }
    }

    pub fn unlock_a(self) -> bool {
        matches!(self, Self::Fuse)
    }
}

/// A locked NFT to release from the fusion, and the p2 puzzle hash that it's sent to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FusionUnlock {
    pub nft: Nft,
    pub p2_puzzle_hash: Bytes32,
}

impl FusionUnlock {
    pub fn new(nft: Nft, p2_puzzle_hash: Bytes32) -> Self {
        Self {
            nft,
            p2_puzzle_hash,
        }
    }
}

/// The result of a swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FusionSwap {
    /// The next coin of the fusion singleton.
    pub fusion: Fusion,
    /// The NFTs that are now locked by the fusion.
    pub locked: Vec<Nft>,
    /// The NFTs that were released from the fusion.
    pub unlocked: Vec<Nft>,
}

impl Launcher {
    /// Creates a [`Launcher`] for a fusion, with the hint that identifies it on-chain.
    pub fn for_fusion(
        ctx: &mut SpendContext,
        parent_coin_id: Bytes32,
    ) -> Result<Self, DriverError> {
        let memos = ctx.hint(FUSION_HINT)?;
        Ok(Self::with_memos(
            parent_coin_id,
            FUSION_SINGLETON_AMOUNT,
            memos,
        ))
    }

    /// Launches a fusion that swaps the NFTs on each side. The launcher should be created with
    /// [`Launcher::for_fusion`], which makes it possible to find the fusion by its hint.
    ///
    /// The launcher id and sides are stored in the launcher's key value list, as they are on
    /// mainnet, so that the fusion can be recovered with [`Fusion::parse_launcher`].
    pub fn launch_fusion(
        self,
        ctx: &mut SpendContext,
        side_a: Vec<Bytes32>,
        side_b: Vec<Bytes32>,
    ) -> Result<(Conditions, Fusion), DriverError> {
        // The singleton puzzle requires an odd amount, so it could never be spent otherwise
        if self.singleton_amount().is_multiple_of(2) {
            return Err(DriverError::InvalidFusionSingletonAmount(
                self.singleton_amount(),
            ));
        }

        let launcher_coin = self.coin();
        let launcher_id = launcher_coin.coin_id();
        let info = FusionLayer::new(launcher_id, side_a, side_b)?;

        let memo = FusionMemo {
            launcher_id,
            side_a: info.side_a().to_vec(),
            side_b: info.side_b().to_vec(),
        };

        let (conditions, coin) = self.spend(ctx, info.inner_puzzle_hash().into(), memo)?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((conditions, Fusion::new(coin, proof, info)))
    }
}

impl SingletonInfo for FusionLayer {
    fn launcher_id(&self) -> Bytes32 {
        self.launcher_id
    }

    fn inner_puzzle_hash(&self) -> TreeHash {
        self.tree_hash()
    }
}

/// The launcher's key value list, which was used to deploy the fusions on mainnet.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
struct FusionMemo {
    launcher_id: Bytes32,
    side_a: Vec<Bytes32>,
    side_b: Vec<Bytes32>,
}

impl Fusion {
    /// Recovers a fusion from the spend of its launcher coin. Returns [`None`] if the coin isn't a
    /// launcher, if the sides it lists aren't those of the singleton it created, or if the singleton
    /// has an even amount, since it could never be spent and anything locked by it would be stuck.
    ///
    /// The reference implementation has since moved this data to a fourth item in the solution, so
    /// that's checked if the key value list is empty.
    pub fn parse_launcher(
        allocator: &Allocator,
        launcher_coin: Coin,
        launcher_solution: NodePtr,
    ) -> Option<Self> {
        if launcher_coin.puzzle_hash != SINGLETON_LAUNCHER_HASH.into() {
            return None;
        }

        let launcher_id = launcher_coin.coin_id();

        let (puzzle_hash, (amount, (key_value_list, extra_data))) =
            <(Bytes32, (u64, (NodePtr, NodePtr)))>::from_clvm(allocator, launcher_solution).ok()?;

        if amount.is_multiple_of(2) {
            return None;
        }

        let memo = FusionMemo::from_clvm(allocator, key_value_list)
            .or_else(|_| <(FusionMemo, ())>::from_clvm(allocator, extra_data).map(|memo| memo.0))
            .ok()?;

        if memo.launcher_id != launcher_id {
            return None;
        }

        let info = FusionLayer::new(launcher_id, memo.side_a, memo.side_b).ok()?;

        if puzzle_hash != info.puzzle_hash().into() {
            return None;
        }

        Some(Self::new(
            Coin::new(launcher_id, puzzle_hash, amount),
            Proof::Eve(EveProof {
                parent_parent_coin_info: launcher_coin.parent_coin_info,
                parent_amount: launcher_coin.amount,
            }),
            info,
        ))
    }

    /// Parses the child of a [`Fusion`] from the parent coin spend, along with the direction of the
    /// swap, which tells you which side is locked now. Returns [`None`] if the parent isn't a fusion.
    ///
    /// The solution must come from a confirmed spend. It isn't signed, so anyone can change the parts
    /// that the puzzle ignores or treats loosely. Only the direction is read, and in the same way as
    /// the puzzle reads it, so that those spends can still be parsed.
    pub fn parse_child(
        allocator: &Allocator,
        parent_coin: Coin,
        parent_puzzle: Puzzle,
        parent_solution: NodePtr,
    ) -> Result<Option<(Self, FusionDirection)>, DriverError> {
        let Some(layers) = SingletonLayer::<FusionLayer>::parse_puzzle(allocator, parent_puzzle)?
        else {
            return Ok(None);
        };

        if layers.launcher_id != layers.inner_puzzle.launcher_id {
            return Err(DriverError::InvalidSingletonStruct);
        }

        let unlock_a = parse_unlock_a(allocator, parent_solution)?;
        let parent = layers.inner_puzzle;

        let fusion = Self::new(
            Coin::new(
                parent_coin.coin_id(),
                parent_coin.puzzle_hash,
                parent_coin.amount,
            ),
            Proof::Lineage(LineageProof {
                parent_parent_coin_info: parent_coin.parent_coin_info,
                parent_inner_puzzle_hash: parent.inner_puzzle_hash().into(),
                parent_amount: parent_coin.amount,
            }),
            parent,
        );

        Ok(Some((fusion, FusionDirection::from_unlock_a(unlock_a))))
    }

    /// The conditions that an NFT on either side must output to be locked by the fusion. They send
    /// it to the fusion's p2 puzzle, and remove its assigned owner, since requested NFTs can't have
    /// one in an offer. An NFT that isn't on either side, or that can't be swapped, is rejected
    /// since it would be stuck forever.
    pub fn lock_nft_conditions(
        &self,
        ctx: &mut SpendContext,
        nft: &Nft,
    ) -> Result<Conditions, DriverError> {
        let launcher_id = nft.info.launcher_id;

        if !self.info.side_a().contains(&launcher_id) && !self.info.side_b().contains(&launcher_id)
        {
            return Err(DriverError::FusionNftMismatch);
        }

        check_nft(nft)?;

        let p2_puzzle_hash = self.info.p2_puzzle_hash();
        let memos = ctx.hint(p2_puzzle_hash)?;

        Ok(Conditions::new()
            .create_coin(p2_puzzle_hash, nft.coin.amount, memos)
            .with(TransferNft::new(None, Vec::new(), None)))
    }

    /// Locks an NFT on either side with a [`Layer`] that supports [`SpendWithConditions`]. This is
    /// how side A is first locked. See [`Fusion::lock_nft_conditions`].
    pub fn lock_nft<I>(
        &self,
        ctx: &mut SpendContext,
        nft: Nft,
        inner: &I,
        extra_conditions: Conditions,
    ) -> Result<Nft, DriverError>
    where
        I: SpendWithConditions,
    {
        let conditions = self.lock_nft_conditions(ctx, &nft)?;
        nft.spend_with(ctx, inner, extra_conditions.extend(conditions))
    }

    /// Spends the fusion to swap the two sides.
    ///
    /// The `offered` NFTs are those being locked, which must be in the settlement puzzle, such as
    /// the [offered coins](crate::OfferCoins) of an offer. The `unlocks` are the NFTs on the other
    /// side, which must be locked by the fusion. They can each be in any order, but the whole of
    /// both sides are always swapped.
    ///
    /// All of the spends are added to the [`SpendContext`], but neither the offered NFTs' parent
    /// spends nor any fee are included. Anyone can make this spend, since no signature is required.
    ///
    /// The spend that moves an offered NFT into the settlement puzzle must assert the payment that
    /// it expects in return, as offers do. Otherwise, anyone who sees that spend can pay the NFT to
    /// themselves instead, since settlement coins can be spent by anyone. Use
    /// [`Fusion::take_offer`] unless a locked NFT has an assigned owner, which an offer can't request.
    pub fn swap(
        &self,
        ctx: &mut SpendContext,
        direction: FusionDirection,
        offered: &[Nft],
        unlocks: &[FusionUnlock],
        offer_nonce: Bytes32,
    ) -> Result<FusionSwap, DriverError> {
        let (lock_side, unlock_side) = match direction {
            FusionDirection::Fuse => (self.info.side_b(), self.info.side_a()),
            FusionDirection::Defuse => (self.info.side_a(), self.info.side_b()),
        };

        let p2_puzzle_hash = self.info.p2_puzzle_hash();
        let offered = sort_nfts(lock_side, offered, |nft| nft)?;
        let unlocks = sort_nfts(unlock_side, unlocks, |unlock| &unlock.nft)?;

        let settlement = Bytes32::from(SETTLEMENT_PAYMENT_HASH);
        let inner_puzzle_hash = self.info.inner_puzzle_hash().into();

        // Everything is checked first so that an error doesn't leave spends in the context
        for nft in &offered {
            check_nft(nft)?;

            if nft.info.p2_puzzle_hash != settlement {
                return Err(DriverError::FusionNftMismatch);
            }
        }

        for unlock in &unlocks {
            check_nft(&unlock.nft)?;

            if unlock.nft.info.p2_puzzle_hash != p2_puzzle_hash {
                return Err(DriverError::FusionNftMismatch);
            }
        }

        let mut nfts_to_lock = Vec::with_capacity(offered.len());
        let mut locked = Vec::with_capacity(offered.len());

        for nft in offered {
            nfts_to_lock.push(FusionNftToLock::new(
                nft.coin.coin_id(),
                nft.info.metadata.tree_hash().into(),
                nft.info.current_owner,
                transfer_program_hash(&nft.info),
            ));

            let payment = settlement_payment(ctx, offer_nonce, p2_puzzle_hash)?;
            locked.push(nft.unlock_settlement(ctx, vec![payment])?);
        }

        let mut nfts_to_unlock = Vec::with_capacity(unlocks.len());
        let mut unlocked = Vec::with_capacity(unlocks.len());

        for &FusionUnlock {
            nft,
            p2_puzzle_hash: target_p2_puzzle_hash,
        } in unlocks
        {
            nfts_to_unlock.push(FusionNftToUnlock::new(
                nft.coin.coin_id(),
                nft.info.metadata.tree_hash().into(),
                nft.info.current_owner,
                transfer_program_hash(&nft.info),
                target_p2_puzzle_hash,
            ));

            let p2_spend = P2FusionLayer::new(self.info.launcher_id).spend(
                ctx,
                P2FusionSolution {
                    singleton_inner_puzzle_hash: inner_puzzle_hash,
                    singleton_coin_id: self.coin.coin_id(),
                    nft_launcher_id: nft.info.launcher_id,
                    nft_inner_puzzle_hash: nft.info.inner_puzzle_hash().into(),
                    my_amount: nft.coin.amount,
                    next_puzzle_hash: settlement,
                },
            )?;

            let settlement_nft = nft.spend(ctx, p2_spend)?;
            let payment = settlement_payment(ctx, offer_nonce, target_p2_puzzle_hash)?;
            unlocked.push(settlement_nft.unlock_settlement(ctx, vec![payment])?);
        }

        let spend = SingletonLayer::new(self.info.launcher_id, self.info.clone()).construct_spend(
            ctx,
            SingletonSolution {
                lineage_proof: self.proof,
                amount: self.coin.amount,
                inner_solution: FusionSingletonSolution {
                    my_id: self.coin.coin_id(),
                    my_inner_puzzle_hash: inner_puzzle_hash,
                    my_amount: self.coin.amount,
                    nfts_to_lock,
                    nfts_to_unlock,
                    unlock_a: direction.unlock_a(),
                    offer_nonce,
                },
            },
        )?;

        ctx.spend(self.coin, spend)?;

        Ok(FusionSwap {
            fusion: self.child_with(self.info.clone(), self.coin.amount),
            locked,
            unlocked,
        })
    }

    /// Takes an offer of the NFTs on one side, which requests the NFTs on the other side, by
    /// swapping them. The direction is determined by the offer, and each requested NFT is sent to
    /// the puzzle hash requested for it. The `unlocks` are the locked NFTs that are being requested.
    ///
    /// The spends that are added to the [`SpendContext`] should be passed to [`Offer::take`].
    pub fn take_offer(
        &self,
        ctx: &mut SpendContext,
        offer: &Offer,
        unlocks: &[Nft],
    ) -> Result<FusionSwap, DriverError> {
        let requested_payments = offer.requested_payments();

        let is_side = |side: &[Bytes32], launcher_ids: Vec<&Bytes32>| {
            launcher_ids.len() == side.len() && launcher_ids.iter().all(|id| side.contains(id))
        };

        let offered_ids: Vec<_> = offer.offered_coins().nfts.keys().collect();
        let requested_ids: Vec<_> = requested_payments.nfts.keys().collect();

        let direction = if is_side(self.info.side_b(), offered_ids.clone())
            && is_side(self.info.side_a(), requested_ids.clone())
        {
            FusionDirection::Fuse
        } else if is_side(self.info.side_a(), offered_ids)
            && is_side(self.info.side_b(), requested_ids)
        {
            FusionDirection::Defuse
        } else {
            return Err(DriverError::FusionOfferMismatch);
        };

        // The maker's assertions could only be met by the singleton if these are the only payments
        if !requested_payments.xch.is_empty()
            || !requested_payments.cats.is_empty()
            || !requested_payments.options.is_empty()
        {
            return Err(DriverError::FusionOfferMismatch);
        }

        let mut nonce = None;
        let mut fusion_unlocks = Vec::with_capacity(unlocks.len());

        for &nft in unlocks {
            let launcher_id = nft.info.launcher_id;

            let ([notarized_payment], Some(info)) = (
                requested_payments
                    .nfts
                    .get(&launcher_id)
                    .ok_or(DriverError::FusionOfferMismatch)?
                    .as_slice(),
                offer.asset_info().nft(launcher_id),
            ) else {
                return Err(DriverError::FusionOfferMismatch);
            };

            let [payment] = notarized_payment.payments.as_slice() else {
                return Err(DriverError::FusionOfferMismatch);
            };

            // The puzzle only announces payments with this amount and the puzzle hash as the hint
            let Memos::Some(memos) = payment.memos else {
                return Err(DriverError::FusionOfferMismatch);
            };

            if payment.amount != FUSION_NFT_AMOUNT
                || ctx.extract::<Vec<Bytes32>>(memos).ok() != Some(vec![payment.puzzle_hash])
                || *nonce.get_or_insert(notarized_payment.nonce) != notarized_payment.nonce
            {
                return Err(DriverError::FusionOfferMismatch);
            }

            // The maker's assertion is for an NFT with this puzzle, but the p2 puzzle can't change
            // the NFT's owner, so it must not have one
            let requested = NftInfo::new(
                launcher_id,
                info.metadata,
                info.metadata_updater_puzzle_hash,
                None,
                info.royalty_puzzle_hash,
                info.royalty_basis_points,
                SETTLEMENT_PAYMENT_HASH.into(),
            );
            let actual = NftInfo {
                p2_puzzle_hash: SETTLEMENT_PAYMENT_HASH.into(),
                ..nft.info
            };

            if requested.puzzle_hash() != actual.puzzle_hash() {
                return Err(DriverError::FusionOfferMismatch);
            }

            fusion_unlocks.push(FusionUnlock::new(nft, payment.puzzle_hash));
        }

        let offered: Vec<Nft> = offer.offered_coins().nfts.values().copied().collect();

        self.swap(
            ctx,
            direction,
            &offered,
            &fusion_unlocks,
            nonce.ok_or(DriverError::FusionOfferMismatch)?,
        )
    }
}

/// Reads `unlock_a` from a singleton solution like the puzzle does, where anything but nil is true.
fn parse_unlock_a(allocator: &Allocator, solution: NodePtr) -> Result<bool, DriverError> {
    let inner_solution = nth(allocator, solution, SINGLETON_INNER_SOLUTION_INDEX)?;
    let unlock_a = nth(allocator, inner_solution, FUSION_UNLOCK_A_INDEX)?;

    Ok(match allocator.sexp(unlock_a) {
        SExp::Atom => allocator.atom_len(unlock_a) != 0,
        SExp::Pair(..) => true,
    })
}

fn nth(allocator: &Allocator, mut list: NodePtr, index: usize) -> Result<NodePtr, DriverError> {
    for _ in 0..index {
        let (_, rest) = allocator.next(list).ok_or(DriverError::InvalidFusion)?;
        list = rest;
    }

    let (first, _) = allocator.next(list).ok_or(DriverError::InvalidFusion)?;
    Ok(first)
}

/// The puzzle only supports NFTs that have the default metadata updater and an amount of 1.
fn check_nft(nft: &Nft) -> Result<(), DriverError> {
    if nft.coin.amount != FUSION_NFT_AMOUNT
        || nft.info.metadata_updater_puzzle_hash != NFT_METADATA_UPDATER_DEFAULT_HASH.into()
    {
        return Err(DriverError::UnsupportedFusionNft);
    }

    Ok(())
}

fn transfer_program_hash(info: &NftInfo) -> Bytes32 {
    NftRoyaltyTransferPuzzleArgs::curry_tree_hash(
        info.launcher_id,
        info.royalty_puzzle_hash,
        info.royalty_basis_points,
    )
    .into()
}

fn settlement_payment(
    ctx: &mut SpendContext,
    nonce: Bytes32,
    puzzle_hash: Bytes32,
) -> Result<NotarizedPayment, DriverError> {
    let memos = ctx.hint(puzzle_hash)?;
    Ok(NotarizedPayment::new(
        nonce,
        vec![Payment::new(puzzle_hash, FUSION_NFT_AMOUNT, memos)],
    ))
}

/// Puts the NFTs in the order of the side, since the puzzle matches them by position. There must be
/// exactly one for each launcher id.
fn sort_nfts<'a, T>(
    side: &[Bytes32],
    items: &'a [T],
    nft: impl Fn(&'a T) -> &'a Nft,
) -> Result<Vec<&'a T>, DriverError> {
    if items.len() != side.len() {
        return Err(DriverError::FusionNftMismatch);
    }

    let mut items: HashMap<Bytes32, &T> = items
        .iter()
        .map(|item| (nft(item).info.launcher_id, item))
        .collect();

    side.iter()
        .map(|launcher_id| {
            items
                .remove(launcher_id)
                .ok_or(DriverError::FusionNftMismatch)
        })
        .collect()
}
