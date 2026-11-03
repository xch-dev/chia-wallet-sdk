use std::collections::{HashMap, HashSet};

use chia_protocol::Bytes32;
use indexmap::IndexMap;

use crate::{Cat, CatSpend, DriverError, Spend, SpendContext};

use super::{BagAsset, BagCoin, BagFormat, BagPayment};

/// A coin created by a node, which is either another node or a payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagChild {
    Node {
        puzzle_hash: Bytes32,
        amount: u64,
    },
    Payment {
        /// The puzzle hash of the coin created for the payment, see [`BagPayment::coin_puzzle_hash`].
        puzzle_hash: Bytes32,
        payment: BagPayment,
    },
}

impl BagChild {
    /// The p2 puzzle hash of the child coin.
    pub fn puzzle_hash(&self) -> Bytes32 {
        match self {
            Self::Node { puzzle_hash, .. } | Self::Payment { puzzle_hash, .. } => *puzzle_hash,
        }
    }

    pub fn amount(&self) -> u64 {
        match self {
            Self::Node { amount, .. } => *amount,
            Self::Payment { payment, .. } => payment.amount,
        }
    }

    pub fn payment(&self) -> Option<&BagPayment> {
        match self {
            Self::Node { .. } => None,
            Self::Payment { payment, .. } => Some(payment),
        }
    }

    pub fn is_clawback(&self) -> bool {
        self.payment()
            .is_some_and(|payment| payment.clawback.is_some())
    }
}

/// A node in the bag, whose coin creates each of its children when spent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagNode {
    pub amount: u64,
    pub children: Vec<BagChild>,
}

/// A tree of nodes that pays out a list of payments, committed to by a single root hash.
///
/// A root coin with the asset's puzzle for [`SecureTheBag::root_hash`] and [`SecureTheBag::total_amount`] secures every
/// payment, since anyone can spend the nodes down to the payments once the root is created.
#[derive(Debug, Clone)]
pub struct SecureTheBag {
    format: BagFormat,
    asset: BagAsset,
    root_hash: Bytes32,
    total_amount: u64,
    depth: usize,
    payments: Vec<BagPayment>,
    nodes: IndexMap<Bytes32, BagNode>,
    parents: HashMap<Bytes32, Bytes32>,
}

impl SecureTheBag {
    /// Builds the bag for a list of payments.
    ///
    /// Fails if the list is empty, if two payments would create the same coin puzzle hash, if a clawback doesn't
    /// match its payment, or if the total amount doesn't fit in a `u64`.
    pub fn build(
        format: BagFormat,
        asset: BagAsset,
        payments: impl IntoIterator<Item = BagPayment>,
    ) -> Result<Self, DriverError> {
        let mut level = Vec::new();
        let mut puzzle_hashes = HashSet::new();
        let mut total_amount = 0u64;

        for payment in payments {
            if let Some(clawback) = payment.clawback
                && (clawback.receiver_puzzle_hash != payment.puzzle_hash
                    || clawback.amount != payment.amount)
            {
                return Err(DriverError::BagClawbackMismatch);
            }

            let puzzle_hash = payment.coin_puzzle_hash();

            if !puzzle_hashes.insert(puzzle_hash) {
                return Err(DriverError::DuplicateBagPayment(puzzle_hash));
            }

            total_amount = total_amount
                .checked_add(payment.amount)
                .ok_or(DriverError::AmountOverflow)?;

            level.push(BagChild::Payment {
                puzzle_hash,
                payment,
            });
        }

        if level.is_empty() {
            return Err(DriverError::EmptyBag);
        }

        let width = format.validate_width(level.len())?;

        format.sort(&mut level);

        let payments = level
            .iter()
            .filter_map(BagChild::payment)
            .copied()
            .collect();

        let mut nodes = IndexMap::new();
        let mut parents = HashMap::new();
        let mut depth = 0;

        while !format.is_root(&level) {
            let mut next_level = Vec::with_capacity(level.len().div_ceil(width));

            for children in format.group(&level, width) {
                let puzzle_hash = format.node_puzzle_hash(children);

                // This can't overflow, since the total amount fits
                let amount = children.iter().map(BagChild::amount).sum();

                for child in children {
                    parents.insert(child.puzzle_hash(), puzzle_hash);
                }

                nodes.insert(
                    puzzle_hash,
                    BagNode {
                        amount,
                        children: children.to_vec(),
                    },
                );

                next_level.push(BagChild::Node {
                    puzzle_hash,
                    amount,
                });
            }

            level = next_level;
            depth += 1;
        }

        Ok(Self {
            format,
            asset,
            root_hash: level[0].puzzle_hash(),
            total_amount,
            depth,
            payments,
            nodes,
            parents,
        })
    }

