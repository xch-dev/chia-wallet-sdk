use chia_puzzle_types::{EveProof, Proof};
use chia_sdk_types::Conditions;
use clvm_traits::ToClvm;
use clvm_utils::TreeHash;
use clvmr::Allocator;

use crate::{DriverError, Launcher, SpendContext};

use super::Vault;

impl Launcher {
    pub fn mint_vault<M>(
        self,
        ctx: &mut SpendContext,
        custody_hash: TreeHash,
        memos: M,
    ) -> Result<(Conditions, Vault), DriverError>
    where
        M: ToClvm<Allocator>,
    {
        let launcher_coin = self.coin();
        let (conditions, coin) = self.spend(ctx, custody_hash.into(), memos)?;
        let vault = Vault::new(
            coin,
            launcher_coin.coin_id(),
            Proof::Eve(EveProof {
                parent_parent_coin_info: launcher_coin.parent_coin_info,
                parent_amount: launcher_coin.amount,
            }),
            custody_hash,
        );
        Ok((conditions, vault))
    }
}
