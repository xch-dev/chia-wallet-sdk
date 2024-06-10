use chia_protocol::{Bytes32, Coin};
use chia_puzzles::{
    did::{DidArgs, DidSolution, DID_INNER_PUZZLE_HASH},
    singleton::{SingletonArgs, SingletonStruct},
    Proof,
};
use chia_sdk_types::{
    conditions::{puzzle_conditions, Condition, CreateCoin},
    puzzles::DidInfo,
};
use clvm_traits::FromClvm;
use clvm_utils::{tree_hash, CurriedProgram, ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{ParseError, Puzzle, SingletonPuzzle};

#[derive(Debug, Clone, Copy)]
pub struct DidPuzzle {
    pub p2_puzzle: Puzzle,
    pub recovery_did_list_hash: Bytes32,
    pub num_verifications_required: u64,
    pub metadata: NodePtr,
}

impl DidPuzzle {
    pub fn parse(
        allocator: &Allocator,
        launcher_id: Bytes32,
        puzzle: &Puzzle,
    ) -> Result<Option<Self>, ParseError> {
        let Some(puzzle) = puzzle.as_curried() else {
            return Ok(None);
        };

        if puzzle.mod_hash != DID_INNER_PUZZLE_HASH {
            return Ok(None);
        }

        let args = DidArgs::<NodePtr, NodePtr>::from_clvm(allocator, puzzle.args)?;

        if args.singleton_struct != SingletonStruct::new(launcher_id) {
            return Err(ParseError::InvalidSingletonStruct);
        }

        Ok(Some(DidPuzzle {
            p2_puzzle: Puzzle::parse(allocator, args.inner_puzzle),
            recovery_did_list_hash: args.recovery_did_list_hash,
            num_verifications_required: args.num_verifications_required,
            metadata: args.metadata,
        }))
    }

    pub fn output(
        &self,
        allocator: &mut Allocator,
        solution: NodePtr,
    ) -> Result<Option<CreateCoin>, ParseError> {
        let DidSolution::InnerSpend(p2_solution) =
            DidSolution::<NodePtr>::from_clvm(allocator, solution)?;

        let conditions = puzzle_conditions(allocator, self.p2_puzzle.ptr(), p2_solution)?;

        let create_coin = conditions
            .into_iter()
            .find_map(|condition| match condition {
                Condition::CreateCoin(create_coin) if create_coin.amount % 2 == 1 => {
                    Some(create_coin)
                }
                _ => None,
            });

        Ok(create_coin)
    }

    pub fn child_coin_info(
        &self,
        allocator: &mut Allocator,
        singleton: &SingletonPuzzle,
        parent_coin: Coin,
        child_coin: Coin,
        solution: NodePtr,
    ) -> Result<DidInfo<NodePtr>, ParseError> {
        let create_coin = self
            .output(allocator, solution)?
            .ok_or(ParseError::MissingChild)?;

        let Some(hint) = create_coin.memos.first() else {
            return Err(ParseError::MissingHint);
        };

        let p2_puzzle_hash = hint.try_into().map_err(|_| ParseError::MissingHint)?;

        let did_inner_puzzle_hash = CurriedProgram {
            program: DID_INNER_PUZZLE_HASH,
            args: DidArgs {
                inner_puzzle: TreeHash::from(p2_puzzle_hash),
                recovery_did_list_hash: self.recovery_did_list_hash,
                num_verifications_required: self.num_verifications_required,
                metadata: tree_hash(allocator, self.metadata),
                singleton_struct: SingletonStruct::new(singleton.launcher_id),
            },
        }
        .tree_hash();

        let singleton_puzzle_hash =
            SingletonArgs::curry_tree_hash(singleton.launcher_id, did_inner_puzzle_hash);

        if singleton_puzzle_hash != child_coin.puzzle_hash.into() {
            return Err(ParseError::MismatchedOutput);
        }

        Ok(DidInfo {
            launcher_id: singleton.launcher_id,
            coin: child_coin,
            p2_puzzle_hash,
            did_inner_puzzle_hash: did_inner_puzzle_hash.into(),
            recovery_did_list_hash: self.recovery_did_list_hash,
            num_verifications_required: self.num_verifications_required,
            metadata: self.metadata,
            proof: Proof::Lineage(singleton.lineage_proof(parent_coin)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chia_bls::PublicKey;
    use chia_protocol::Coin;
    use chia_puzzles::{singleton::SingletonSolution, standard::StandardArgs};
    use chia_sdk_driver::{Launcher, SpendContext};
    use clvm_traits::ToNodePtr;

    #[test]
    fn test_parse_did() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ctx = &mut SpendContext::new(&mut allocator);

        let pk = PublicKey::default();
        let puzzle_hash = StandardArgs::curry_tree_hash(pk).into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 1);

        let (create_did, did_info) =
            Launcher::new(parent.coin_id(), 1).create_standard_did(ctx, pk)?;

        ctx.spend_p2_coin(parent, pk, create_did)?;

        let coin_spends = ctx.take_spends();

        let coin_spend = coin_spends
            .into_iter()
            .find(|cs| cs.coin.coin_id() == did_info.coin.parent_coin_info)
            .unwrap();

        let puzzle_ptr = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution_ptr = coin_spend.solution.to_node_ptr(&mut allocator)?;

        let puzzle = Puzzle::parse(&allocator, puzzle_ptr);

        let singleton =
            SingletonPuzzle::parse(&allocator, &puzzle)?.expect("not a singleton puzzle");
        let singleton_solution = SingletonSolution::<NodePtr>::from_clvm(&allocator, solution_ptr)?;

        let did = DidPuzzle::parse(&allocator, singleton.launcher_id, &singleton.inner_puzzle)?
            .expect("not a did puzzle");

        let parsed_did_info = did.child_coin_info(
            &mut allocator,
            &singleton,
            coin_spend.coin,
            did_info.coin,
            singleton_solution.inner_solution,
        )?;

        assert_eq!(parsed_did_info, did_info.with_metadata(NodePtr::NIL));

        Ok(())
    }
}
