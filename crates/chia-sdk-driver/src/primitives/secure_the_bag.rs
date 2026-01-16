use std::{
    cmp::Reverse,
    collections::{HashMap, VecDeque},
};

use chia_protocol::Bytes32;
use chia_puzzle_types::Memos;
use chia_sdk_types::conditions::CreateCoin;
use chia_sha2::Sha256;
use clvm_traits::clvm_quote;
use clvm_utils::{ToTreeHash, TreeHash};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BagPayment {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StructureAlgorithm {
    MinimizeIntermediateCoins,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DistributionAlgorithm {
    Striped,
    Naive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BagOptions {
    pub bag_width: usize,
    pub structure_algorithm: StructureAlgorithm,
    pub distribution_algorithm: DistributionAlgorithm,
}

impl Default for BagOptions {
    fn default() -> Self {
        Self {
            bag_width: 10,
            structure_algorithm: StructureAlgorithm::MinimizeIntermediateCoins,
            distribution_algorithm: DistributionAlgorithm::Striped,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagNode {
    Leaf(BagPayment),
    Branch { puzzle_hash: Bytes32, amount: u64 },
}

#[derive(Debug, Clone)]
pub struct SecureTheBag {
    root_hash: Bytes32,
    branches: HashMap<Bytes32, Vec<BagNode>>,
}

impl SecureTheBag {
    pub fn new(mut payments: Vec<BagPayment>, options: BagOptions) -> Self {
        payments.sort_by_key(|payment| (Reverse(payment.amount), payment.puzzle_hash));

        match options.structure_algorithm {
            StructureAlgorithm::MinimizeIntermediateCoins => {
                let bag_levels = calculate_bag_levels(payments.len(), options);

                let mut branches = HashMap::new();

                let (root_hash, _) = compute_striped_bag(
                    &payments,
                    &bag_levels,
                    &mut branches,
                    &options.distribution_algorithm,
                    &mut 0,
                );

                Self {
                    root_hash,
                    branches,
                }
            }
        }
    }

    pub fn root_hash(&self) -> Bytes32 {
        self.root_hash
    }

    pub fn branch(&self, hash: Bytes32) -> Option<&[BagNode]> {
        self.branches.get(&hash).map(Vec::as_slice)
    }
}

trait SelectLeaf {
    fn select_leaf(
        &self,
        payments: &[BagPayment],
        width: usize,
        index: usize,
    ) -> Option<BagPayment>;
}

impl SelectLeaf for DistributionAlgorithm {
    fn select_leaf(
        &self,
        payments: &[BagPayment],
        width: usize,
        index: usize,
    ) -> Option<BagPayment> {
        match self {
            DistributionAlgorithm::Striped => {
                if index >= payments.len() || width == 0 {
                    return None;
                }

                let rows = payments.len().div_ceil(width);
                let remainder = payments.len() % width;
                let mut offset = index;

                for col in 0..width {
                    let col_len = if remainder == 0 || col < remainder {
                        rows
                    } else {
                        rows.saturating_sub(1)
                    };

                    if offset < col_len {
                        let payment_index = offset * width + col;
                        return payments.get(payment_index).copied();
                    }

                    offset -= col_len;
                }

                None
            }
            DistributionAlgorithm::Naive => {
                if index >= payments.len() || width == 0 {
                    return None;
                }

                payments.get(index).copied()
            }
        }
    }
}

fn compute_striped_bag<S>(
    payments: &[BagPayment],
    bag_levels: &[usize],
    branches: &mut HashMap<Bytes32, Vec<BagNode>>,
    select_leaf: &S,
    leaf_index: &mut usize,
) -> (Bytes32, u64)
where
    S: SelectLeaf,
{
    let mut conditions = Vec::<CreateCoin<TreeHash>>::new();
    let mut branch = Vec::new();
    let mut total_amount = 0;

    let width = bag_levels[0];

    for _ in 0..width {
        let payment = if bag_levels.len() == 1 {
            let Some(payment) = select_leaf.select_leaf(payments, width, *leaf_index) else {
                break;
            };

            *leaf_index += 1;
            branch.push(BagNode::Leaf(payment));
            payment
        } else {
            let (branch_hash, amount) = compute_striped_bag(
                payments,
                &bag_levels[1..],
                branches,
                select_leaf,
                leaf_index,
            );
            branch.push(BagNode::Branch {
                puzzle_hash: branch_hash,
                amount,
            });
            BagPayment {
                puzzle_hash: branch_hash,
                amount,
            }
        };

        total_amount += payment.amount;

        conditions.push(CreateCoin {
            puzzle_hash: payment.puzzle_hash,
            amount: payment.amount,
            memos: Memos::None,
        });

        if *leaf_index >= payments.len() {
            break;
        }
    }

    let branch_hash = clvm_quote!(conditions).tree_hash().into();

    branches.insert(branch_hash, branch);

    (branch_hash, total_amount)
}

fn calculate_bag_levels(total: usize, options: BagOptions) -> Vec<usize> {
    let mut current_size = total;
    let mut bag_levels = VecDeque::new();

    while current_size > options.bag_width {
        let new_size = current_size.div_ceil(options.bag_width);
        bag_levels.push_front(current_size.div_ceil(new_size));
        current_size = new_size;
    }

    bag_levels.push_front(current_size);

    debug_assert!(bag_levels.iter().product::<usize>() >= total);

    Vec::from(bag_levels)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_bls::Signature;
    use chia_protocol::{Coin, SpendBundle};
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use indexmap::{IndexMap, IndexSet};
    use rstest::rstest;
    use std::time::Instant;

    use crate::{SpendContext, StandardLayer, spend_bundle_cost};

    use super::*;

    #[test]
    fn test_spec_bag_levels() {
        assert_eq!(calculate_bag_levels(5000, BagOptions::default()), [50, 100]);
        assert_eq!(
            calculate_bag_levels(10_001, BagOptions::default()),
            [2, 51, 100]
        );
        assert_eq!(
            calculate_bag_levels(100_000, BagOptions::default()),
            [10, 100, 100]
        );
    }

    #[test]
    fn test_edge_case_bag_levels() {
        assert_eq!(calculate_bag_levels(0, BagOptions::default()), [0]);
        assert_eq!(calculate_bag_levels(1, BagOptions::default()), [1]);
        assert_eq!(calculate_bag_levels(99, BagOptions::default()), [99]);
        assert_eq!(calculate_bag_levels(100, BagOptions::default()), [100]);
        assert_eq!(calculate_bag_levels(101, BagOptions::default()), [2, 51]);
        assert_eq!(
            calculate_bag_levels(10000, BagOptions::default()),
            [100, 100]
        );
        assert_eq!(calculate_bag_levels(9000, BagOptions::default()), [90, 100]);
        assert_eq!(calculate_bag_levels(6527, BagOptions::default()), [66, 99]);
        assert_eq!(
            calculate_bag_levels(1_000_000, BagOptions::default()),
            [100, 100, 100]
        );
        assert_eq!(
            calculate_bag_levels(100_000_000, BagOptions::default()),
            [100, 100, 100, 100]
        );
        assert_eq!(calculate_bag_levels(298, BagOptions::default()), [3, 100]);
    }

    #[test]
    fn test_low_bag_width() {
        assert_eq!(
            calculate_bag_levels(
                10000,
                BagOptions {
                    bag_width: 10,
                    ..BagOptions::default()
                }
            ),
            [10, 10, 10, 10]
        );
        assert_eq!(
            calculate_bag_levels(
                10000,
                BagOptions {
                    bag_width: 3,
                    ..BagOptions::default()
                }
            ),
            [2, 3, 3, 3, 3, 3, 3, 3, 3]
        );
    }

    #[test]
    fn test_striped_select_leaf_order() {
        let payments = (0..8)
            .map(|i| BagPayment {
                puzzle_hash: Bytes32::from([0; 32]),
                amount: i,
            })
            .collect::<Vec<_>>();

        let algorithm = DistributionAlgorithm::Striped;
        let width = 3usize;

        let selected = (0..payments.len())
            .filter_map(|idx| algorithm.select_leaf(&payments, width, idx))
            .map(|payment| payment.amount)
            .collect::<Vec<_>>();

        assert_eq!(selected, vec![0, 3, 6, 1, 4, 7, 2, 5]);
    }

    #[rstest]
    fn test_secure_the_bag(
        #[values(5, 10, 20, 100)] bag_width: usize,
        #[values(1, 1000, 1_000_000)] payment_count: u64,
    ) -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let mut payments = Vec::new();
        let mut puzzle_hashes = IndexSet::new();
        let mut total_amount = 0;

        let start_time = Instant::now();

        for i in 0..payment_count {
            let puzzle_hash: Bytes32 = i.tree_hash().into();

            payments.push(BagPayment {
                puzzle_hash,
                amount: i,
            });

            puzzle_hashes.insert(puzzle_hash);
            total_amount += i;
        }

        let end_time = Instant::now();
        println!(
            "Time taken initialization: {:?}",
            end_time.duration_since(start_time)
        );

        let alice = sim.bls(total_amount);

        let start_time = Instant::now();
        let bag = SecureTheBag::new(
            payments,
            BagOptions {
                bag_width,
                ..BagOptions::default()
            },
        );

        let end_time = Instant::now();
        println!(
            "Time taken bag construction: {:?}",
            end_time.duration_since(start_time)
        );

        StandardLayer::new(alice.pk).spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(bag.root_hash(), total_amount, Memos::None),
        )?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let bag_coin = Coin::new(alice.coin.coin_id(), bag.root_hash(), total_amount);

        let mut bag_coins = vec![bag_coin];
        let mut unique_amounts = IndexMap::new();

        let start_time = Instant::now();

        let mut costs = Vec::new();

        while let Some(bag_coin) = bag_coins.pop() {
            let nodes = bag.branch(bag_coin.puzzle_hash).unwrap();

            let mut conditions = Conditions::new();

            for node in nodes {
                match node {
                    BagNode::Branch {
                        puzzle_hash,
                        amount,
                    } => {
                        bag_coins.push(Coin::new(bag_coin.coin_id(), *puzzle_hash, *amount));
                        conditions = conditions.create_coin(*puzzle_hash, *amount, Memos::None);
                    }
                    BagNode::Leaf(payment) => {
                        conditions = conditions.create_coin(
                            payment.puzzle_hash,
                            payment.amount,
                            Memos::None,
                        );
                        *unique_amounts.entry(payment.amount).or_insert(0) += 1;
                    }
                }
            }

            let spend = ctx.delegated_spend(conditions)?;
            ctx.spend(bag_coin, spend)?;

            let spend_bundle = SpendBundle::new(ctx.take(), Signature::default());
            let cost = spend_bundle_cost(&spend_bundle.coin_spends)?;

            sim.new_transaction(spend_bundle)?;
            costs.push(cost);
        }

        let total_cost = costs.iter().sum::<u64>();
        println!("total_cost for leaf size {bag_width}: {total_cost}");

        let end_time = Instant::now();
        println!(
            "Time taken spending: {:?}",
            end_time.duration_since(start_time)
        );

        // println!("unique_amounts: {unique_amounts:?}");

        let payment_count = puzzle_hashes.len();

        assert_eq!(
            sim.lookup_puzzle_hashes(puzzle_hashes, false).len(),
            payment_count
        );
        assert_eq!(unique_amounts.len(), payment_count);

        Ok(())
    }
}
