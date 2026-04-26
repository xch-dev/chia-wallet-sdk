use chia_protocol::Coin;
use clvmr::Allocator;

use crate::{
    Cat, ClawbackInfo, ClawbackPath, CustodyInfo, DriverError, Nft, RevealedCoinSpend, Reveals,
    parse_inner_spend,
};

#[derive(Debug, Clone)]
pub struct ParsedSpend {
    pub asset: ParsedAsset,
    pub clawback: Option<ClawbackInfo>,
    pub custody: Option<CustodyInfo>,
    pub required_expiration_time: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub enum ParsedAsset {
    Cat(Cat),
    Nft(Nft),
    Xch(Coin),
}

impl ParsedAsset {
    pub fn coin(&self) -> Coin {
        match self {
            Self::Cat(cat) => cat.coin,
            Self::Nft(nft) => nft.coin,
            Self::Xch(coin) => *coin,
        }
    }
}

pub fn parse_spend(
    reveals: &Reveals,
    allocator: &mut Allocator,
    spend: &RevealedCoinSpend,
) -> Result<ParsedSpend, DriverError> {
    // The default is to treat the spend as XCH if we don't have a more complex asset to try and parse.
    let mut asset = ParsedAsset::Xch(spend.coin);
    let mut inner_puzzle = spend.puzzle;
    let mut inner_solution = spend.solution;

    if let Some((cat, parsed_inner_puzzle, parsed_inner_solution)) =
        Cat::parse(allocator, spend.coin, spend.puzzle, spend.solution)?
    {
        asset = ParsedAsset::Cat(cat);
        inner_puzzle = parsed_inner_puzzle;
        inner_solution = parsed_inner_solution;
    } else if let Some((nft, parsed_inner_puzzle, parsed_inner_solution)) =
        Nft::parse(allocator, spend.coin, spend.puzzle, spend.solution)?
    {
        asset = ParsedAsset::Nft(nft);
        inner_puzzle = parsed_inner_puzzle;
        inner_solution = parsed_inner_solution;
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
    })
}
