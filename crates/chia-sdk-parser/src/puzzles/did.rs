use chia_protocol::Bytes32;
use chia_puzzles::{
    did::{DidArgs, DidSolution, DID_INNER_PUZZLE_HASH},
    singleton::{SingletonArgs, SingletonStruct, SINGLETON_TOP_LAYER_PUZZLE_HASH},
    LineageProof, Proof,
};
use chia_sdk_types::{conditions::CreateCoin, puzzles::DidInfo};
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{reduction::Reduction, run_program, Allocator, ChiaDialect, NodePtr};

use crate::{ParseContext, ParseError, ParseSingleton};

pub fn parse_did(
    allocator: &mut Allocator,
    ctx: &ParseContext,
    singleton: &ParseSingleton,
    max_cost: u64,
) -> Result<Option<DidInfo<NodePtr>>, ParseError> {
    if singleton.inner_mod_hash().to_bytes() != DID_INNER_PUZZLE_HASH.to_bytes() {
        return Ok(None);
    }

    let args = DidArgs::<NodePtr, NodePtr>::from_clvm(allocator, singleton.inner_args())?;

    let DidSolution::InnerSpend(p2_solution) =
        DidSolution::<NodePtr>::from_clvm(allocator, singleton.inner_solution())?;

    if args.singleton_struct != singleton.args().singleton_struct {
        return Err(ParseError::DidSingletonStructMismatch);
    }

    let Reduction(_cost, output) = run_program(
        allocator,
        &ChiaDialect::new(0),
        args.inner_puzzle,
        p2_solution,
        max_cost,
    )?;

    let conditions = Vec::<NodePtr>::from_clvm(allocator, output)?;
    let mut p2_puzzle_hash = None;

    for condition in conditions {
        let Ok(create_coin) = CreateCoin::from_clvm(allocator, condition) else {
            continue;
        };

        if create_coin.amount % 2 == 0 {
            continue;
        }

        p2_puzzle_hash = create_coin
            .memos
            .first()
            .and_then(|memo| Some(Bytes32::new(memo.as_ref().try_into().ok()?)));
        break;
    }

    let Some(p2_puzzle_hash) = p2_puzzle_hash else {
        return Err(ParseError::MissingCreateCoin);
    };

    let did_inner_puzzle_hash = CurriedProgram {
        program: DID_INNER_PUZZLE_HASH,
        args: DidArgs {
            inner_puzzle: TreeHash::from(p2_puzzle_hash),
            recovery_did_list_hash: args.recovery_did_list_hash,
            num_verifications_required: args.num_verifications_required,
            metadata: tree_hash(allocator, args.metadata),
            singleton_struct: SingletonStruct::new(args.singleton_struct.launcher_id),
        },
    }
    .tree_hash()
    .into();

    let singleton_puzzle_hash: Bytes32 = CurriedProgram {
        program: SINGLETON_TOP_LAYER_PUZZLE_HASH,
        args: SingletonArgs {
            singleton_struct: args.singleton_struct,
            inner_puzzle: TreeHash::from(did_inner_puzzle_hash),
        },
    }
    .tree_hash()
    .into();

    if singleton_puzzle_hash != ctx.coin().puzzle_hash {
        return Err(ParseError::UnknownDidOutput);
    }

    Ok(Some(DidInfo {
        launcher_id: args.singleton_struct.launcher_id,
        coin: ctx.coin(),
        p2_puzzle_hash,
        did_inner_puzzle_hash,
        recovery_did_list_hash: args.recovery_did_list_hash,
        num_verifications_required: args.num_verifications_required,
        metadata: args.metadata,
        proof: Proof::Lineage(LineageProof {
            parent_parent_coin_id: ctx.parent_coin().parent_coin_info,
            parent_inner_puzzle_hash: tree_hash(allocator, singleton.args().inner_puzzle).into(),
            parent_amount: ctx.parent_coin().amount,
        }),
    }))
}

#[cfg(test)]
mod tests {
    use chia_bls::PublicKey;
    use chia_protocol::{Bytes32, Coin};
    use chia_puzzles::standard::{StandardArgs, STANDARD_PUZZLE_HASH};
    use chia_sdk_driver::{
        puzzles::{CreateDid, Launcher, StandardSpend},
        SpendContext,
    };
    use clvm_traits::ToNodePtr;
    use clvm_utils::{CurriedProgram, ToTreeHash};
    use clvmr::Allocator;

    use crate::{parse_did, parse_puzzle, parse_singleton};

    #[test]
    fn test_parse_did() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let pk = PublicKey::default();
        let puzzle_hash = CurriedProgram {
            program: STANDARD_PUZZLE_HASH,
            args: StandardArgs { synthetic_key: pk },
        }
        .tree_hash()
        .into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 1);

        let (create_did, did_info) = Launcher::new(parent.coin_id(), 1)
            .create(&mut ctx)?
            .create_standard_did(&mut ctx, pk)?;

        StandardSpend::new()
            .chain(create_did)
            .finish(&mut ctx, parent, pk)?;

        let coin_spends = ctx.take_spends();

        let coin_spend = coin_spends
            .into_iter()
            .find(|cs| cs.coin.coin_id() == did_info.coin.parent_coin_info)
            .unwrap();

        let puzzle = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution = coin_spend.solution.to_node_ptr(&mut allocator)?;

        let parse_ctx = parse_puzzle(&allocator, puzzle, solution, coin_spend.coin, did_info.coin)?;
        let parse = parse_singleton(&allocator, &parse_ctx)?.unwrap();
        let parse = parse_did(&mut allocator, &parse_ctx, &parse, u64::MAX)?;
        assert_eq!(
            parse.map(|did_info| did_info.with_metadata(())),
            Some(did_info)
        );

        Ok(())
    }
}
