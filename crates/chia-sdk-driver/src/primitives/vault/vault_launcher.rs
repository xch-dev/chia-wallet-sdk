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
        let launcher_id = self.coin().coin_id();
        let custody_hash = custody.puzzle_hash();
        let (conditions, coin) = self.spend(ctx, custody_hash.into(), memos)?;
        let vault = Vault::new(coin, launcher_id, custody);
        Ok((conditions, vault))
    }
}
