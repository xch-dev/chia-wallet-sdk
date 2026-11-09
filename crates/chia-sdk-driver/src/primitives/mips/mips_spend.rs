use std::collections::HashMap;

use clvm_utils::TreeHash;

use crate::{DriverError, Spend, SpendContext};

use super::InnerPuzzleSpend;

/// Everything needed to spend a MIPS custody puzzle, such as the inner puzzle of a [`Vault`](crate::Vault).
///
/// A MIPS puzzle is a tree of inner puzzles. The top level puzzle runs the [`delegated`](Self::delegated)
/// spend to get its output conditions, and each member in the tree authorizes the delegated puzzle
/// hash. To build the spend, add an [`InnerPuzzleSpend`] for each inner puzzle that will be spent
/// (keyed by its MIPS puzzle hash), and a spend for each restriction (keyed by its puzzle hash).
#[derive(Debug, Clone)]
pub struct MipsSpend {
    /// The delegated spend, whose output conditions are the conditions of the custody puzzle.
    pub delegated: Spend,
    /// The spends of the inner puzzles in the tree, keyed by their MIPS puzzle hash
    /// (see [`mips_puzzle_hash`](crate::mips_puzzle_hash)).
    pub members: HashMap<TreeHash, InnerPuzzleSpend>,
    /// The spends of restrictions and delegated puzzle wrappers, keyed by their puzzle hash.
    pub restrictions: HashMap<TreeHash, Spend>,
}

impl MipsSpend {
    /// Creates a new [`MipsSpend`] with the given delegated spend, and no members or restrictions.
    pub fn new(delegated_spend: Spend) -> Self {
        Self {
            delegated: delegated_spend,
            members: HashMap::new(),
            restrictions: HashMap::new(),
        }
    }

    /// Constructs the spend of the top level MIPS puzzle with the given custody hash.
    ///
    /// Returns [`DriverError::MissingSubpathSpend`] if a needed member or restriction spend is missing.
    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        custody_hash: TreeHash,
    ) -> Result<Spend, DriverError> {
        self.members
            .get(&custody_hash)
            .ok_or(DriverError::MissingSubpathSpend)?
            .spend(ctx, self, &mut Vec::new(), true)
    }
}
