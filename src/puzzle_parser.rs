use chia_protocol::{Bytes32, Coin};
use chia_wallet::{
    cat::{cat_puzzle_hash, CatArgs, CatSolution, CAT_PUZZLE_HASH},
    did::{DidArgs, DidSolution, DID_INNER_PUZZLE_HASH},
    singleton::{
        SingletonArgs, SingletonSolution, SINGLETON_LAUNCHER_PUZZLE_HASH,
        SINGLETON_TOP_LAYER_PUZZLE_HASH,
    },
    LineageProof, Proof,
};
use clvm_traits::{FromClvm, FromClvmError};
use clvm_utils::{tree_hash, CurriedProgram};
use clvmr::{
    reduction::{EvalErr, Reduction},
    run_program, Allocator, ChiaDialect, NodePtr,
};
use thiserror::Error;

use crate::{
    did_inner_puzzle_hash, singleton_puzzle_hash, CatInfo, CreateCoin, CreateCoinWithMemos,
    DidInfo, NftInfo,
};

#[derive(Debug, Error)]
pub enum PuzzleError {
    #[error("Eval error: {0}")]
    Eval(#[from] EvalErr),

    #[error("CLVM error: {0}")]
    FromClvm(#[from] FromClvmError),

    #[error("Invalid puzzle")]
    InvalidPuzzle,

    #[error("Incorrect hint")]
    MissingCreateCoin,

    #[error("DID singleton struct mismatch")]
    DidSingletonStructMismatch,

    #[error("Invalid singleton struct")]
    InvalidSingletonStruct,

    #[error("Unknown DID output")]
    UnknownDidOutput,
}

pub enum Puzzle {
    Cat(CatInfo),
    Did(DidInfo<NodePtr>),
    Nft(NftInfo<NodePtr>),
}

impl Puzzle {
    pub fn parse(
        allocator: &mut Allocator,
        parent_puzzle: NodePtr,
        parent_solution: NodePtr,
        parent_coin: Coin,
        coin: Coin,
        max_cost: u64,
    ) -> Result<Self, PuzzleError> {
        let CurriedProgram { program, args } =
            CurriedProgram::<NodePtr, NodePtr>::from_clvm(allocator, parent_puzzle)?;

        match tree_hash(allocator, program) {
            CAT_PUZZLE_HASH => {
                let cat_args = CatArgs::<NodePtr>::from_clvm(allocator, args)?;
                let cat_solution = CatSolution::<NodePtr>::from_clvm(allocator, parent_solution)?;

                let Reduction(_cost, output) = run_program(
                    allocator,
                    &ChiaDialect::new(0),
                    cat_args.inner_puzzle,
                    cat_solution.inner_puzzle_solution,
                    max_cost,
                )?;

                let conditions = Vec::<NodePtr>::from_clvm(allocator, output)?;
                let mut p2_puzzle_hash = None;

                for condition in conditions {
                    let Ok(create_coin) = CreateCoin::from_clvm(allocator, condition) else {
                        continue;
                    };

                    let cat_puzzle_hash = Bytes32::new(cat_puzzle_hash(
                        cat_args.tail_program_hash.into(),
                        create_coin.puzzle_hash().into(),
                    ));

                    if cat_puzzle_hash == coin.puzzle_hash && create_coin.amount() == coin.amount {
                        p2_puzzle_hash = Some(create_coin.puzzle_hash());
                        break;
                    }
                }

                let Some(p2_puzzle_hash) = p2_puzzle_hash else {
                    return Err(PuzzleError::MissingCreateCoin);
                };

                Ok(Puzzle::Cat(CatInfo {
                    asset_id: cat_args.tail_program_hash,
                    p2_puzzle_hash,
                    coin,
                    lineage_proof: LineageProof {
                        parent_coin_info: parent_coin.parent_coin_info,
                        inner_puzzle_hash: tree_hash(allocator, cat_args.inner_puzzle).into(),
                        amount: parent_coin.amount,
                    },
                }))
            }
            SINGLETON_TOP_LAYER_PUZZLE_HASH => {
                let singleton_args = SingletonArgs::<NodePtr>::from_clvm(allocator, args)?;
                let singleton_solution =
                    SingletonSolution::<NodePtr>::from_clvm(allocator, parent_solution)?;
                let CurriedProgram { program, args } =
                    CurriedProgram::<NodePtr, NodePtr>::from_clvm(
                        allocator,
                        singleton_args.inner_puzzle,
                    )?;

                let singleton_mod_hash = singleton_args.singleton_struct.mod_hash.as_ref();
                let launcher_puzzle_hash = singleton_args
                    .singleton_struct
                    .launcher_puzzle_hash
                    .as_ref();

                if singleton_mod_hash != SINGLETON_TOP_LAYER_PUZZLE_HASH
                    || launcher_puzzle_hash != SINGLETON_LAUNCHER_PUZZLE_HASH
                {
                    return Err(PuzzleError::InvalidSingletonStruct);
                }

                match tree_hash(allocator, program) {
                    DID_INNER_PUZZLE_HASH => {
                        let did_args = DidArgs::<NodePtr, NodePtr>::from_clvm(allocator, args)?;
                        let DidSolution::InnerSpend(p2_solution) =
                            DidSolution::<NodePtr>::from_clvm(
                                allocator,
                                singleton_solution.inner_solution,
                            )?;

                        if did_args.singleton_struct != singleton_args.singleton_struct {
                            return Err(PuzzleError::DidSingletonStructMismatch);
                        }

                        let Reduction(_cost, output) = run_program(
                            allocator,
                            &ChiaDialect::new(0),
                            did_args.inner_puzzle,
                            p2_solution,
                            max_cost,
                        )?;

                        let conditions = Vec::<NodePtr>::from_clvm(allocator, output)?;
                        let mut p2_puzzle_hash = None;

                        for condition in conditions {
                            let Ok(create_coin) =
                                CreateCoinWithMemos::from_clvm(allocator, condition)
                            else {
                                continue;
                            };

                            if create_coin.amount % 2 == 0 {
                                continue;
                            }

                            p2_puzzle_hash = create_coin.memos.first().and_then(|memo| {
                                Some(Bytes32::new(memo.as_ref().try_into().ok()?))
                            });
                            break;
                        }

                        let Some(p2_puzzle_hash) = p2_puzzle_hash else {
                            return Err(PuzzleError::MissingCreateCoin);
                        };

                        let did_inner_puzzle_hash = did_inner_puzzle_hash(
                            p2_puzzle_hash,
                            did_args.recovery_did_list_hash,
                            did_args.num_verifications_required,
                            did_args.singleton_struct.launcher_id,
                            tree_hash(allocator, did_args.metadata).into(),
                        );

                        let singleton_puzzle_hash = singleton_puzzle_hash(
                            singleton_args.singleton_struct.launcher_id,
                            did_inner_puzzle_hash,
                        );

                        if singleton_puzzle_hash != coin.puzzle_hash {
                            return Err(PuzzleError::UnknownDidOutput);
                        }

                        Ok(Puzzle::Did(DidInfo {
                            launcher_id: singleton_args.singleton_struct.launcher_id,
                            coin,
                            p2_puzzle_hash,
                            did_inner_puzzle_hash,
                            recovery_did_list_hash: did_args.recovery_did_list_hash,
                            num_verifications_required: did_args.num_verifications_required,
                            metadata: did_args.metadata,
                            proof: Proof::Lineage(LineageProof {
                                parent_coin_info: parent_coin.parent_coin_info,
                                inner_puzzle_hash: tree_hash(
                                    allocator,
                                    singleton_args.inner_puzzle,
                                )
                                .into(),
                                amount: parent_coin.amount,
                            }),
                        }))
                    }
                    _ => Err(PuzzleError::InvalidPuzzle),
                }
            }
            _ => Err(PuzzleError::InvalidPuzzle),
        }
    }
}

#[cfg(test)]
mod tests {
    use chia_bls::PublicKey;
    use chia_wallet::standard::standard_puzzle_hash;
    use clvm_traits::ToNodePtr;

    use crate::{
        Chainable, CreateCoinWithMemos, CreateDid, IssueCat, Launcher, SpendContext, StandardSpend,
    };

    use super::*;

    #[test]
    fn test_parse_cat() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let pk = PublicKey::default();
        let puzzle_hash = standard_puzzle_hash(&pk).into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 1);

        let (issue_cat, issuance_info) = IssueCat::new(parent.coin_id())
            .condition(ctx.alloc(CreateCoinWithMemos {
                puzzle_hash,
                amount: 1,
                memos: vec![puzzle_hash.to_vec().into()],
            })?)
            .multi_issuance(&mut ctx, pk.clone(), 1)?;

        let cat_info = CatInfo {
            asset_id: issuance_info.asset_id,
            p2_puzzle_hash: puzzle_hash,
            coin: Coin::new(
                issuance_info.eve_coin.coin_id(),
                cat_puzzle_hash(issuance_info.asset_id.into(), puzzle_hash.into()).into(),
                1,
            ),
            lineage_proof: LineageProof {
                parent_coin_info: issuance_info.eve_coin.parent_coin_info,
                inner_puzzle_hash: issuance_info.eve_inner_puzzle_hash,
                amount: 1,
            },
        };

        let standard_spend = StandardSpend::new()
            .chain(issue_cat)
            .finish(&mut ctx, parent, pk)?;

        let coin_spend = standard_spend
            .into_iter()
            .find(|cs| cs.coin.coin_id() == issuance_info.eve_coin.coin_id())
            .unwrap();

        let puzzle = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution = coin_spend.solution.to_node_ptr(&mut allocator)?;

        let parse = Puzzle::parse(
            &mut allocator,
            puzzle,
            solution,
            coin_spend.coin,
            cat_info.coin.clone(),
            u64::MAX,
        )?;

        match parse {
            Puzzle::Cat(parsed_cat_info) => assert_eq!(parsed_cat_info, cat_info),
            _ => panic!("unexpected puzzle"),
        }

        Ok(())
    }

    #[test]
    fn test_parse_did() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let pk = PublicKey::default();
        let puzzle_hash = standard_puzzle_hash(&pk).into();
        let parent = Coin::new(Bytes32::default(), puzzle_hash, 1);

        let (create_did, did_info) = Launcher::new(parent.coin_id(), 1)
            .create(&mut ctx)?
            .create_standard_did(&mut ctx, pk.clone())?;

        let standard_spend = StandardSpend::new()
            .chain(create_did)
            .finish(&mut ctx, parent, pk)?;

        let coin_spend = standard_spend
            .into_iter()
            .find(|cs| cs.coin.coin_id() == did_info.coin.parent_coin_info)
            .unwrap();

        let puzzle = coin_spend.puzzle_reveal.to_node_ptr(&mut allocator)?;
        let solution = coin_spend.solution.to_node_ptr(&mut allocator)?;

        let parse = Puzzle::parse(
            &mut allocator,
            puzzle,
            solution,
            coin_spend.coin,
            did_info.coin.clone(),
            u64::MAX,
        )?;

        match parse {
            Puzzle::Did(parsed_did_info) => assert_eq!(parsed_did_info.with_metadata(()), did_info),
            _ => panic!("unexpected puzzle"),
        }

        Ok(())
    }
}
