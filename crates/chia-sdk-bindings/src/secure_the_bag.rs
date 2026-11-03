use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes32, Coin};
use chia_sdk_driver::{
    self as sdk, BagAsset, BagChild, BagCoin, BagFanOuts, BagFormat, BagNode, BagPayment, Cat,
    CatInfo, ClawbackV2, SpendContext,
};

use chia_bls::PublicKey;
use clvmr::Allocator;

use crate::{AsProgram, AsPtr, Clvm, Nft, Program, Spend};

/// The bag is shared, since it can hold millions of payments and bindings clone their values.
#[derive(Clone)]
pub struct SecureTheBag(pub(crate) Arc<sdk::SecureTheBag>);

impl SecureTheBag {
    pub fn new(format: BagFormat, asset: BagAsset, payments: Vec<BagPayment>) -> Result<Self> {
        Ok(Self(Arc::new(sdk::SecureTheBag::build(
            format, asset, payments,
        )?)))
    }

    pub fn format(&self) -> Result<BagFormat> {
        Ok(self.0.format())
    }

    pub fn asset(&self) -> Result<BagAsset> {
        Ok(self.0.asset())
    }

    pub fn root_hash(&self) -> Result<Bytes32> {
        Ok(self.0.root_hash())
    }

    pub fn root_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.0.root_puzzle_hash())
    }

    pub fn total_amount(&self) -> Result<u64> {
        Ok(self.0.total_amount())
    }

    pub fn depth(&self) -> Result<usize> {
        Ok(self.0.depth())
    }

    pub fn payments(&self) -> Result<Vec<BagPayment>> {
        Ok(self.0.payments().to_vec())
    }

    /// The p2 puzzle hashes of every node, in the order they were built. The root comes last.
    pub fn node_puzzle_hashes(&self) -> Result<Vec<Bytes32>> {
        Ok(self.0.nodes().keys().copied().collect())
    }

    pub fn node(&self, puzzle_hash: Bytes32) -> Result<Option<BagNode>> {
        Ok(self.0.node(puzzle_hash).cloned())
    }

    pub fn parent(&self, puzzle_hash: Bytes32) -> Result<Option<Bytes32>> {
        Ok(self.0.parent(puzzle_hash))
    }

    pub fn path(&self, puzzle_hash: Bytes32) -> Result<Option<Vec<Bytes32>>> {
        Ok(self.0.path(puzzle_hash))
    }

    pub fn amount(&self, puzzle_hash: Bytes32) -> Result<Option<u64>> {
        Ok(self.0.amount(puzzle_hash))
    }

    pub fn coin_path(&self, start: BagCoin, puzzle_hash: Bytes32) -> Result<Vec<BagCoin>> {
        Ok(self.0.coin_path(start, puzzle_hash)?)
    }

    pub fn node_spend(&self, clvm: Clvm, puzzle_hash: Bytes32) -> Result<Spend> {
        let mut ctx = clvm.0.lock().unwrap();
        let spend = self.0.node_spend(&mut ctx, puzzle_hash)?;

        Ok(Spend {
            puzzle: Program(clvm.0.clone(), spend.puzzle),
            solution: Program(clvm.0.clone(), spend.solution),
        })
    }

    pub fn spend_node(&self, clvm: Clvm, coin: BagCoin) -> Result<Vec<BagCoin>> {
        let mut ctx = clvm.0.lock().unwrap();
        Ok(self.0.spend_node(&mut ctx, coin)?)
    }

    pub fn spend_path(&self, clvm: Clvm, start: BagCoin, puzzle_hash: Bytes32) -> Result<BagCoin> {
        let mut ctx = clvm.0.lock().unwrap();
        Ok(self.0.spend_path(&mut ctx, start, puzzle_hash)?)
    }

    pub fn unroll(&self, clvm: Clvm, start: BagCoin) -> Result<Vec<BagCoin>> {
        let mut ctx = clvm.0.lock().unwrap();
        Ok(self.0.unroll(&mut ctx, start)?)
    }
}

pub trait BagPaymentExt: Sized {
    fn from_clawback(clawback: ClawbackV2) -> Result<Self>;
    fn coin_puzzle_hash(&self) -> Result<Bytes32>;
}

impl BagPaymentExt for BagPayment {
    fn from_clawback(clawback: ClawbackV2) -> Result<Self> {
        Ok(Self::from_clawback(clawback))
    }

    fn coin_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.coin_puzzle_hash())
    }
}

#[derive(Clone)]
pub struct BagFormatLegacy {
    pub leaf_width: Option<usize>,
    pub sorted: bool,
}

