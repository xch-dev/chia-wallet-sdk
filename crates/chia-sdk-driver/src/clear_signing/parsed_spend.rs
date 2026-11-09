use chia_protocol::Coin;
use chia_puzzles::SETTLEMENT_PAYMENT_HASH;
use clvmr::Allocator;

use crate::{
    Bulletin, Cat, ClawbackInfo, ClawbackPath, CustodyInfo, DriverError, Nft, RevealedCoinSpend,
    Reveals, parse_inner_spend,
};

/// A revealed coin spend, parsed into its asset and custody, before it's known whether the vault authorizes it.
#[derive(Debug, Clone)]
pub struct ParsedSpend {
    /// The asset held by the coin.
    pub asset: ParsedAsset,
    /// The clawback the coin is locked in, and the path it's spent with, if the clawback was revealed.
    pub clawback: Option<ClawbackInfo>,
    /// The p2 puzzle that controls the coin, or `None` if it isn't recognized (or is a clawback spent with the
    /// push through path).
    pub custody: Option<CustodyInfo>,
    /// The expiration time of the clawback, if the coin is being clawed back with the sender path. The
    /// transaction must expire by then.
    pub required_expiration_time: Option<u64>,
    /// Whether the coin is a revocable CAT being spent with its hidden puzzle.
    pub revoked: bool,
}

/// The asset held by a coin, as identified by its outer puzzles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedAsset {
    /// A CAT.
    Cat(Cat),
    /// An NFT.
    Nft(Nft),
    /// XCH, which is also used for any coin whose outer puzzle isn't recognized as another asset.
    Xch(Coin),
    /// A bulletin coin, which holds XCH and publishes messages.
    Bulletin(Bulletin),
}

impl ParsedAsset {
    /// The coin, including its outer puzzle hash.
    pub fn coin(&self) -> Coin {
        match self {
            Self::Cat(cat) => cat.coin,
            Self::Nft(nft) => nft.coin,
            Self::Xch(coin) => *coin,
            Self::Bulletin(bulletin) => bulletin.coin,
        }
    }
}

/// Parses a revealed coin spend into its asset and custody.
///
/// If the coin's p2 puzzle is the settlement payments puzzle, its notarized payments are revealed as requested
/// payments as well.
pub fn parse_spend(
    reveals: &mut Reveals,
    allocator: &mut Allocator,
    spend: &RevealedCoinSpend,
) -> Result<ParsedSpend, DriverError> {
    // The default is to treat the spend as XCH if we don't have a more complex asset to try and parse.
    let mut asset = ParsedAsset::Xch(spend.coin);
    let mut inner_puzzle = spend.puzzle;
    let mut inner_solution = spend.solution;
    let mut revoked = false;

    if let Some(parsed) = Cat::parse(allocator, spend.coin, spend.puzzle, spend.solution)? {
        asset = ParsedAsset::Cat(parsed.cat);
        inner_puzzle = parsed.p2_puzzle;
        inner_solution = parsed.p2_solution;
        revoked = parsed.revoked;
    } else if let Some((nft, parsed_inner_puzzle, parsed_inner_solution)) =
        Nft::parse(allocator, spend.coin, spend.puzzle, spend.solution)?
    {
        asset = ParsedAsset::Nft(nft);
        inner_puzzle = parsed_inner_puzzle;
        inner_solution = parsed_inner_solution;
    } else if let Some((bulletin, parsed_inner_puzzle, parsed_inner_solution)) =
        Bulletin::parse(allocator, spend.coin, spend.puzzle, spend.solution)?
    {
        asset = ParsedAsset::Bulletin(bulletin);
        inner_puzzle = parsed_inner_puzzle;
        inner_solution = parsed_inner_solution;
    }

    if inner_puzzle.curried_puzzle_hash() == SETTLEMENT_PAYMENT_HASH.into() {
        reveals.reveal_settlement_payment(allocator, spend.puzzle, inner_solution)?;
    }

    let inner_spend = parse_inner_spend(reveals, allocator, inner_puzzle, inner_solution)?;

    // If we're clawing a coin back, we need to keep track of its expiration time.
    // This will be used to ensure that the clawback won't expire before the rest of
    // the transaction. If it might, the facts of this spend will be disregarded.
    let mut required_expiration_time = None;

    if let Some(clawback_info) = &inner_spend.clawback
        && clawback_info.path == ClawbackPath::Sender
    {
        required_expiration_time = Some(clawback_info.clawback.seconds);
    }

    Ok(ParsedSpend {
        asset,
        clawback: inner_spend.clawback,
        custody: inner_spend.custody,
        required_expiration_time,
        revoked,
    })
}
