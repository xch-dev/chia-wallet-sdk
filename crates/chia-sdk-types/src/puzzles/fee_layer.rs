use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_sdk_puzzles::{FEE_LAYER_V1, FEE_LAYER_V1_HASH};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;

use crate::Mod;

// Asset kind is inferred from `asset_id`: `None` means XCH, `Some` means CAT.
pub const TRANSFER_FEE_TRADE_PRICE_ASSET_KIND_XCH: u8 = 0;
pub const TRANSFER_FEE_TRADE_PRICE_ASSET_KIND_CAT: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct TransferFeeTradePrice {
    pub amount: u64,
    pub asset_id: Option<Bytes32>,
    pub quote_hidden_puzzle_hash: Option<Bytes32>,
    pub quote_fee_policy: Option<TransferFeeQuoteFeePolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ToClvm, FromClvm)]
#[clvm(list)]
pub struct TransferFeeQuoteFeePolicy {
    pub issuer_fee_puzzle_hash: Bytes32,
    pub fee_basis_points: u16,
    pub min_fee: u64,
    pub allow_zero_price: bool,
    pub allow_revoke_fee_bypass: bool,
}

impl TransferFeeTradePrice {
    pub fn xch(amount: u64) -> Self {
        Self {
            amount,
            asset_id: None,
            quote_hidden_puzzle_hash: None,
            quote_fee_policy: None,
        }
    }

    pub fn cat(
        amount: u64,
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
        fee_policy: Option<TransferFeeQuoteFeePolicy>,
    ) -> Self {
        Self::cat_with_quote_layers(amount, asset_id, hidden_puzzle_hash, fee_policy)
    }

    pub fn cat_with_quote_layers(
        amount: u64,
        asset_id: Bytes32,
        hidden_puzzle_hash: Option<Bytes32>,
        fee_policy: Option<TransferFeeQuoteFeePolicy>,
    ) -> Self {
        Self {
            amount,
            asset_id: Some(asset_id),
            quote_hidden_puzzle_hash: hidden_puzzle_hash,
            quote_fee_policy: fee_policy,
        }
    }

    pub fn is_xch(&self) -> bool {
        self.asset_id.is_none()
    }

    pub fn is_valid_quote_descriptor(&self) -> bool {
        if self.is_xch() {
            self.quote_hidden_puzzle_hash.is_none() && self.quote_fee_policy.is_none()
        } else {
            true
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct FeeLayerArgs<I> {
    pub mod_hash: Bytes32,
    pub issuer_fee_puzzle_hash: Bytes32,
    pub fee_basis_points: u16,
    pub min_fee: u64,
    pub allow_zero_price: bool,
    pub allow_revoke_fee_bypass: bool,
    pub has_hidden_revoke_layer: bool,
    pub inner_puzzle: I,
}

impl<I> FeeLayerArgs<I> {
    pub fn new(
        issuer_fee_puzzle_hash: Bytes32,
        fee_basis_points: u16,
        min_fee: u64,
        allow_zero_price: bool,
        allow_revoke_fee_bypass: bool,
        has_hidden_revoke_layer: bool,
        inner_puzzle: I,
    ) -> Self {
        Self {
            mod_hash: FEE_LAYER_V1_HASH.into(),
            issuer_fee_puzzle_hash,
            fee_basis_points,
            min_fee,
            allow_zero_price,
            allow_revoke_fee_bypass,
            has_hidden_revoke_layer,
            inner_puzzle,
        }
    }
}

impl<I> Mod for FeeLayerArgs<I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(FEE_LAYER_V1)
    }

    fn mod_hash() -> TreeHash {
        TreeHash::new(FEE_LAYER_V1_HASH)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct FeeLayerSolution<S> {
    pub inner_solution: S,
}

impl<S> FeeLayerSolution<S> {
    pub fn new(inner_solution: S) -> Self {
        Self { inner_solution }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clvm_traits::{FromClvm, ToClvm};
    use clvmr::Allocator;

    use super::*;

    #[test]
    fn xch_trade_price_clvm_shape_includes_quote_fee_policy_slot() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ptr = TransferFeeTradePrice::xch(1).to_clvm(&mut allocator)?;
        let fields = Vec::<clvmr::NodePtr>::from_clvm(&allocator, ptr)?;

        assert_eq!(fields.len(), 4);
        Ok(())
    }

    #[test]
    fn fee_layer_rue_source_matches_embedded_constants() -> anyhow::Result<()> {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../chia-sdk-puzzles/puzzles/fee_layer_v1.rue"
        ));

        let compiled = crate::compile_rue(path, false, None)?;
        if compiled.reveal.as_slice() != FEE_LAYER_V1 {
            let first_mismatch = compiled
                .reveal
                .iter()
                .zip(FEE_LAYER_V1.iter())
                .position(|(a, b)| a != b);

            panic!(
                "fee_layer_v1 bytes drifted (compiled_len={}, embedded_len={}, first_mismatch={first_mismatch:?})",
                compiled.reveal.len(),
                FEE_LAYER_V1.len(),
            );
        }
        assert_eq!(compiled.hash, TreeHash::new(FEE_LAYER_V1_HASH));

        Ok(())
    }
}