    pub fn format(&self) -> BagFormat {
        self.format
    }

    pub fn asset(&self) -> BagAsset {
        self.asset
    }

    /// The p2 puzzle hash of the root coin, which is what the bag is funded with.
    /// For a bag of CATs, this is the puzzle hash inside of the CAT layer (and revocation layer).
    pub fn root_hash(&self) -> Bytes32 {
        self.root_hash
    }

    /// The full puzzle hash of the root coin, including the asset's outer puzzle.
    pub fn root_puzzle_hash(&self) -> Bytes32 {
        self.asset.puzzle_hash(self.root_hash)
    }

    pub fn total_amount(&self) -> u64 {
        self.total_amount
    }

    /// The number of levels of nodes, which is the number of spends needed to unroll any payment.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// The payments in the order of the bag.
    pub fn payments(&self) -> &[BagPayment] {
        &self.payments
    }

    /// Every node in the bag, keyed by its p2 puzzle hash, in the order they were built.
    /// Each level comes before the level above it, so the root is last.
    pub fn nodes(&self) -> &IndexMap<Bytes32, BagNode> {
        &self.nodes
    }

    pub fn node(&self, puzzle_hash: Bytes32) -> Option<&BagNode> {
        self.nodes.get(&puzzle_hash)
    }

    /// The node that creates the coin with the given p2 puzzle hash.
    pub fn parent(&self, puzzle_hash: Bytes32) -> Option<Bytes32> {
        self.parents.get(&puzzle_hash).copied()
    }

    /// The p2 puzzle hashes of the nodes that need to be spent to create the coin with the given p2 puzzle hash, starting
    /// with the root. For a payment, this is [`BagPayment::coin_puzzle_hash`].
    ///
    /// Returns [`None`] if the coin isn't part of the bag.
    pub fn path(&self, puzzle_hash: Bytes32) -> Option<Vec<Bytes32>> {
        let mut path = Vec::new();
        let mut current = puzzle_hash;

        while let Some(parent) = self.parent(current) {
            path.push(parent);
            current = parent;
        }

        if current != self.root_hash {
            return None;
        }

        path.reverse();

        Some(path)
    }

    /// The coins from the root coin down to the coin with the given p2 puzzle hash, including both.
    pub fn coin_path(&self, root: BagCoin, puzzle_hash: Bytes32) -> Option<Vec<BagCoin>> {
        let path = self.path(puzzle_hash)?;

        let mut coins = vec![root];
        let mut coin = root;

        // Each node on the path creates the next one, and the last creates the target
        for (index, &node_puzzle_hash) in path.iter().enumerate() {
            let child_puzzle_hash = path.get(index + 1).copied().unwrap_or(puzzle_hash);

            let amount = self
                .node(node_puzzle_hash)?
                .children
                .iter()
                .find(|child| child.puzzle_hash() == child_puzzle_hash)?
                .amount();

            coin = coin.child(child_puzzle_hash, amount);
            coins.push(coin);
        }

        Some(coins)
    }

    /// The p2 spend of a node, which is the same for every asset. It still needs to be wrapped in the asset's outer
    /// puzzle, which [`SecureTheBag::spend_node`] does for a given coin.
    pub fn node_spend(
        &self,
        ctx: &mut SpendContext,
        puzzle_hash: Bytes32,
    ) -> Result<Spend, DriverError> {
        let node = self
            .node(puzzle_hash)
            .ok_or(DriverError::UnknownBagNode(puzzle_hash))?;

        self.format.node_spend(ctx, &node.children)
    }

