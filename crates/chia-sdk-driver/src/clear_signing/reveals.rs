use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_puzzle_types::singleton::SingletonArgs;
use clvm_traits::ToClvm;
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};
use indexmap::{IndexMap, IndexSet};

use crate::{
    AssetInfo, BagAsset, ClawbackV2, DriverError, P2ConditionsOrSingleton, Puzzle,
    RequestedPayments, SecureTheBag, Spend, VAULT_AMOUNT, VaultVersion,
};

/// The current custody hash and coin of the vault being signed for, which are revealed with
/// [`Reveals::reveal_signing_vault`].
///
/// These are untrusted until they're checked by [`parse_vault_transaction`](crate::parse_vault_transaction), which
/// returns them as a [`SigningVault`](crate::SigningVault).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevealedSigningVault {
    /// The vault's current custody hash (the singleton's inner puzzle hash).
    pub custody_hash: Bytes32,
    /// The vault's current coin, if it's known.
    pub coin: Option<Coin>,
}

/// A p2 puzzle that can't be identified from its puzzle hash alone, revealed so that it can be parsed.
#[derive(Debug, Clone, Copy)]
pub enum RevealedP2Puzzle {
    /// A clawback, revealed with [`Reveals::reveal_clawback`].
    Clawback(ClawbackV2),
    /// A p2 conditions or singleton puzzle, revealed with [`Reveals::reveal_p2_conditions_or_singleton`].
    P2ConditionsOrSingleton(P2ConditionsOrSingleton),
}

/// A revealed coin spend, whose puzzle has been checked to match the coin's puzzle hash.
#[derive(Debug, Clone, Copy)]
pub struct RevealedCoinSpend {
    /// The coin being spent.
    pub coin: Coin,
    /// The puzzle reveal.
    pub puzzle: Puzzle,
    /// The solution.
    pub solution: NodePtr,
}

/// The recovery path that a controlled vault's delegated spend is authorized through, if it isn't authorized through
/// its custody path. The controlling vault signs the delegated puzzle hash wrapped in the recovery path's delegated
/// puzzle wrappers, as calculated by [`vault_recovery_delegated_puzzle_hash`](crate::vault_recovery_delegated_puzzle_hash).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlledVaultRecovery {
    /// The hash of the left side of the controlled vault's 1-of-2 custody, which recovery must keep.
    pub left_side_subtree_hash: Bytes32,
    /// The timelock that the recovery path requires the right side of the new custody to be restricted by, which
    /// delays completing the recovery.
    pub recovery_timelock: u64,
    /// The version of the controlled vault's puzzle, which determines the recovery path's restrictions.
    pub version: VaultVersion,
}

impl ControlledVaultRecovery {
    /// Creates the recovery path details.
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
    /// The controlled vault's coin, which is sent the message. If the message is sent to its puzzle hash, which is the
    /// case for a fast-forwardable singleton member, the spend can be fast-forwarded onto a newer coin of the vault.
    pub coin: Coin,
    /// The controlled vault's launcher id.
    pub launcher_id: Bytes32,
    /// The controlled vault's current custody hash, which the coin's puzzle hash commits to.
    pub custody_hash: Bytes32,
    /// The delegated spend of the controlled vault, whose delegated puzzle hash is the content of the message.
    pub delegated_spend: Spend,
    /// Set if the delegated spend is authorized through the controlled vault's recovery path.
    pub recovery: Option<ControlledVaultRecovery>,
}

/// Information about a transaction that is revealed to [`parse_vault_transaction`](crate::parse_vault_transaction).
///
/// This typically comes from whoever built the transaction, so it isn't trusted. Coin spends are only accepted
/// if their puzzle matches the coin's puzzle hash, and revealed p2 puzzles and bags are looked up by their tree hash,
/// so they can only be used for coins whose puzzle hash they actually match. Controlled vaults are checked against
/// their coin's puzzle hash. Revealing too little makes parsing fail or leaves things out of the summary, but can't
/// make it report something false.
///
/// The exception is [`reveal_signing_vault`](Self::reveal_signing_vault), whose custody hash and coin can't be checked
/// against anything but the launcher id. They're only guaranteed if the message that's signed is calculated from the
/// returned [`SigningVault`](crate::SigningVault).
///
/// By default, vault nonce 0 is revealed, so the vault's first p2 puzzle hash is known.
#[derive(Debug, Clone)]
pub struct Reveals {
    coin_spends: IndexMap<Bytes32, RevealedCoinSpend>,
    p2_puzzles: HashMap<TreeHash, RevealedP2Puzzle>,
    bags: HashMap<(Bytes32, BagAsset), Arc<SecureTheBag>>,
    requested_payments: RequestedPayments,
    asset_info: AssetInfo,
    nft_owners: HashMap<Bytes32, IndexSet<Option<Bytes32>>>,
    vault_nonces: BTreeSet<usize>,
    controlled_vaults: IndexMap<Bytes32, RevealedControlledVault>,
    signing_vault: Option<RevealedSigningVault>,
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
            nft_owners: HashMap::new(),
            vault_nonces,
            controlled_vaults: IndexMap::new(),
            signing_vault: None,
        }
    }
}

