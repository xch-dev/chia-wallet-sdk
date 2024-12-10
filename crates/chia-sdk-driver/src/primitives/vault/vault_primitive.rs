use chia_protocol::{Bytes32, Coin};
use chia_puzzles::singleton::SingletonArgs;
use chia_sdk_types::Mod;
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

use super::{KnownPuzzles, Member, PuzzleWithRestrictions, VaultLayer};

#[derive(Debug, Clone)]
pub struct Vault {
    pub coin: Coin,
    pub launcher_id: Bytes32,
    pub custody: PuzzleWithRestrictions<Member>,
}

impl Vault {
    pub fn new(coin: Coin, launcher_id: Bytes32, custody: PuzzleWithRestrictions<Member>) -> Self {
        Self {
            coin,
            launcher_id,
            custody,
        }
    }
}

impl VaultLayer for Vault {
    fn puzzle_hash(&self) -> TreeHash {
        SingletonArgs::new(self.launcher_id, self.custody.puzzle_hash()).curry_tree_hash()
    }

    fn puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let puzzle = self.custody.puzzle(ctx)?;
        ctx.curry(SingletonArgs::new(self.launcher_id, puzzle))
    }

    fn replace(self, known_puzzles: &KnownPuzzles) -> Self {
        Self {
            coin: self.coin,
            launcher_id: self.launcher_id,
            custody: self.custody.replace(known_puzzles),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chia_sdk_test::Simulator;
    use chia_sdk_types::BlsMember;

    use crate::{Launcher, MemberKind, SpendContext, StandardLayer};

    use super::*;

    #[test]
    fn test_single_sig() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let (sk, pk, _puzzle_hash, coin) = sim.new_p2(1)?;
        let p2 = StandardLayer::new(pk);

        let hidden_member = BlsMember::new(pk);

        let custody = PuzzleWithRestrictions::top_level(
            0,
            Vec::new(),
            Member::unknown(hidden_member.curry_tree_hash()),
        );
        let (mint_vault, vault) = Launcher::new(coin.coin_id(), 1).mint_vault(ctx, custody, ())?;
        p2.spend(ctx, coin, mint_vault)?;
        sim.spend_coins(ctx.take(), &[sk])?;

        let mut known_members = HashMap::new();
        known_members.insert(
            hidden_member.curry_tree_hash(),
            MemberKind::Bls(hidden_member),
        );
        let _vault = vault.replace(&KnownPuzzles {
            members: known_members,
            ..Default::default()
        });

        Ok(())
    }
}