#[derive(Clone)]
pub struct BagFormatChip0053 {
    pub leaf_width: Option<usize>,
    pub fan_outs: Option<Vec<usize>>,
    pub sorted: bool,
}

pub trait BagFormatExt: Sized {
    fn legacy(leaf_width: Option<usize>, sorted: bool) -> Result<Self>;
    fn chip0053(
        leaf_width: Option<usize>,
        fan_outs: Option<Vec<usize>>,
        sorted: bool,
    ) -> Result<Self>;
    fn width(&self, payment_count: usize) -> Result<usize>;
    fn fan_outs(&self, payment_count: usize) -> Result<Option<Vec<usize>>>;
    fn to_legacy(&self) -> Result<Option<BagFormatLegacy>>;
    fn to_chip0053(&self) -> Result<Option<BagFormatChip0053>>;
}

impl BagFormatExt for BagFormat {
    fn legacy(leaf_width: Option<usize>, sorted: bool) -> Result<Self> {
        Ok(Self::Legacy { leaf_width, sorted })
    }

    fn chip0053(
        leaf_width: Option<usize>,
        fan_outs: Option<Vec<usize>>,
        sorted: bool,
    ) -> Result<Self> {
        Ok(Self::Chip0053 {
            leaf_width,
            fan_outs: fan_outs
                .map(|fan_outs| BagFanOuts::new(&fan_outs))
                .transpose()?,
            sorted,
        })
    }

    fn width(&self, payment_count: usize) -> Result<usize> {
        Ok(self.width(payment_count))
    }

    fn fan_outs(&self, payment_count: usize) -> Result<Option<Vec<usize>>> {
        Ok(self
            .fan_outs(payment_count)
            .map(|fan_outs| fan_outs.iter().collect()))
    }

    fn to_legacy(&self) -> Result<Option<BagFormatLegacy>> {
        match *self {
            Self::Legacy { leaf_width, sorted } => Ok(Some(BagFormatLegacy { leaf_width, sorted })),
            _ => Ok(None),
        }
    }

    fn to_chip0053(&self) -> Result<Option<BagFormatChip0053>> {
        match *self {
            Self::Chip0053 {
                leaf_width,
                fan_outs,
                sorted,
            } => Ok(Some(BagFormatChip0053 {
                leaf_width,
                fan_outs: fan_outs.map(|fan_outs| fan_outs.iter().collect()),
                sorted,
            })),
            _ => Ok(None),
        }
    }
}

pub trait BagAssetExt: Sized {
    fn xch() -> Result<Self>;
    fn cat(asset_id: Bytes32) -> Result<Self>;
    fn revocable_cat(asset_id: Bytes32, hidden_puzzle_hash: Bytes32) -> Result<Self>;
    fn asset_id(&self) -> Result<Option<Bytes32>>;
    fn hidden_puzzle_hash(&self) -> Result<Option<Bytes32>>;
    fn puzzle_hash(&self, p2_puzzle_hash: Bytes32) -> Result<Bytes32>;
    fn cat_info(&self, p2_puzzle_hash: Bytes32) -> Result<Option<CatInfo>>;
}

impl BagAssetExt for BagAsset {
    fn xch() -> Result<Self> {
        Ok(Self::Xch)
    }

    fn cat(asset_id: Bytes32) -> Result<Self> {
        Ok(Self::Cat { asset_id })
    }

    fn revocable_cat(asset_id: Bytes32, hidden_puzzle_hash: Bytes32) -> Result<Self> {
        Ok(Self::RevocableCat {
            asset_id,
            hidden_puzzle_hash,
        })
    }

    fn asset_id(&self) -> Result<Option<Bytes32>> {
        Ok(match *self {
            Self::Xch => None,
            Self::Cat { asset_id } | Self::RevocableCat { asset_id, .. } => Some(asset_id),
        })
    }

    fn hidden_puzzle_hash(&self) -> Result<Option<Bytes32>> {
        Ok(match *self {
            Self::Xch | Self::Cat { .. } => None,
            Self::RevocableCat {
                hidden_puzzle_hash, ..
            } => Some(hidden_puzzle_hash),
        })
    }

    fn puzzle_hash(&self, p2_puzzle_hash: Bytes32) -> Result<Bytes32> {
        Ok(self.puzzle_hash(p2_puzzle_hash))
    }

    fn cat_info(&self, p2_puzzle_hash: Bytes32) -> Result<Option<CatInfo>> {
        Ok(self.cat_info(p2_puzzle_hash))
    }
}

