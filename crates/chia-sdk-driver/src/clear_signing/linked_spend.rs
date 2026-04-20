use chia_protocol::Coin;
use clvmr::Allocator;

use crate::{
    Cat, CustodyInfo, DriverError, Facts, InnerSpend, Nft, VaultMessage, parse_inner_spend,
};

#[derive(Debug, Clone)]
pub struct LinkedSpendSummary {
    pub linked_asset: LinkedAsset,
    pub inner_spend: InnerSpend,
}

#[derive(Debug, Clone, Copy)]
pub enum LinkedAsset {
    Cat(Cat),
    Nft(Nft),
    Xch(Coin),
}

pub fn parse_linked_spend(
    facts: &mut Facts,
    allocator: &mut Allocator,
    vault_message: VaultMessage,
) -> Result<LinkedSpendSummary, DriverError> {
    let Some(spend) = facts.coin_spend(vault_message.spent_coin_id) else {
        return Err(DriverError::MissingSpend);
    };

    // The default is to treat the spend as XCH if we don't have a more complex asset to try and parse.
    let mut linked_asset = LinkedAsset::Xch(spend.coin);
    let mut inner_puzzle = spend.puzzle;
    let mut inner_solution = spend.solution;

    if let Some((cat, parsed_inner_puzzle, parsed_inner_solution)) =
        Cat::parse(allocator, spend.coin, spend.puzzle, spend.solution)?
    {
        linked_asset = LinkedAsset::Cat(cat);
        inner_puzzle = parsed_inner_puzzle;
        inner_solution = parsed_inner_solution;
    } else if let Some((nft, parsed_inner_puzzle, parsed_inner_solution)) =
        Nft::parse(allocator, spend.coin, spend.puzzle, spend.solution)?
    {
        linked_asset = LinkedAsset::Nft(nft);
        inner_puzzle = parsed_inner_puzzle;
        inner_solution = parsed_inner_solution;
    }

    let inner_spend = parse_inner_spend(facts, allocator, inner_puzzle, inner_solution)?;

    let Some(CustodyInfo::P2Singleton {
        launcher_id,
        nonce,
        conditions,
    }) = &inner_spend.custody
    else {
        return Err(DriverError::InvalidLinkedCustody);
    };

    Ok(LinkedSpendSummary {
        linked_asset,
        inner_spend,
    })
}
