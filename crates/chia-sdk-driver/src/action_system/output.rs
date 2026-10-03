use chia_protocol::Bytes32;

/// A coin created by a spend, identified by its puzzle hash and amount.
///
/// The parent coin id is implied by the spend that creates it. A spend can't create two coins with
/// the same puzzle hash and amount, since they would have the same coin id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Output {
    /// The puzzle hash in the `CREATE_COIN` condition. For CATs and singletons, this is the inner
    /// puzzle hash that the outer layers wrap.
    pub puzzle_hash: Bytes32,
    pub amount: u64,
}

impl Output {
    pub fn new(puzzle_hash: Bytes32, amount: u64) -> Self {
        Self {
            puzzle_hash,
            amount,
        }
    }
}

/// Restrictions on the amounts of the coins that a spend can create, depending on the asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputConstraints {
    /// The spend is of a singleton, so any odd amount would be treated as the singleton itself.
    pub singleton: bool,
    /// The spend is of a settlement coin, which never creates coins with an amount of zero.
    pub settlement: bool,
}

/// The coins that a spend has created so far, used to avoid creating duplicates.
pub trait OutputSet {
    /// Whether the spend already creates a coin with the same puzzle hash and amount.
    fn has_output(&self, output: &Output) -> bool;

    /// Whether the spend can run a TAIL, since each CAT spend can only run one.
    fn can_run_cat_tail(&self) -> bool;

    /// Whether the spend of a singleton has yet to recreate or melt the singleton.
    fn missing_singleton_output(&self) -> bool;

    /// The smallest amount at which the spend can create a new coin with the given puzzle hash.
    /// This is used for intermediate coins and launchers, where the amount doesn't matter.
    fn find_amount(
        &self,
        puzzle_hash: Bytes32,
        output_constraints: &OutputConstraints,
    ) -> Option<u64> {
        (0..u64::MAX)
            .find(|amount| self.is_allowed(&Output::new(puzzle_hash, *amount), output_constraints))
    }

    /// Whether the spend can create the output without duplicating an existing coin or violating
    /// the [`OutputConstraints`].
    fn is_allowed(&self, output: &Output, output_constraints: &OutputConstraints) -> bool {
        if output_constraints.singleton && output.amount % 2 == 1 {
            return false;
        }

        if output_constraints.settlement && output.amount == 0 {
            return false;
        }

        !self.has_output(output)
    }
}
