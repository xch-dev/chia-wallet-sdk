use chia_protocol::CoinSpend;
use chia_wallet::{
    did::{DidArgs, DidSolution},
    singleton::{SingletonStruct, SINGLETON_LAUNCHER_PUZZLE_HASH, SINGLETON_TOP_LAYER_PUZZLE_HASH},
};
use clvm_traits::ToClvm;
use clvm_utils::CurriedProgram;
use clvmr::NodePtr;

use crate::{spend_singleton, DidInfo, InnerSpend, SpendContext, SpendError};

pub fn spend_did<T>(
    ctx: &mut SpendContext,
    did_info: DidInfo<T>,
    inner_spend: InnerSpend,
) -> Result<CoinSpend, SpendError>
where
    T: ToClvm<NodePtr>,
{
    let did_inner_puzzle = ctx.did_inner_puzzle();

    let puzzle = ctx.alloc(CurriedProgram {
        program: did_inner_puzzle,
        args: DidArgs {
            inner_puzzle: inner_spend.puzzle(),
            recovery_did_list_hash: did_info.recovery_did_list_hash,
            num_verifications_required: did_info.num_verifications_required,
            singleton_struct: SingletonStruct {
                mod_hash: SINGLETON_TOP_LAYER_PUZZLE_HASH.into(),
                launcher_id: did_info.launcher_id,
                launcher_puzzle_hash: SINGLETON_LAUNCHER_PUZZLE_HASH.into(),
            },
            metadata: did_info.metadata,
        },
    })?;

    let solution = ctx.alloc(DidSolution::InnerSpend(inner_spend.solution()))?;

    let did_spend = InnerSpend::new(puzzle, solution);

    spend_singleton(
        ctx,
        did_info.coin,
        did_info.launcher_id,
        did_info.proof,
        did_spend,
    )
}