    /// Spends a node coin, and returns each of the coins it creates.
    pub fn spend_node(
        &self,
        ctx: &mut SpendContext,
        coin: BagCoin,
    ) -> Result<Vec<BagCoin>, DriverError> {
        if !coin.matches(self.asset) {
            return Err(DriverError::IncompatibleAssetInfo);
        }

        let puzzle_hash = coin.p2_puzzle_hash();
        let spend = self.node_spend(ctx, puzzle_hash)?;
        let node = &self.nodes[&puzzle_hash];

        match coin {
            BagCoin::Xch(coin) => ctx.spend(coin, spend)?,
            BagCoin::Cat(cat) => {
                Cat::spend_all(ctx, &[CatSpend::new(cat, spend)])?;
            }
        }

        Ok(node
            .children
            .iter()
            .map(|child| coin.child(child.puzzle_hash(), child.amount()))
            .collect())
    }

    /// Spends each node from the root coin down to the coin with the given p2 puzzle hash, and returns that coin.
    pub fn spend_path(
        &self,
        ctx: &mut SpendContext,
        root: BagCoin,
        puzzle_hash: Bytes32,
    ) -> Result<BagCoin, DriverError> {
        let coins = self
            .coin_path(root, puzzle_hash)
            .ok_or(DriverError::UnknownBagNode(puzzle_hash))?;

        for &coin in &coins[..coins.len() - 1] {
            self.spend_node(ctx, coin)?;
        }

        Ok(coins[coins.len() - 1])
    }

