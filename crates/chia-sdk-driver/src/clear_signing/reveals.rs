use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::singleton::SingletonArgs;
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use indexmap::IndexMap;

use crate::{
    AssetInfo, BagAsset, ClawbackV2, DriverError, P2ConditionsOrSingleton, Puzzle,
    RequestedPayments, SecureTheBag, Spend, VaultVersion,
};

#[derive(Debug, Clone, Copy)]
pub enum RevealedP2Puzzle {
    Clawback(ClawbackV2),
    P2ConditionsOrSingleton(P2ConditionsOrSingleton),
}

#[derive(Debug, Clone, Copy)]
pub struct RevealedCoinSpend {
    pub coin: Coin,
    pub puzzle: Puzzle,
    pub solution: NodePtr,
}

/// The recovery path that a controlled vault's delegated spend is authorized through, if it isn't authorized through
/// its custody path. The controlling vault signs the delegated puzzle hash wrapped in the recovery path's delegated
/// puzzle wrappers, as calculated by [`vault_recovery_delegated_puzzle_hash`](crate::vault_recovery_delegated_puzzle_hash).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlledVaultRecovery {
    pub left_side_subtree_hash: Bytes32,
    pub recovery_timelock: u64,
    pub version: VaultVersion,
}

impl ControlledVaultRecovery {
    pub fn new(
        left_side_subtree_hash: Bytes32,
        recovery_timelock: u64,
        version: VaultVersion,
    ) -> Self {
        Self {
            left_side_subtree_hash,
            recovery_timelock,
            version,
        }
    }
}

/// The spend of a vault whose custody includes another vault (as a singleton member), which is authorized by a message
/// from the vault that controls it.
#[derive(Debug, Clone, Copy)]
pub struct RevealedControlledVault {
    /// The controlled vault's coin, which is sent the message.
    pub coin: Coin,
    pub launcher_id: Bytes32,
    /// The controlled vault's current custody hash, which the coin's puzzle hash commits to.
    pub custody_hash: Bytes32,
    /// The delegated spend of the controlled vault, whose delegated puzzle hash is the content of the message.
    pub delegated_spend: Spend,
    /// Set if the delegated spend is authorized through the controlled vault's recovery path.
    pub recovery: Option<ControlledVaultRecovery>,
}

#[derive(Debug, Clone)]
pub struct Reveals {
    coin_spends: IndexMap<Bytes32, RevealedCoinSpend>,
    p2_puzzles: HashMap<TreeHash, RevealedP2Puzzle>,
    bags: HashMap<(Bytes32, BagAsset), Arc<SecureTheBag>>,
    requested_payments: RequestedPayments,
    asset_info: AssetInfo,
    vault_nonces: BTreeSet<usize>,
    controlled_vaults: IndexMap<Bytes32, RevealedControlledVault>,
    controlled_vault_nonces: HashMap<Bytes32, BTreeSet<usize>>,
}

impl Default for Reveals {
    fn default() -> Self {
        let mut vault_nonces = BTreeSet::new();

        vault_nonces.insert(0);

        Self {
            coin_spends: IndexMap::new(),
            p2_puzzles: HashMap::new(),
            bags: HashMap::new(),
            requested_payments: RequestedPayments::default(),
            asset_info: AssetInfo::default(),
            vault_nonces,
            controlled_vaults: IndexMap::new(),
            controlled_vault_nonces: HashMap::new(),
        }
    }
}

impl Reveals {
    pub fn from_coin_spends(
        allocator: &mut Allocator,
        coin_spends: &[CoinSpend],
    ) -> Result<Self, DriverError> {
        let mut reveals = Self::default();

        for coin_spend in coin_spends {
            reveals.reveal_coin_spend(allocator, coin_spend)?;
        }

        Ok(reveals)
    }

    /// All coins that are sent messages from the primary vault (the one being signed for) in the transaction, or
    /// from a vault that it controls, must be revealed. Controlled vaults themselves are revealed with
    /// [`reveal_controlled_vault`](Self::reveal_controlled_vault) instead. The coin spend is used to determine both the conditions that the spends output, and
    /// the type of asset being sent.
    ///
    /// In some cases, it's insufficient to only reveal the coin spend. For example, if it's a clawback coin,
    /// you must reveal the clawback itself as well. Otherwise, there's no way to verify if the coin won't
    /// consume the message while doing something other than the delegated puzzle's conditions you expect.
    ///
    /// This also records requested payments (i.e., coin spends with a parent coin id of 32 zeros), which are
    /// used to determine what would be paid to us if the announcement from the settlement puzzle were to be
    /// asserted. Note that requested payments are ignored if they aren't asserted.
    pub fn reveal_coin_spend(
        &mut self,
        allocator: &mut Allocator,
        coin_spend: &CoinSpend,
    ) -> Result<(), DriverError> {
        let puzzle = coin_spend.puzzle_reveal.to_clvm(allocator)?;
        let puzzle = Puzzle::parse(allocator, puzzle);
        let solution = coin_spend.solution.to_clvm(allocator)?;

        // If the coin spend's puzzle doesn't match the coin's puzzle hash, we should return an error.
        // This prevents spoofing what will happen as a result of the coin spend being included in the transaction.
        if coin_spend.coin.puzzle_hash != puzzle.curried_puzzle_hash().into() {
            return Err(DriverError::WrongPuzzleHash);
        }

        if coin_spend.coin.parent_coin_info == Bytes32::default() {
            self.reveal_settlement_payment(allocator, puzzle, solution)?;
        } else {
            self.coin_spends.insert(
                coin_spend.coin.coin_id(),
                RevealedCoinSpend {
                    coin: coin_spend.coin,
                    puzzle,
                    solution,
                },
            );
        }

        Ok(())
    }

