use std::collections::{HashMap, HashSet};

use chia_protocol::Bytes32;

use crate::{Cat, CatSpend, DriverError, SpendContext};

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
    nodes: HashMap<Bytes32, BagNode>,
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

        let mut nodes = HashMap::new();
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

    /// Every node in the bag, keyed by its p2 puzzle hash.
    pub fn nodes(&self) -> &HashMap<Bytes32, BagNode> {
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

        let node = self
            .node(puzzle_hash)
            .ok_or(DriverError::UnknownBagNode(puzzle_hash))?;

        let spend = self.format.node_spend(ctx, &node.children)?;

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