impl Reveals {
    /// Reveals every coin spend with [`Reveals::reveal_coin_spend`], for example the coin spends of a spend
    /// bundle.
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

    /// Reveals a coin spend, or returns [`DriverError::WrongPuzzleHash`] if its puzzle doesn't match the coin.
    ///
    /// All coins that are sent messages from the vault being signed for in the transaction, or from a vault that it
    /// controls, must be revealed. That includes controlled vaults, which must also be revealed with
    /// [`reveal_controlled_vault`](Self::reveal_controlled_vault). The coin spend is used to determine both the
    /// conditions that the spends output, and the type of asset being sent.
    ///
    /// In some cases, it's insufficient to only reveal the coin spend. For example, if it's a clawback coin,
    /// you must reveal the clawback itself as well. Otherwise, there's no way to verify if the coin won't
    /// consume the message while doing something other than the delegated puzzle's conditions you expect.
    ///
    /// This also records requested payments (i.e., coin spends with a parent coin id of 32 zeros), which are
    /// used to determine what would be paid to us if the announcement from the settlement puzzle were to be
    /// asserted. Note that requested payments are ignored if they aren't asserted. These must be settlement
    /// payments of XCH, a CAT, an NFT, or an option contract, or [`DriverError::UnsupportedRequestedAsset`] is
    /// returned.
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
    ///
    /// The current owner of a requested NFT is recorded too, since the settlement coin's puzzle hash (and therefore
    /// the announcement of its payments) commits to it.
    pub fn reveal_settlement_payment(
        &mut self,
        allocator: &mut Allocator,
        outer_puzzle: Puzzle,
        inner_settlement_solution: NodePtr,
    ) -> Result<(), DriverError> {
        if let Some((launcher_id, current_owner)) = self.requested_payments.parse_settlement(
            allocator,
            &mut self.asset_info,
            outer_puzzle,
            inner_settlement_solution,
        )? {
            self.nft_owners
                .entry(launcher_id)
                .or_default()
                .insert(current_owner);
        }

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

    /// Adds a vault nonce to the set of vault nonces to derive p2 puzzle hashes for. They're the nonces of the vault
    /// whose transaction is parsed, which is the vault at the end of the
    /// [`authorization_path`](crate::VaultTransaction::authorization_path) if the vault being signed for authorizes the
    /// spend of a controlled vault.
    pub fn reveal_vault_nonce(&mut self, nonce: usize) {
        self.vault_nonces.insert(nonce);
    }

    /// Reveals the current custody hash of the vault being signed for, and optionally its coin. They're checked against
    /// the vault's launcher id when the transaction is parsed, and returned in
    /// [`VaultTransaction::signing_vault`](crate::VaultTransaction::signing_vault).
    ///
    /// Clear signing can't look them up, so they're only verified by the signature: a key member signs the delegated
    /// puzzle hash with the vault's puzzle hash ([`calculate_vault_puzzle_message`](crate::calculate_vault_puzzle_message))
    /// or coin id ([`calculate_vault_coin_message`](crate::calculate_vault_coin_message)), which commit to the custody
    /// hash. So the message that's signed must be calculated from the returned
    /// [`SigningVault`](crate::SigningVault), rather than from anything else in the request. Then if the custody hash or
    /// coin is wrong, the signature can't be used to spend the vault.
    ///
    /// With the custody hash, the vault being signed for must be recreated with it if it authorizes the spend of a
    /// controlled vault, or signs messages. With the coin, its amount is also checked to be [`VAULT_AMOUNT`].
    pub fn reveal_signing_vault(&mut self, custody_hash: Bytes32, coin: Option<Coin>) {
        self.signing_vault = Some(RevealedSigningVault { custody_hash, coin });
    }

    /// Reveals the spend of a vault that's controlled by another vault in the transaction, which must be revealed if
    /// the vault being signed for (or another controlled vault) sends it a message. The controlled vault is then added
    /// to the [`authorization_path`](crate::VaultTransaction::authorization_path). Its coin spend must also be revealed with
    /// [`reveal_coin_spend`](Self::reveal_coin_spend), so that its puzzle can be checked to be a vault.
    ///
    /// The coin's puzzle hash must be the singleton puzzle hash for the launcher id and custody hash. Since the coin
    /// id commits to it, and the transaction can only be valid if the coin exists, the custody hash is the vault's
    /// current one without looking it up. Whether the custody is controlled by the vault that sends it the message
    /// isn't checked, so see [`VaultTransaction::authorization_path`](crate::VaultTransaction::authorization_path).
    ///
    /// If the controlled vault's singleton member is fast-forwardable, its message is sent to the puzzle hash rather
    /// than the coin id, so the spend can be fast-forwarded onto a newer coin of the vault. The custody hash is still
    /// the vault's in that case, since the coin must have the same puzzle hash. The coin's amount must be
    /// [`VAULT_AMOUNT`], like every vault's.
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

        if controlled_vault.coin.amount != VAULT_AMOUNT {
            return Err(DriverError::InvalidVaultAmount);
        }

        self.controlled_vaults
            .insert(controlled_vault.coin.coin_id(), controlled_vault);

        Ok(())
    }

