use std::sync::{Arc, Mutex};

use bindy::{Error, Result};
use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, Program as SerializedProgram};
use chia_puzzle_types::nft;
use chia_sdk_driver::{HashedPtr, Launcher, SpendContext, StandardLayer};
use clvm_tools_rs::classic::clvm_tools::binutils::assemble;
use clvm_traits::{clvm_quote, ToClvm};
use clvm_utils::TreeHash;
use clvmr::{
    serde::{node_from_bytes, node_from_bytes_backrefs},
    NodePtr,
};
use num_bigint::BigInt;

use crate::{
    CatSpend, Did, MintedNfts, MipsSpend, Nft, NftMetadata, NftMint, Program, Spend, StreamedCat,
    VaultMint,
};

pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
pub const MIN_SAFE_INTEGER: f64 = -MAX_SAFE_INTEGER;

// This is sort of an implementation detail of the CLVM runtime.
pub const MAX_CLVM_SMALL_INTEGER: i64 = 67_108_863;

// We use an Arc because we need to be able to share the SpendContext with the Program class
// And we use a Mutex because we need to retain mutability even while Program instances exist
#[derive(Default, Clone)]
pub struct Clvm(pub(crate) Arc<Mutex<SpendContext>>);

impl Clvm {
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn add_coin_spend(&self, coin_spend: CoinSpend) -> Result<()> {
        self.0.lock().unwrap().insert(coin_spend);
        Ok(())
    }

    pub fn spend_coin(&self, coin: Coin, spend: Spend) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        let puzzle_reveal = ctx.serialize(&spend.puzzle.1)?;
        let solution = ctx.serialize(&spend.solution.1)?;
        ctx.insert(chia_protocol::CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    pub fn coin_spends(&self) -> Result<Vec<CoinSpend>> {
        Ok(self.0.lock().unwrap().take())
    }

    pub fn delegated_spend(&self, conditions: Vec<Program>) -> Result<Spend> {
        let delegated_puzzle = self.0.lock().unwrap().alloc(&clvm_quote!(conditions
            .into_iter()
            .map(|p| p.1)
            .collect::<Vec<_>>()))?;
        Ok(Spend {
            puzzle: Program(self.0.clone(), delegated_puzzle),
            solution: Program(self.0.clone(), NodePtr::NIL),
        })
    }

    pub fn standard_spend(&self, synthetic_key: PublicKey, spend: Spend) -> Result<Spend> {
        let mut ctx = self.0.lock().unwrap();
        let spend =
            StandardLayer::new(synthetic_key).delegated_inner_spend(&mut ctx, spend.into())?;
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
        let mut ctx = self.0.lock().unwrap();
        let spend = self.standard_spend(synthetic_key, spend)?;
        let puzzle_reveal = ctx.serialize(&spend.puzzle.1)?;
        let solution = ctx.serialize(&spend.solution.1)?;
        ctx.insert(chia_protocol::CoinSpend::new(coin, puzzle_reveal, solution));
        Ok(())
    }

    pub fn spend_cat_coins(&self, cat_spends: Vec<CatSpend>) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();

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
        let mut ctx = self.0.lock().unwrap();
        let mut nfts = Vec::new();
        let mut parent_conditions = Vec::new();

        for (i, nft_mint) in nft_mints.into_iter().enumerate() {
            let nft_mint: chia_sdk_driver::NftMint<NodePtr> = nft_mint.into();
            let nft_mint = chia_sdk_driver::NftMint {
                metadata: HashedPtr::from_ptr(&ctx, nft_mint.metadata),
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
                let condition = condition.to_clvm(&mut ctx)?;
                parent_conditions.push(Program(self.0.clone(), condition));
            }
        }

        Ok(MintedNfts {
            nfts,
            parent_conditions,
        })
    }

    pub fn spend_nft(&self, nft: Nft, inner_spend: Spend) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();

        let ptr = nft.info.metadata.1;

        let nft = chia_sdk_driver::Nft {
            coin: nft.coin,
            proof: nft.lineage_proof.into(),
            info: chia_sdk_driver::NftInfo::from(nft.info)
                .with_metadata(HashedPtr::from_ptr(&ctx, ptr)),
        };

        nft.spend(
            &mut ctx,
            chia_sdk_driver::Spend::new(inner_spend.puzzle.1, inner_spend.solution.1),
        )?;

