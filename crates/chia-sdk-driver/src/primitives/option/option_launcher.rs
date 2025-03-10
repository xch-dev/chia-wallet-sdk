use chia_protocol::Bytes32;
use chia_puzzle_types::{EveProof, Proof};
use chia_sdk_types::Conditions;
use clvm_traits::clvm_quote;
use clvmr::NodePtr;

use crate::{DriverError, Launcher, Spend, SpendContext};

use super::{OptionContract, OptionInfo, OptionMetadata};

impl Launcher {
    pub fn mint_eve_option(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
        underlying_coin_id: Bytes32,
        underlying_delegated_puzzle_hash: Bytes32,
        metadata: OptionMetadata,
    ) -> Result<(Conditions, OptionContract), DriverError> {
        let launcher_coin = self.coin();

        let option_info = OptionInfo::new(
            launcher_coin.coin_id(),
            underlying_coin_id,
            underlying_delegated_puzzle_hash,
            p2_puzzle_hash,
        );

        let inner_puzzle_hash = option_info.inner_puzzle_hash();
        let (launch_singleton, eve_coin) = self.spend(ctx, inner_puzzle_hash.into(), metadata)?;

        let proof = Proof::Eve(EveProof {
            parent_parent_coin_info: launcher_coin.parent_coin_info,
            parent_amount: launcher_coin.amount,
        });

        Ok((
            launch_singleton,
            OptionContract::new(eve_coin, proof, option_info),
        ))
    }

    pub fn mint_option(
        self,
        ctx: &mut SpendContext,
        p2_puzzle_hash: Bytes32,
        underlying_coin_id: Bytes32,
        underlying_delegated_puzzle_hash: Bytes32,
        metadata: OptionMetadata,
    ) -> Result<(Conditions, OptionContract), DriverError> {
        let memos = ctx.hint(p2_puzzle_hash)?;
        let conditions = Conditions::new().create_coin(p2_puzzle_hash, 1, Some(memos));

        let inner_puzzle = ctx.alloc(&clvm_quote!(conditions))?;
        let p2_puzzle_hash = ctx.tree_hash(inner_puzzle).into();
        let inner_spend = Spend::new(inner_puzzle, NodePtr::NIL);

        let (mint_eve_option, eve_option) = self.mint_eve_option(
            ctx,
            p2_puzzle_hash,
            underlying_coin_id,
            underlying_delegated_puzzle_hash,
            metadata,
        )?;

        eve_option.spend(ctx, inner_spend)?;

        let child = eve_option.wrapped_child(p2_puzzle_hash);

        Ok((mint_eve_option, child))
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::Simulator;

    use crate::{OptionType, StandardLayer};

    use super::*;

    #[test]
    fn test_mint_option() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let alice = sim.bls(1);
        let alice_p2 = StandardLayer::new(alice.pk);

        let (create_option, _option) = Launcher::new(alice.coin.coin_id(), 1).mint_option(
            ctx,
            alice.puzzle_hash,
            alice.coin.coin_id(),
            alice.coin.puzzle_hash,
            OptionMetadata::new(10, OptionType::Xch),
        )?;
        alice_p2.spend(ctx, alice.coin, create_option)?;

        sim.spend_coins(ctx.take(), &[alice.sk])?;

        Ok(())
    }
}
