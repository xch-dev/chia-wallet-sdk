use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{
    EveProof, LineageProof, Proof,
    singleton::{LauncherSolution, SingletonArgs},
};
use chia_puzzles::SINGLETON_LAUNCHER_HASH;
use chia_sdk_types::{Conditions, puzzles::AuctionState};
use clvm_traits::FromClvm;
use clvmr::{Allocator, NodePtr};

use crate::{
    AUCTION_SINGLETON_AMOUNT, Auction, AuctionInfo, AuctionMemo, AuctionReserve, AuctionSettings,
    DriverError, Launcher, Nft, RoyaltyInfo, SingletonInfo, SpendContext,
    auction_lock_p2_puzzle_hash,
};

impl Launcher {
    /// Launches an auction for `nft`, which must already be locked by the auction (its p2 puzzle
    /// hash must be [`auction_lock_p2_puzzle_hash`] of the launcher id). The `reserve` must be an
    /// empty coin that is also locked by the auction, which is typically created in the same spend
    /// bundle.
    ///
    /// The launcher must have a singleton amount of [`AUCTION_SINGLETON_AMOUNT`].
    ///
    /// A reserve in a revocable CAT can be revoked by the CAT's issuer at any time. That takes the
    /// winning bid, and the auction can't be ended (so the NFT stays locked) until a coin of the
    /// same amount is locked by the auction to stand in for the reserve. Wallets should warn
    /// sellers about this before launching an auction with a revocable CAT reserve.
    pub fn launch_auction(
        self,
        ctx: &mut SpendContext,
        settings: AuctionSettings,
        reserve: AuctionReserve,
        nft: &Nft,
    ) -> Result<(Conditions, Auction), DriverError> {
        let launcher_coin = self.coin();

        let info = AuctionInfo::new(
            launcher_coin.coin_id(),
            settings,
            nft.coin.coin_id(),
            RoyaltyInfo::new(
                nft.info.launcher_id,
                nft.info.royalty_puzzle_hash,
                nft.info.royalty_basis_points,
            ),
            AuctionState::initial(settings.payments.payout_puzzle_hash),
            reserve,
        );

        info.validate()?;

        if self.singleton_amount() != AUCTION_SINGLETON_AMOUNT {
            return Err(DriverError::InvalidAuctionSingletonAmount(
                self.singleton_amount(),
            ));
        }

        // The memo only stores the reserve's parent, so a reserve that isn't an empty coin locked
        // by the auction couldn't be found by other wallets or spent, and the NFT would be stuck
        let lock_puzzle_hash = auction_lock_p2_puzzle_hash(info.launcher_id);

        let reserve_puzzle_hash = match reserve {
            AuctionReserve::Xch(_) => lock_puzzle_hash,
            AuctionReserve::Cat(cat) => cat.info.puzzle_hash().into(),
        };

        if reserve.p2_puzzle_hash() != lock_puzzle_hash
            || reserve.coin().puzzle_hash != reserve_puzzle_hash
            || reserve.coin().amount != 0
        {
            return Err(DriverError::InvalidAuctionReserve);
        }

        if nft.info.p2_puzzle_hash != lock_puzzle_hash {
            return Err(DriverError::AuctionNftNotLocked);
        }

        let (conditions, coin) = self.spend(ctx, info.inner_puzzle_hash().into(), info.memo())?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((conditions, Auction::new(coin, proof, info)))
    }
}

/// The spend of an auction's launcher coin, before it's been checked against the locked NFT.
///
/// The NFT's royalty info is curried into the auction, but isn't stored in the memo. So wallets
/// must fetch the NFT at [`nft_coin_id`](AuctionLaunch::nft_coin_id) from the blockchain, and pass
/// it to [`into_auction`](AuctionLaunch::into_auction), which checks that it's locked by the
/// auction. For CAT reserves, they must also parse the spend of the reserve's parent (at
/// [`reserve_parent_coin_info`](AuctionLaunch::reserve_parent_coin_info)) to get its lineage proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuctionLaunch {
    /// The launcher coin that was spent.
    pub launcher_coin: Coin,
    /// The auction memo from the key value list of the launcher solution.
    pub memo: AuctionMemo,
    /// The puzzle hash of the singleton created by the launcher, which is checked against the
    /// reconstructed auction by [`into_auction`](AuctionLaunch::into_auction).
    pub singleton_puzzle_hash: Bytes32,
    /// The amount of the singleton created by the launcher.
    pub singleton_amount: u64,
}

