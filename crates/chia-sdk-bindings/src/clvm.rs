use std::sync::{Arc, RwLock};

use bindy::{Error, Result};
use chia_protocol::{Bytes, Bytes32, Program as SerializedProgram};
use chia_sdk_driver::{HashedPtr, Launcher, SpendContext, StandardLayer};
use clvm_traits::{clvm_quote, ToClvm};
use clvmr::{
    serde::{node_from_bytes, node_from_bytes_backrefs},
    NodePtr,
};
use num_bigint::BigInt;

use crate::{CatSpend, Coin, CoinSpend, MintedNfts, NftMint, Program, PublicKey, Spend};

#[derive(Default, Clone)]
pub struct Clvm(Arc<RwLock<SpendContext>>);

impl Clvm {
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn add_coin_spend(&self, coin_spend: CoinSpend) -> Result<()> {
        self.0.write().unwrap().insert(coin_spend.into());
        Ok(())
    }

    pub fn coin_spends(&self) -> Result<Vec<CoinSpend>> {
        Ok(self
            .0
            .write()
            .unwrap()
            .take()
            .into_iter()
            .map(CoinSpend::from)
            .collect())
    }

    pub fn deserialize(&self, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.write().unwrap();
        let ptr = node_from_bytes(&mut ctx.allocator, &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn deserialize_with_backrefs(&self, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.write().unwrap();
        let ptr = node_from_bytes_backrefs(&mut ctx.allocator, &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn delegated_spend(&self, conditions: Vec<Program>) -> Result<Spend> {
        let delegated_puzzle = self.0.write().unwrap().alloc(&clvm_quote!(conditions
            .into_iter()
            .map(|p| p.1)
            .collect::<Vec<_>>()))?;
        Ok(Spend {
            puzzle: Program(self.0.clone(), delegated_puzzle),
            solution: Program(self.0.clone(), NodePtr::NIL),
        })
    }

    pub fn standard_spend(&self, synthetic_key: PublicKey, spend: Spend) -> Result<Spend> {
        let mut ctx = self.0.write().unwrap();
        let spend =
            StandardLayer::new(synthetic_key.0).delegated_inner_spend(&mut ctx, spend.into())?;
        Ok(Spend {
            puzzle: Program(self.0.clone(), spend.puzzle),
            solution: Program(self.0.clone(), spend.solution),
        })
    }

    pub fn spend_standard_coin(
        &self,
        coin: Coin,
        synthetic_key: PublicKey,
        spend: Spend,
    ) -> Result<()> {
        let mut ctx = self.0.write().unwrap();
        let spend = self.standard_spend(synthetic_key, spend)?;
        let puzzle_reveal = ctx.serialize(&spend.puzzle.1)?;
        let solution = ctx.serialize(&spend.solution.1)?;
        ctx.insert(chia_protocol::CoinSpend::new(
            coin.into(),
            puzzle_reveal,
            solution,
        ));
        Ok(())
    }

    pub fn spend_cat_coins(&self, cat_spends: Vec<CatSpend>) -> Result<()> {
        let mut ctx = self.0.write().unwrap();

        let mut rust_cat_spends = Vec::new();

        for cat_spend in cat_spends {
            rust_cat_spends.push(cat_spend.try_into()?);
        }

        chia_sdk_driver::Cat::spend_all(&mut ctx, &rust_cat_spends)?;

        Ok(())
    }

    pub fn mint_nfts(
        &self,
        parent_coin_id: Bytes32,
        nft_mints: Vec<NftMint>,
    ) -> Result<MintedNfts> {
        let mut ctx = self.0.write().unwrap();
        let mut nfts = Vec::new();
        let mut parent_conditions = Vec::new();

        for (i, nft_mint) in nft_mints.into_iter().enumerate() {
            let nft_mint: chia_sdk_driver::NftMint<NodePtr> = nft_mint.into();
            let nft_mint = chia_sdk_driver::NftMint {
                metadata: HashedPtr::from_ptr(&ctx.allocator, nft_mint.metadata),
                metadata_updater_puzzle_hash: nft_mint.metadata_updater_puzzle_hash,
                royalty_puzzle_hash: nft_mint.royalty_puzzle_hash,
                royalty_ten_thousandths: nft_mint.royalty_ten_thousandths,
                p2_puzzle_hash: nft_mint.p2_puzzle_hash,
                owner: nft_mint.owner,
            };

            let (conditions, nft) =
                Launcher::new(parent_coin_id, i as u64 * 2 + 1).mint_nft(&mut ctx, nft_mint)?;

            nfts.push(
                nft.with_metadata(Program(self.0.clone(), nft.info.metadata.ptr()))
                    .into(),
            );

            for condition in conditions {
                let condition = condition.to_clvm(&mut ctx.allocator)?;
                parent_conditions.push(Program(self.0.clone(), condition));
            }
        }

        Ok(MintedNfts {
            nfts,
            parent_conditions,
        })
    }

    pub fn pair(&self, first: Program, second: Program) -> Result<Program> {
        let mut ctx = self.0.write().unwrap();
        let ptr = ctx.allocator.new_pair(first.1, second.1)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn nil(&self) -> Result<Program> {
        Ok(Program(self.0.clone(), NodePtr::NIL))
    }

    // This is called by the individual napi and wasm binding crates
    pub fn f64(&self, value: f64) -> Result<Program> {
        let mut ctx = self.0.write().unwrap();

        if value.is_infinite() {
            return Err(Error::Infinite);
        }

        if value.is_nan() {
            return Err(Error::NaN);
        }

        if value.fract() != 0.0 {
            return Err(Error::Fractional);
        }

        if value > 9_007_199_254_740_991.0 {
            return Err(Error::TooLarge);
        }

        if value < -9_007_199_254_740_991.0 {
            return Err(Error::TooSmall);
        }

        #[allow(clippy::cast_possible_truncation)]
        let value = value as i64;

        if (0..=67_108_863).contains(&value) {
            Ok(Program(
                self.0.clone(),
                ctx.allocator.new_small_number(value.try_into().unwrap())?,
            ))
        } else {
            Ok(Program(
                self.0.clone(),
                ctx.allocator.new_number(value.into())?,
            ))
        }
    }

    /// This is called by the individual binding crates
    pub fn big_int(&self, value: BigInt) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.write().unwrap().allocator.new_number(value)?,
        ))
    }

    pub fn string(&self, value: String) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0
                .write()
                .unwrap()
                .allocator
                .new_atom(value.as_bytes())?,
        ))
    }

    pub fn bool(&self, value: bool) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0
                .write()
                .unwrap()
                .allocator
                .new_small_number(value as u32)?,
        ))
    }

    pub fn atom(&self, value: Bytes) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.write().unwrap().allocator.new_atom(&value)?,
        ))
    }

    pub fn list(&self, value: Vec<Program>) -> Result<Program> {
        let mut ctx = self.0.write().unwrap();
        let mut result = NodePtr::NIL;

        for item in value.into_iter().rev() {
            result = ctx.allocator.new_pair(item.1, result)?;
        }

        Ok(Program(self.0.clone(), result))
    }
}
