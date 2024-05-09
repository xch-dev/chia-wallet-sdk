use chia_bls::PublicKey;
use chia_protocol::{Bytes32, Coin, CoinSpend};
use chia_wallet::cat::{
    CatArgs, CatSolution, CoinProof, EverythingWithSignatureTailArgs, CAT_PUZZLE_HASH,
};
use clvm_traits::clvm_quote;
use clvm_utils::{curry_tree_hash, tree_hash_atom, CurriedProgram};
use clvmr::NodePtr;
use hex_literal::hex;

use crate::{ChainedSpend, CreateCoinWithMemos, RunTail, SpendContext, SpendError};

pub struct IssueCat {
    parent_coin_id: Bytes32,
    conditions: Vec<NodePtr>,
}

pub struct CatIssuanceInfo {
    pub asset_id: Bytes32,
    pub eve_coin: Coin,
    pub eve_inner_puzzle_hash: Bytes32,
}

impl IssueCat {
    pub fn new(parent_coin_id: Bytes32) -> Self {
        Self {
            parent_coin_id,
            conditions: Vec::new(),
        }
    }

    pub fn condition(mut self, condition: NodePtr) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn conditions(mut self, conditions: impl IntoIterator<Item = NodePtr>) -> Self {
        self.conditions.extend(conditions);
        self
    }

    pub fn multi_issuance(
        self,
        ctx: &mut SpendContext,
        public_key: PublicKey,
        amount: u64,
    ) -> Result<(ChainedSpend, CatIssuanceInfo), SpendError> {
        let tail_puzzle_ptr = ctx.everything_with_signature_tail_puzzle();

        let tail = ctx.alloc(CurriedProgram {
            program: tail_puzzle_ptr,
            args: EverythingWithSignatureTailArgs { public_key },
        })?;
        let asset_id = ctx.tree_hash(tail);

        self.condition(ctx.alloc(RunTail {
            program: tail,
            solution: NodePtr::NIL,
        })?)
        .finish(ctx, asset_id, amount)
    }

    pub fn finish(
        self,
        ctx: &mut SpendContext,
        asset_id: Bytes32,
        amount: u64,
    ) -> Result<(ChainedSpend, CatIssuanceInfo), SpendError> {
        let cat_puzzle_ptr = ctx.cat_puzzle();

        let inner_puzzle = ctx.alloc(clvm_quote!(self.conditions))?;
        let inner_puzzle_hash = ctx.tree_hash(inner_puzzle);

        let puzzle = ctx.alloc(CurriedProgram {
            program: cat_puzzle_ptr,
            args: CatArgs {
                mod_hash: CAT_PUZZLE_HASH.into(),
                tail_program_hash: asset_id,
                inner_puzzle,
            },
        })?;

        let puzzle_hash = ctx.tree_hash(puzzle);
        let coin = Coin::new(self.parent_coin_id, puzzle_hash, amount);

        let solution = ctx.serialize(CatSolution {
            inner_puzzle_solution: (),
            lineage_proof: None,
            prev_coin_id: coin.coin_id(),
            this_coin_info: coin.clone(),
            next_coin_proof: CoinProof {
                parent_coin_info: self.parent_coin_id,
                inner_puzzle_hash,
                amount,
            },
            prev_subtotal: 0,
            extra_delta: 0,
        })?;

        let puzzle_reveal = ctx.serialize(puzzle)?;
        ctx.spend(CoinSpend::new(coin.clone(), puzzle_reveal, solution));

        let chained_spend = ChainedSpend {
            parent_conditions: vec![ctx.alloc(CreateCoinWithMemos {
                puzzle_hash,
                amount,
                memos: vec![puzzle_hash.to_vec().into()],
            })?],
        };

        let issuance_info = CatIssuanceInfo {
            asset_id,
            eve_coin: coin,
            eve_inner_puzzle_hash: inner_puzzle_hash,
        };

        Ok((chained_spend, issuance_info))
    }
}

pub fn multi_issuance_asset_id(public_key: &PublicKey) -> Bytes32 {
    let public_key_hash = tree_hash_atom(&public_key.to_bytes());
    curry_tree_hash(
        hex!("1720d13250a7c16988eaf530331cefa9dd57a76b2c82236bec8bbbff91499b89"),
        &[public_key_hash],
    )
    .into()
}

#[cfg(test)]
mod tests {
    use chia_bls::{sign, Signature};
    use chia_protocol::SpendBundle;
    use chia_wallet::{
        standard::{standard_puzzle_hash, DEFAULT_HIDDEN_PUZZLE_HASH},
        DeriveSynthetic,
    };
    use clvmr::Allocator;

    use crate::{
        testing::SECRET_KEY, Chainable, CreateCoinWithMemos, RequiredSignature, StandardSpend,
        WalletSimulator,
    };

    use super::*;

    #[tokio::test]
    async fn test_cat_issuance() -> anyhow::Result<()> {
        let sim = WalletSimulator::new().await;
        let peer = sim.peer().await;

        let mut allocator = Allocator::new();
        let mut ctx = SpendContext::new(&mut allocator);

        let sk = SECRET_KEY.derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH);
        let pk = sk.public_key();
        let puzzle_hash = standard_puzzle_hash(&pk).into();
        let xch_coin = sim.generate_coin(puzzle_hash, 1).await.coin;

        let (issue_cat, _cat_info) = IssueCat::new(xch_coin.coin_id())
            .condition(ctx.alloc(CreateCoinWithMemos {
                puzzle_hash,
                amount: 1,
                memos: vec![puzzle_hash.to_vec().into()],
            })?)
            .multi_issuance(&mut ctx, pk.clone(), 1)?;

        StandardSpend::new()
            .chain(issue_cat)
            .finish(&mut ctx, xch_coin, pk)?;

        let coin_spends = ctx.take_spends();

        let required_signatures = RequiredSignature::from_coin_spends(
            &mut allocator,
            &coin_spends,
            WalletSimulator::AGG_SIG_ME.into(),
        )?;

        let mut aggregated_signature = Signature::default();

        for required in required_signatures {
            aggregated_signature += &sign(&sk, required.final_message());
        }

        let spend_bundle = SpendBundle::new(coin_spends, aggregated_signature);

        let ack = peer.send_transaction(spend_bundle).await?;
        assert_eq!(ack.error, None);
        assert_eq!(ack.status, 1);

        Ok(())
    }
}