impl AuctionLaunch {
    /// Parses the spend of a launcher coin. Returns [`None`] if the coin isn't a launcher, or its
    /// solution doesn't contain an auction memo.
    pub fn parse(
        allocator: &Allocator,
        launcher_coin: Coin,
        launcher_solution: NodePtr,
    ) -> Option<Self> {
        if launcher_coin.puzzle_hash != SINGLETON_LAUNCHER_HASH.into() {
            return None;
        }

        let solution =
            LauncherSolution::<AuctionMemo>::from_clvm(allocator, launcher_solution).ok()?;

        Some(Self {
            launcher_coin,
            memo: solution.key_value_list,
            singleton_puzzle_hash: solution.singleton_puzzle_hash,
            singleton_amount: solution.amount,
        })
    }

    /// The coin id of the locked NFT, which must be passed to
    /// [`into_auction`](AuctionLaunch::into_auction).
    pub fn nft_coin_id(&self) -> Bytes32 {
        self.memo.nft_coin_id
    }

    /// The parent coin id of the reserve. For CAT reserves, the spend of this coin is parsed to get
    /// the reserve's lineage proof.
    pub fn reserve_parent_coin_info(&self) -> Bytes32 {
        self.memo.reserve.parent_coin_info()
    }

    /// Reconstructs the auction, given the NFT at [`nft_coin_id`](AuctionLaunch::nft_coin_id) as
    /// parsed from the blockchain, and the lineage proof of the reserve if it's a CAT.
    ///
    /// Returns an error if the auction is invalid or doesn't match the launched singleton (which
    /// includes an NFT royalty that doesn't match the auction), or if `nft` isn't that coin or isn't
    /// locked by the auction. An auction whose NFT isn't locked by it can never be ended, so the
    /// winning bid would be locked up forever.
    ///
    /// Whether the NFT is still unspent can't be checked offline, so wallets must check that before
    /// bidding. A wrong lineage proof can't put any funds at risk, but the reserve can't be spent
    /// with it.
    pub fn into_auction(
        self,
        nft: &Nft,
        reserve_lineage_proof: Option<LineageProof>,
    ) -> Result<Auction, DriverError> {
        let launcher_id = self.launcher_coin.coin_id();
        let nft_royalty = RoyaltyInfo::new(
            nft.info.launcher_id,
            nft.info.royalty_puzzle_hash,
            nft.info.royalty_basis_points,
        );
        let info =
            AuctionInfo::from_memo(launcher_id, self.memo, nft_royalty, reserve_lineage_proof);

        info.validate()?;

        if self.singleton_amount != AUCTION_SINGLETON_AMOUNT {
            return Err(DriverError::InvalidAuctionSingletonAmount(
                self.singleton_amount,
            ));
        }

        let puzzle_hash: Bytes32 =
            SingletonArgs::curry_tree_hash(launcher_id, info.inner_puzzle_hash()).into();

        if puzzle_hash != self.singleton_puzzle_hash {
            return Err(DriverError::AuctionPuzzleHashMismatch);
        }

        // The royalty info and lock are read from `nft.info`, so it must be the info of the coin
        if nft.coin.coin_id() != self.memo.nft_coin_id
            || nft.coin.puzzle_hash != nft.info.puzzle_hash().into()
        {
            return Err(DriverError::AuctionNftMismatch);
        }

        if nft.info.p2_puzzle_hash != auction_lock_p2_puzzle_hash(launcher_id) {
            return Err(DriverError::AuctionNftNotLocked);
        }

        let coin = Coin::new(launcher_id, puzzle_hash, self.singleton_amount);

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: self.launcher_coin.parent_coin_info,
            parent_amount: self.launcher_coin.amount,
        });

        Ok(Auction::new(coin, proof, info))
    }
}