    /// Spends every node in the bag, starting with the root coin, and returns the payment coins.
    ///
    /// For large bags, the spends may not fit in a single block, so consider [`SecureTheBag::spend_node`] instead.
    pub fn unroll(
        &self,
        ctx: &mut SpendContext,
        root: BagCoin,
    ) -> Result<Vec<BagCoin>, DriverError> {
        let mut payments = Vec::with_capacity(self.payments.len());
        let mut stack = vec![root];

        while let Some(coin) = stack.pop() {
            if self.node(coin.p2_puzzle_hash()).is_some() {
                stack.extend(self.spend_node(ctx, coin)?);
            } else {
                payments.push(coin);
            }
        }

        Ok(payments)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use chia_protocol::Coin;
    use chia_puzzle_types::cat::GenesisByCoinIdTailArgs;
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use clvm_utils::ToTreeHash;

    use crate::{ClawbackV2, StandardLayer};

    use super::*;

    fn payments(count: u8) -> Vec<BagPayment> {
        (0..count)
            .map(|i| BagPayment::new(Bytes32::new([i; 32]), u64::from(i) + 1))
            .collect()
    }

    fn clawback(receiver: u8, amount: u64) -> BagPayment {
        BagPayment::from_clawback(ClawbackV2::new(
            Bytes32::new([255; 32]),
            Bytes32::new([receiver; 32]),
            100,
            amount,
            true,
        ))
    }

    fn build(width: Option<usize>, payments: Vec<BagPayment>) -> Result<SecureTheBag, DriverError> {
        SecureTheBag::build(
            BagFormat::Legacy {
                leaf_width: width,
                sorted: true,
            },
            BagAsset::Xch,
            payments,
        )
    }

    #[test]
    fn test_rejects_invalid_payments() {
        assert!(matches!(build(None, vec![]), Err(DriverError::EmptyBag)));

        let mut duplicate = payments(3);
        duplicate.push(BagPayment::new(Bytes32::new([1; 32]), 5));
        assert!(matches!(
            build(None, duplicate),
            Err(DriverError::DuplicateBagPayment(puzzle_hash)) if puzzle_hash == Bytes32::new([1; 32])
        ));

        assert!(matches!(
            build(None, vec![clawback(1, 5), clawback(1, 5)]),
            Err(DriverError::DuplicateBagPayment(_))
        ));

        let overflow = vec![
            BagPayment::new(Bytes32::new([1; 32]), u64::MAX),
            BagPayment::new(Bytes32::new([2; 32]), 1),
        ];
        assert!(matches!(
            build(None, overflow),
            Err(DriverError::AmountOverflow)
        ));

        let mut wrong_receiver = clawback(1, 5);
        wrong_receiver.puzzle_hash = Bytes32::new([2; 32]);
        assert!(matches!(
            build(None, vec![wrong_receiver]),
            Err(DriverError::BagClawbackMismatch)
        ));

        let mut wrong_amount = clawback(1, 5);
        wrong_amount.amount = 6;
        assert!(matches!(
            build(None, vec![wrong_amount]),
            Err(DriverError::BagClawbackMismatch)
        ));
    }

    #[test]
    fn test_rejects_invalid_width() {
        assert!(matches!(
            build(Some(0), payments(1)),
            Err(DriverError::InvalidBagWidth { width: 0, .. })
        ));
        assert!(matches!(
            build(Some(1), payments(2)),
            Err(DriverError::InvalidBagWidth { width: 1, .. })
        ));
        assert!(build(Some(1), vec![clawback(1, 5)]).is_ok());
    }

    #[test]
    fn test_total_can_be_max() -> Result<()> {
        let bag = build(
            None,
            vec![
                BagPayment::new(Bytes32::new([1; 32]), u64::MAX - 1),
                BagPayment::new(Bytes32::new([2; 32]), 1),
            ],
        )?;
        assert_eq!(bag.total_amount(), u64::MAX);
        Ok(())
    }

    #[test]
    fn test_receiver_with_and_without_clawback() -> Result<()> {
        let bag = build(
            None,
            vec![BagPayment::new(Bytes32::new([1; 32]), 5), clawback(1, 5)],
        )?;
        assert_eq!(bag.payments().len(), 2);
        assert_eq!(bag.total_amount(), 10);
        Ok(())
    }

    #[test]
    fn test_single_payment() -> Result<()> {
        let bag = build(None, payments(1))?;
        assert_eq!(bag.root_hash(), Bytes32::new([0; 32]));
        assert_eq!(bag.depth(), 0);
        assert!(bag.nodes().is_empty());
        assert_eq!(bag.path(bag.root_hash()), Some(vec![]));

        let payment = clawback(1, 5);
        let bag = build(None, vec![payment])?;
        assert_eq!(bag.depth(), 1);
        assert_eq!(bag.nodes().len(), 1);
        assert_eq!(
            bag.path(payment.coin_puzzle_hash()),
            Some(vec![bag.root_hash()])
        );
        assert_eq!(
            payment.coin_puzzle_hash(),
            Bytes32::from(payment.clawback.unwrap().tree_hash())
        );

        Ok(())
    }

    #[test]
    fn test_sorted_into_levels() -> Result<()> {
        let mut input = payments(10);
        input.reverse();

        let bag = build(Some(3), input)?;
        assert_eq!(bag.payments(), payments(10));
        assert_eq!(bag.depth(), 3);
        assert_eq!(bag.nodes().len(), 4 + 2 + 1);

        let root = bag.node(bag.root_hash()).unwrap();
        assert_eq!(root.amount, 55);
        assert_eq!(root.children.len(), 2);

        let path = bag.path(Bytes32::new([9; 32])).unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], bag.root_hash());
        assert_eq!(
            bag.node(path[2]).unwrap().children,
            vec![BagChild::Payment {
                puzzle_hash: Bytes32::new([9; 32]),
                payment: BagPayment::new(Bytes32::new([9; 32]), 10),
            }]
        );

        assert_eq!(bag.path(Bytes32::new([10; 32])), None);

        // Nodes are built level by level, so the root comes last
        assert_eq!(
            bag.nodes().last().map(|(hash, _)| *hash),
            Some(bag.root_hash())
        );