pub trait BagChildExt: Sized {
    fn puzzle_hash(&self) -> Result<Bytes32>;
    fn amount(&self) -> Result<u64>;
    fn is_node(&self) -> Result<bool>;
    fn payment(&self) -> Result<Option<BagPayment>>;
}

impl BagChildExt for BagChild {
    fn puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.puzzle_hash())
    }

    fn amount(&self) -> Result<u64> {
        Ok(self.amount())
    }

    fn is_node(&self) -> Result<bool> {
        Ok(matches!(self, Self::Node { .. }))
    }

    fn payment(&self) -> Result<Option<BagPayment>> {
        Ok(self.payment().copied())
    }
}

pub trait BagCoinExt: Sized {
    fn xch(coin: Coin) -> Result<Self>;
    fn cat(cat: Cat) -> Result<Self>;
    fn coin(&self) -> Result<Coin>;
    fn p2_puzzle_hash(&self) -> Result<Bytes32>;
    fn child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Result<Self>;
    fn to_cat(&self) -> Result<Option<Cat>>;
}

impl BagCoinExt for BagCoin {
    fn xch(coin: Coin) -> Result<Self> {
        Ok(Self::Xch(coin))
    }

    fn cat(cat: Cat) -> Result<Self> {
        Ok(Self::Cat(cat))
    }

    fn coin(&self) -> Result<Coin> {
        Ok(self.coin())
    }

    fn p2_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.p2_puzzle_hash())
    }

    fn child(&self, p2_puzzle_hash: Bytes32, amount: u64) -> Result<Self> {
        Ok(self.child(p2_puzzle_hash, amount))
    }

    fn to_cat(&self) -> Result<Option<Cat>> {
        Ok(match self {
            Self::Xch(_) => None,
            Self::Cat(cat) => Some(*cat),
        })
    }
}

#[derive(Clone)]
pub struct SecuredMint {
    pub metadata: Program,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub p2_puzzle_hash: Bytes32,
    pub melt_public_key: Option<PublicKey>,
}

impl SecuredMint {
    fn as_sdk(&self, allocator: &Allocator) -> sdk::SecuredMint {
        sdk::SecuredMint {
            metadata: self.metadata.as_ptr(allocator),
            metadata_updater_puzzle_hash: self.metadata_updater_puzzle_hash,
            royalty_puzzle_hash: self.royalty_puzzle_hash,
            royalty_basis_points: self.royalty_basis_points,
            p2_puzzle_hash: self.p2_puzzle_hash,
            melt_public_key: self.melt_public_key,
        }
    }

    fn with_sdk<T>(&self, f: impl FnOnce(sdk::SecuredMint) -> T) -> T {
        let ctx = self.metadata.0.lock().unwrap();
        f(self.as_sdk(&ctx))
    }

    pub fn pre_launcher_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.with_sdk(|mint| mint.pre_launcher_puzzle_hash()))
    }

    pub fn eve_p2_puzzle_hash(&self) -> Result<Bytes32> {
        Ok(self.with_sdk(|mint| mint.eve_p2_puzzle_hash()))
    }

    pub fn bag_payment(&self) -> Result<BagPayment> {
        Ok(self.with_sdk(|mint| mint.bag_payment()))
    }

    pub fn output(&self, pre_launcher: Coin) -> Result<SecuredMintOutput> {
        let output = self.with_sdk(|mint| mint.output(pre_launcher));
        Ok(SecuredMintOutput::from_sdk(&output, &self.metadata.0))
    }

    pub fn mint(&self, clvm: Clvm, pre_launcher: Coin) -> Result<SecuredMintOutput> {
        let mut ctx = clvm.0.lock().unwrap();
        let mint = self.as_sdk(&ctx);
        let output = mint.mint(&mut ctx, pre_launcher)?;
        Ok(SecuredMintOutput::from_sdk(&output, &clvm.0))
    }

    pub fn melt(&self, clvm: Clvm, pre_launcher: Coin) -> Result<()> {
        let mut ctx = clvm.0.lock().unwrap();
        let mint = self.as_sdk(&ctx);
        Ok(mint.melt(&mut ctx, pre_launcher)?)
    }
}

#[derive(Clone)]
pub struct SecuredMintOutput {
    pub pre_launcher: Coin,
    pub launcher: Coin,
    pub eve_nft: Nft,
    pub nft: Nft,
}

impl SecuredMintOutput {
    fn from_sdk(output: &sdk::SecuredMintOutput, clvm: &Arc<Mutex<SpendContext>>) -> Self {
        Self {
            pre_launcher: output.pre_launcher,
            launcher: output.launcher,
            eve_nft: output.eve_nft.as_program(clvm),
            nft: output.nft.as_program(clvm),
        }
    }
}
