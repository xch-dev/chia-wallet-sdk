use chia_puzzles::{EveProof, Proof};
use chia_sdk_types::Conditions;
use clvm_traits::ToClvm;
use clvmr::Allocator;

use crate::{DriverError, Launcher, SpendContext};

use super::{Member, PuzzleWithRestrictions, Vault, VaultLayer};

impl Launcher {
    pub fn mint_vault<M>(
        self,
        ctx: &mut SpendContext,
        custody: PuzzleWithRestrictions<Member>,
        memos: M,
    ) -> Result<(Conditions, Vault), DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let launcher_coin = self.coin();
        let custody_hash = custody.puzzle_hash();
        let (conditions, coin) = self.spend(ctx, custody_hash.into(), memos)?;
        let vault = Vault {
            coin,
            launcher_id: launcher_coin.coin_id(),
            proof: Proof::Eve(EveProof {
                parent_parent_coin_info: launcher_coin.parent_coin_info,
                parent_amount: launcher_coin.amount,
            }),
            custody,
        };
        Ok((conditions, vault))
    }
}