    /// The notarized payments of every revealed settlement spend, whether or not they're asserted.
    pub fn requested_payments(&self) -> &RequestedPayments {
        &self.requested_payments
    }

    /// The CAT, NFT, and option details learned from the revealed settlement spends.
    pub fn asset_info(&self) -> &AssetInfo {
        &self.asset_info
    }

    /// The current owners (`None` for no owner) that the revealed settlement spends of the NFT with the launcher id
    /// have, in the order they were revealed.
    pub(crate) fn nft_owners(&self, launcher_id: Bytes32) -> impl Iterator<Item = Option<Bytes32>> {
        self.nft_owners
            .get(&launcher_id)
            .into_iter()
            .flatten()
            .copied()
    }

    /// The revealed coin spends, in the order they were revealed. Settlement payment reveals aren't included.
    pub fn coin_spends(&self) -> impl Iterator<Item = &RevealedCoinSpend> {
        self.coin_spends.values()
    }

    /// The revealed spend of the coin, if any.
    pub fn coin_spend(&self, coin_id: Bytes32) -> Option<&RevealedCoinSpend> {
        self.coin_spends.get(&coin_id)
    }

    /// The revealed p2 puzzle with the puzzle hash, if any.
    pub fn p2_puzzle(&self, puzzle_hash: TreeHash) -> Option<&RevealedP2Puzzle> {
        self.p2_puzzles.get(&puzzle_hash)
    }

    /// The revealed bag with the root hash and asset, if any.
    pub fn bag(&self, root_hash: Bytes32, asset: BagAsset) -> Option<&SecureTheBag> {
        self.bags.get(&(root_hash, asset)).map(AsRef::as_ref)
    }

    /// The custody hash and coin revealed for the vault being signed for, if any. These haven't been checked yet.
    pub fn signing_vault(&self) -> Option<RevealedSigningVault> {
        self.signing_vault
    }

    /// The revealed vault nonces, in ascending order.
    pub fn vault_nonces(&self) -> impl Iterator<Item = usize> {
        self.vault_nonces.iter().copied()
    }

    /// The revealed controlled vault whose coin has the id, if any.
    pub fn controlled_vault(&self, coin_id: Bytes32) -> Option<&RevealedControlledVault> {
        self.controlled_vaults.get(&coin_id)
    }

    /// Finds the controlled vault that a message to a puzzle hash is sent to. Since any coin with the puzzle hash could
    /// receive the message, more than one revealed controlled vault with the puzzle hash is an error.
    pub fn controlled_vault_by_puzzle_hash(
        &self,
        puzzle_hash: Bytes32,
    ) -> Result<Option<&RevealedControlledVault>, DriverError> {
        let mut matches = self
            .controlled_vaults
            .values()
            .filter(|controlled_vault| controlled_vault.coin.puzzle_hash == puzzle_hash);

        let controlled_vault = matches.next();

        if matches.next().is_some() {
            return Err(DriverError::DuplicateControlledVault);
        }

        Ok(controlled_vault)
    }
}