        let mut ctx = SpendContext::new();
        for &puzzle_hash in bag.nodes().keys() {
            let spend = bag.node_spend(&mut ctx, puzzle_hash)?;
            assert_eq!(Bytes32::from(ctx.tree_hash(spend.puzzle)), puzzle_hash);
        }
        assert!(matches!(
            bag.node_spend(&mut ctx, Bytes32::new([9; 32])),
            Err(DriverError::UnknownBagNode(_))
        ));

        Ok(())
    }

    #[test]
    fn test_spend_path_xch() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let bag = build(Some(3), payments(10))?;
        let alice = sim.bls(bag.total_amount());

        let hint = ctx.hint(bag.root_hash())?;
        StandardLayer::new(alice.pk).spend(
            &mut ctx,
            alice.coin,
            Conditions::new().create_coin(bag.root_hash(), bag.total_amount(), hint),
        )?;
        let root = BagCoin::Xch(Coin::new(
            alice.coin.coin_id(),
            bag.root_hash(),
            bag.total_amount(),
        ));

        let target = Bytes32::new([4; 32]);
        let coins = bag.coin_path(root, target).unwrap();
        assert_eq!(coins.len(), 4);

        let payment = bag.spend_path(&mut ctx, root, target)?;
        assert_eq!(payment, coins[3]);
        assert_eq!(payment.coin().amount, 5);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        let coin_id = payment.coin().coin_id();
        assert!(sim.coin_state(coin_id).is_some());
        assert_eq!(sim.hinted_coins(target), vec![coin_id]);

        // Its siblings are created too, but payments in other leaves aren't
        assert_eq!(sim.hinted_coins(Bytes32::new([3; 32])).len(), 1);
        assert!(sim.hinted_coins(Bytes32::new([0; 32])).is_empty());

        Ok(())
    }

    #[test]
    fn test_unroll_revocable_cat() -> Result<()> {
        let mut sim = Simulator::new();
        let mut ctx = SpendContext::new();

        let alice = sim.bls(1000);
        let asset = BagAsset::RevocableCat {
            asset_id: GenesisByCoinIdTailArgs::curry_tree_hash(alice.coin.coin_id()).into(),
            hidden_puzzle_hash: alice.puzzle_hash,
        };

        let mut input = payments(5);
        input.push(clawback(9, 985));

        let bag = SecureTheBag::build(
            BagFormat::Legacy {
                leaf_width: Some(2),
                sorted: true,
            },
            asset,
            input,
        )?;

        let hint = ctx.hint(bag.root_hash())?;
        let (issue, cats) = Cat::single_issuance(
            &mut ctx,
            alice.coin.coin_id(),
            Some(alice.puzzle_hash),
            1000,
            Conditions::new().create_coin(bag.root_hash(), 1000, hint),
        )?;
        StandardLayer::new(alice.pk).spend(&mut ctx, alice.coin, issue)?;

        let root = BagCoin::Cat(cats[0]);
        assert_eq!(root.coin().puzzle_hash, bag.root_puzzle_hash());

        // The coin must match the asset and be a node
        assert!(matches!(
            bag.spend_node(&mut ctx, BagCoin::Xch(root.coin())),
            Err(DriverError::IncompatibleAssetInfo)
        ));
        let payment_coin = root.child(Bytes32::new([0; 32]), 1);
        assert!(matches!(
            bag.spend_node(&mut ctx, payment_coin),
            Err(DriverError::UnknownBagNode(_))
        ));

        let coins = bag.unroll(&mut ctx, root)?;
        assert_eq!(coins.len(), 6);

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        for payment in bag.payments() {
            let coin = coins
                .iter()
                .find(|coin| coin.p2_puzzle_hash() == payment.coin_puzzle_hash())
                .unwrap();
            assert_eq!(
                coin.coin().puzzle_hash,
                asset.puzzle_hash(payment.coin_puzzle_hash())
            );
            assert_eq!(coin.coin().amount, payment.amount);
            assert!(sim.coin_state(coin.coin().coin_id()).is_some());
            assert!(
                sim.hinted_coins(payment.puzzle_hash)
                    .contains(&coin.coin().coin_id())
            );
        }

        Ok(())
    }
}