    /// Reveals settlement payment spends, which are used to determine what would be paid to us if
    /// the announcement from the settlement puzzle were to be asserted. Note that requested payments
    /// are ignored if they aren't asserted.
    pub fn reveal_settlement_payment(
        &mut self,
        allocator: &mut Allocator,
        outer_puzzle: Puzzle,
        inner_settlement_solution: NodePtr,
    ) -> Result<(), DriverError> {
        self.requested_payments.parse(
            allocator,
            &mut self.asset_info,
            outer_puzzle,
            inner_settlement_solution,
        )?;

        Ok(())
    }

    /// Reveals a clawback, so that we can look it up by p2 puzzle hash.
    pub fn reveal_clawback(&mut self, clawback: ClawbackV2) {
        self.p2_puzzles
            .insert(clawback.tree_hash(), RevealedP2Puzzle::Clawback(clawback));
    }

    /// Reveals a p2 conditions or singleton puzzle, so that we can look it up by p2 puzzle hash.
    pub fn reveal_p2_conditions_or_singleton(&mut self, p2: P2ConditionsOrSingleton) {
        self.p2_puzzles.insert(
            p2.tree_hash(),
            RevealedP2Puzzle::P2ConditionsOrSingleton(p2),
        );
    }

    /// Reveals a Secure the Bag, so that a coin created with its root hash and total amount is shown as the bag's
    /// payments rather than as a payment to an unknown puzzle hash.
    ///
    /// The bag is looked up by the p2 puzzle hash that the created coin's parent outputs (the puzzle hash inside of
    /// the CAT layer for CATs), along with the asset of the coin, so a bag revealed for one asset doesn't match a
    /// coin of another. A bag whose root is a payment rather than a node isn't bagged, so it's ignored and the
    /// payment is shown on its own.
    pub fn reveal_bag(&mut self, bag: impl Into<Arc<SecureTheBag>>) {
        let bag = bag.into();

        if bag.node(bag.root_hash()).is_none() {
            return;
        }

        self.bags.insert((bag.root_hash(), bag.asset()), bag);
    }

    /// Adds a vault nonce to the set of vault nonces to derive p2 puzzle hashes for.
    pub fn reveal_vault_nonce(&mut self, nonce: usize) {
        self.vault_nonces.insert(nonce);
    }

    /// Reveals the spend of a vault that's controlled by another vault in the transaction, which must be revealed if
    /// the vault being signed for (or another controlled vault) sends it a message. The controlled vault and the
    /// spends that it authorizes are then parsed as part of the transaction.
    ///
    /// The coin's puzzle hash must be the singleton puzzle hash for the launcher id and custody hash. Since the coin
    /// id commits to it, and the transaction can only be valid if the coin exists, the custody hash can be trusted
    /// without looking it up.
    pub fn reveal_controlled_vault(
        &mut self,
        controlled_vault: RevealedControlledVault,
    ) -> Result<(), DriverError> {
        let puzzle_hash: Bytes32 = SingletonArgs::curry_tree_hash(
            controlled_vault.launcher_id,
            controlled_vault.custody_hash.into(),
        )
        .into();

        if controlled_vault.coin.puzzle_hash != puzzle_hash {
            return Err(DriverError::WrongControlledVaultPuzzleHash);
        }

        self.controlled_vaults
            .insert(controlled_vault.coin.coin_id(), controlled_vault);

        Ok(())
    }

    /// Adds a vault nonce of a controlled vault, to derive its p2 puzzle hashes. Like the vault being signed for, the
    /// first nonce is included by default.
    pub fn reveal_controlled_vault_nonce(&mut self, launcher_id: Bytes32, nonce: usize) {
        self.controlled_vault_nonces
            .entry(launcher_id)
            .or_default()
            .insert(nonce);
    }

    pub fn requested_payments(&self) -> &RequestedPayments {
        &self.requested_payments
    }

    pub fn asset_info(&self) -> &AssetInfo {
        &self.asset_info
    }

    pub fn coin_spends(&self) -> impl Iterator<Item = &RevealedCoinSpend> {
        self.coin_spends.values()
    }

    pub fn coin_spend(&self, coin_id: Bytes32) -> Option<&RevealedCoinSpend> {
        self.coin_spends.get(&coin_id)
    }

    pub fn p2_puzzle(&self, puzzle_hash: TreeHash) -> Option<&RevealedP2Puzzle> {
        self.p2_puzzles.get(&puzzle_hash)
    }

    pub fn bag(&self, root_hash: Bytes32, asset: BagAsset) -> Option<&SecureTheBag> {
        self.bags.get(&(root_hash, asset)).map(AsRef::as_ref)
    }

    pub fn vault_nonces(&self) -> impl Iterator<Item = usize> {
        self.vault_nonces.iter().copied()
    }

    pub fn controlled_vault(&self, coin_id: Bytes32) -> Option<&RevealedControlledVault> {
        self.controlled_vaults.get(&coin_id)
    }

    /// Switches which vault the nonces are for, so that a controlled vault is parsed with its own p2 puzzle hashes.
    pub(crate) fn use_controlled_vault_nonces(&mut self, launcher_id: Bytes32) {
        let mut nonces = self
            .controlled_vault_nonces
            .get(&launcher_id)
            .cloned()
            .unwrap_or_default();

        nonces.insert(0);

        self.vault_nonces = nonces;
    }
}