        Ok(())
    }

    pub fn spend_did(&self, did: Did, inner_spend: Spend) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();

        let ptr = did.info.metadata.1;

        let did = chia_sdk_driver::Did {
            coin: did.coin,
            proof: did.lineage_proof.into(),
            info: chia_sdk_driver::DidInfo::from(did.info)
                .with_metadata(HashedPtr::from_ptr(&ctx, ptr)),
        };

        did.spend(
            &mut ctx,
            chia_sdk_driver::Spend::new(inner_spend.puzzle.1, inner_spend.solution.1),
        )?;

        Ok(())
    }

    pub fn spend_streamed_cat(
        &self,
        streamed_cat: StreamedCat,
        payment_time: u64,
        clawback: bool,
    ) -> Result<()> {
        let mut ctx = self.0.lock().unwrap();
        let streamed_cat: chia_sdk_driver::StreamedCat = streamed_cat.try_into()?;

        streamed_cat.spend(&mut ctx, payment_time, clawback)?;

        Ok(())
    }

    pub fn mint_vault(
        &self,
        parent_coin_id: Bytes32,
        custody_hash: TreeHash,
        memos: Program,
    ) -> Result<VaultMint> {
        let mut ctx = self.0.lock().unwrap();

        let (parent_conditions, vault) =
            Launcher::new(parent_coin_id, 1).mint_vault(&mut ctx, custody_hash, memos.1)?;

        let parent_conditions = parent_conditions
            .into_iter()
            .map(|program| Ok(Program(self.0.clone(), program.to_clvm(&mut ctx)?)))
            .collect::<Result<Vec<_>>>()?;

        Ok(VaultMint {
            parent_conditions,
            vault: vault.into(),
        })
    }

    pub fn mips_spend(&self, coin: Coin, delegated_spend: Spend) -> Result<MipsSpend> {
        Ok(MipsSpend {
            clvm: self.0.clone(),
            spend: Arc::new(Mutex::new(chia_sdk_driver::MipsSpend::new(
                chia_sdk_driver::Spend::new(delegated_spend.puzzle.1, delegated_spend.solution.1),
            ))),
            coin,
        })
    }

    pub fn parse(&self, program: String) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = assemble(&mut ctx, &program)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn deserialize(&self, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = node_from_bytes(&mut ctx, &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn deserialize_with_backrefs(&self, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = node_from_bytes_backrefs(&mut ctx, &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn cache(&self, mod_hash: Bytes32, value: SerializedProgram) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.puzzle(mod_hash.into(), &value)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn pair(&self, first: Program, second: Program) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let ptr = ctx.new_pair(first.1, second.1)?;
        Ok(Program(self.0.clone(), ptr))
    }

    pub fn nil(&self) -> Result<Program> {
        Ok(Program(self.0.clone(), NodePtr::NIL))
    }

    // This is called by the individual napi and wasm binding crates
    pub fn f64(&self, value: f64) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();

        if value.is_infinite() {
            return Err(Error::Infinite);
        }

        if value.is_nan() {
            return Err(Error::NaN);
        }

        if value.fract() != 0.0 {
            return Err(Error::Fractional);
        }

        // If the value is larger, it can't be safely encoded as a JavaScript number.
        if value > MAX_SAFE_INTEGER {
            return Err(Error::TooLarge);
        }

        // If the value is smaller, it can't be safely encoded as a JavaScript number.
        if value < MIN_SAFE_INTEGER {
            return Err(Error::TooSmall);
        }

        #[allow(clippy::cast_possible_truncation)]
        let value = value as i64;

        if (0..=MAX_CLVM_SMALL_INTEGER).contains(&value) {
            Ok(Program(
                self.0.clone(),
                ctx.new_small_number(value.try_into().unwrap())?,
            ))
        } else {
            Ok(Program(self.0.clone(), ctx.new_number(value.into())?))
        }
    }

    pub fn int(&self, value: BigInt) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.lock().unwrap().new_number(value)?,
        ))
    }

    pub fn string(&self, value: String) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.lock().unwrap().new_atom(value.as_bytes())?,
        ))
    }

    pub fn bool(&self, value: bool) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.lock().unwrap().new_small_number(value as u32)?,
        ))
    }

    pub fn atom(&self, value: Bytes) -> Result<Program> {
        Ok(Program(
            self.0.clone(),
            self.0.lock().unwrap().new_atom(&value)?,
        ))
    }

    pub fn list(&self, value: Vec<Program>) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let mut result = NodePtr::NIL;

        for item in value.into_iter().rev() {
            result = ctx.new_pair(item.1, result)?;
        }

        Ok(Program(self.0.clone(), result))
    }

    pub fn nft_metadata(&self, value: NftMetadata) -> Result<Program> {
        let mut ctx = self.0.lock().unwrap();
        let nft_metadata = nft::NftMetadata::from(value);
        let ptr = ctx.alloc(&nft_metadata)?;
        Ok(Program(self.0.clone(), ptr))
    }
}
