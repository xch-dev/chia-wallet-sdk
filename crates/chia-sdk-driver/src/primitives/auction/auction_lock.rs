use chia_protocol::Bytes32;
use chia_sdk_types::{
    Mod,
    puzzles::{SingletonMember, SingletonMemberSolution},
};
use clvm_utils::TreeHash;

use crate::{DriverError, InnerPuzzleSpend, MipsSpend, Spend, SpendContext, mips_puzzle_hash};

/// The p2 puzzle hash that locks the NFT and reserve coin of the auction with the given launcher
/// id. It's a MIPS puzzle whose only member is the auction singleton, so coins locked by it can
/// only be spent when the auction sends them a message.
///
/// The NFT must be locked by this puzzle hash before the auction is launched.
pub fn auction_lock_p2_puzzle_hash(launcher_id: Bytes32) -> Bytes32 {
    mips_puzzle_hash(
        0,
        vec![],
        SingletonMember::new(launcher_id).curry_tree_hash(),
        true,
    )
    .into()
}

/// Spends a coin locked by the auction. The auction singleton must send a message to the coin
/// with the tree hash of `delegated_spend`'s puzzle in the same spend bundle, where
/// `inner_puzzle_hash` is the inner puzzle hash of the auction coin being spent.
pub fn spend_auction_lock(
    ctx: &mut SpendContext,
    launcher_id: Bytes32,
    inner_puzzle_hash: TreeHash,
    delegated_spend: Spend,
) -> Result<Spend, DriverError> {
    let mut mips_spend = MipsSpend::new(delegated_spend);

    let p2_puzzle_hash = auction_lock_p2_puzzle_hash(launcher_id);
    let puzzle = ctx.curry(SingletonMember::new(launcher_id))?;
    let solution = ctx.alloc(&SingletonMemberSolution::new(inner_puzzle_hash.into(), 1))?;

    mips_spend.members.insert(
        p2_puzzle_hash.into(),
        InnerPuzzleSpend::new(0, vec![], Spend::new(puzzle, solution)),
    );

    mips_spend.spend(ctx, p2_puzzle_hash.into())
}
