use chia_bls::{sign, PublicKey, SecretKey, Signature};
use chia_protocol::{Bytes, CoinSpend};
use clvm_traits::{FromClvm, Result, ToClvm};
use clvmr::{run_program, Allocator, ChiaDialect};
use itertools::Itertools;
use sha2::{
    digest::{Digest, FixedOutput},
    Sha256,
};

use crate::{u64_to_bytes, Condition};

struct AggSigInfo {
    public_key: PublicKey,
    message: Bytes,
    additional_info: Vec<u8>,
    domain_string: Option<[u8; 32]>,
}

pub fn partial_sign_coin_spends(
    allocator: &mut Allocator,
    coin_spends: &[CoinSpend],
    secret_keys: &[SecretKey],
    agg_sig_me_extra_data: [u8; 32],
) -> Result<Signature> {
    let mut aggregate_signature = Signature::default();
    let dialect = ChiaDialect::new(0);
    let public_keys = secret_keys
        .iter()
        .map(|secret_key| secret_key.public_key())
        .collect_vec();

    for coin_spend in coin_spends {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
        let solution = coin_spend.solution.to_clvm(allocator)?;
        let output = run_program(allocator, &dialect, puzzle, solution, u64::MAX)?.1;

        let conditions = Vec::<Condition>::from_clvm(allocator, output)?;
        for condition in conditions {
            let mut hasher = Sha256::new();
            hasher.update(agg_sig_me_extra_data);

            let info = match condition {
                Condition::AggSigParent {
                    public_key,
                    message,
                } => {
                    hasher.update([43]);
                    let parent = &coin_spend.coin.parent_coin_info;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: parent.to_vec(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigPuzzle {
                    public_key,
                    message,
                } => {
                    hasher.update([44]);
                    let puzzle = &coin_spend.coin.puzzle_hash;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: puzzle.to_vec(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigAmount {
                    public_key,
                    message,
                } => {
                    hasher.update([45]);
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: u64_to_bytes(coin_spend.coin.amount),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigPuzzleAmount {
                    public_key,
                    message,
                } => {
                    hasher.update([46]);
                    let puzzle = &coin_spend.coin.puzzle_hash;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: [puzzle.to_vec(), u64_to_bytes(coin_spend.coin.amount)]
                            .concat(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigParentAmount {
                    public_key,
                    message,
                } => {
                    hasher.update([47]);
                    let parent = &coin_spend.coin.parent_coin_info;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: [parent.to_vec(), u64_to_bytes(coin_spend.coin.amount)]
                            .concat(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigParentPuzzle {
                    public_key,
                    message,
                } => {
                    hasher.update([48]);
                    let parent = &coin_spend.coin.parent_coin_info;
                    let puzzle = &coin_spend.coin.puzzle_hash;
                    AggSigInfo {
                        public_key,
                        message,
                        additional_info: [parent.to_vec(), puzzle.to_vec()].concat(),
                        domain_string: Some(hasher.finalize_fixed().into()),
                    }
                }
                Condition::AggSigUnsafe {
                    public_key,
                    message,
                } => AggSigInfo {
                    public_key,
                    message,
                    additional_info: Vec::new(),
                    domain_string: None,
                },
                Condition::AggSigMe {
                    public_key,
                    message,
                } => AggSigInfo {
                    public_key,
                    message,
                    additional_info: coin_spend.coin.coin_id().into(),
                    domain_string: Some(agg_sig_me_extra_data),
                },
                _ => continue,
            };

            let Some(index) = public_keys.iter().position(|pk| pk == &info.public_key) else {
                continue;
            };

            let mut message = Vec::from(info.message.as_ref());
            message.extend(info.additional_info);
            if let Some(domain_string) = info.domain_string {
                message.extend(domain_string);
            }

            let signature = sign(&secret_keys[index], message);
            aggregate_signature.aggregate(&signature);
        }
    }

    Ok(aggregate_signature)
}
