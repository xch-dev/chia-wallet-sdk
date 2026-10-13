use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_sdk_driver::{
    self as sdk, Bulletin, Cat, ClawbackPath, ClawbackV2, DropCoin, P2ConditionsOrSingleton,
    SpendContext, VaultOutput,
};
use chia_sdk_types::Condition;

use crate::{AsProgram, Clvm, Nft, NotarizedPayment, Payment, Program, Spend};

type SharedContext = Arc<Mutex<SpendContext>>;

fn conditions_to_programs(
    ctx: &mut SpendContext,
    clvm: &SharedContext,
    conditions: &[Condition],
) -> Result<Vec<Program>> {
    conditions
        .iter()
        .map(|condition| Ok(Program(clvm.clone(), ctx.alloc(condition)?)))
        .collect()
}

#[derive(Clone)]
pub struct Reveals {
    inner: Arc<Mutex<sdk::Reveals>>,
    clvm: SharedContext,
}

impl Reveals {
    pub fn new(clvm: Clvm) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Mutex::new(sdk::Reveals::default())),
            clvm: clvm.0.clone(),
        })
    }

    pub fn from_coin_spends(clvm: Clvm, coin_spends: Vec<CoinSpend>) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();
        let reveals = sdk::Reveals::from_coin_spends(&mut ctx, &coin_spends)?;

        Ok(Self {
            inner: Arc::new(Mutex::new(reveals)),
            clvm: clvm.0.clone(),
        })
    }

    pub fn reveal_coin_spend(&self, coin_spend: CoinSpend) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();
        self.inner
            .lock()
            .unwrap()
            .reveal_coin_spend(&mut ctx, &coin_spend)?;
        Ok(())
    }

    pub fn reveal_clawback(&self, clawback: ClawbackV2) -> Result<()> {
        self.inner.lock().unwrap().reveal_clawback(clawback);
        Ok(())
    }

    pub fn reveal_p2_conditions_or_singleton(&self, p2: P2ConditionsOrSingleton) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .reveal_p2_conditions_or_singleton(p2);
        Ok(())
    }

    pub fn reveal_vault_nonce(&self, nonce: usize) -> Result<()> {
        self.inner.lock().unwrap().reveal_vault_nonce(nonce);
        Ok(())
    }
}

pub fn parse_vault_transaction(
    clvm: Clvm,
    reveals: Reveals,
    launcher_id: Bytes32,
    delegated_spend: Spend,
) -> Result<VaultTransaction> {
    let reveals = reveals.inner.lock().unwrap().clone();
    let mut ctx = clvm.0.lock().unwrap();

    let transaction =
        sdk::parse_vault_transaction(reveals, &mut ctx, launcher_id, delegated_spend.into())?;

    VaultTransaction::from_sdk(&mut ctx, &clvm.0, transaction)
}

pub fn calculate_vault_puzzle_message(
    delegated_puzzle_hash: Bytes32,
    vault_puzzle_hash: Bytes32,
) -> Result<Bytes> {
    Ok(sdk::calculate_vault_puzzle_message(
        delegated_puzzle_hash,
        vault_puzzle_hash,
    ))
}

pub fn calculate_vault_coin_message(
    delegated_puzzle_hash: Bytes32,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
) -> Result<Bytes> {
    Ok(sdk::calculate_vault_coin_message(
        delegated_puzzle_hash,
        vault_coin_id,
        genesis_challenge,
    ))
}

pub fn calculate_vault_start_recovery_message(
    delegated_puzzle_hash: Bytes32,
    left_side_subtree_hash: Bytes32,
    recovery_timelock: u64,
    vault_coin_id: Bytes32,
    genesis_challenge: Bytes32,
) -> Result<Bytes> {
    Ok(sdk::calculate_vault_start_recovery_message(
        delegated_puzzle_hash,
        left_side_subtree_hash,
        recovery_timelock,
        vault_coin_id,
        genesis_challenge,
    ))
}

#[derive(Clone)]
pub struct VaultTransaction {
    pub vault_child: Option<VaultOutput>,
    pub drop_coins: Vec<DropCoin>,
    pub spends: Vec<VerifiedSpend>,
    pub issuances: Vec<Issuance>,
    pub asserted_payments: Vec<AssertedNotarizedPayment>,
    pub received_payments: Vec<AssertedPayment>,
    pub external_payments: Vec<AssertedPayment>,
    pub asset_flows: Vec<AssetFlow>,
    pub linked_offer: Option<LinkedOffer>,
    pub reserved_fee: u64,
    pub p2_puzzle_hashes: Vec<Bytes32>,
    pub delegated_puzzle_hash: Bytes32,
}

impl VaultTransaction {
    fn from_sdk(
        ctx: &mut SpendContext,
        clvm: &SharedContext,
        value: sdk::VaultTransaction,
    ) -> Result<Self> {
        Ok(Self {
            vault_child: value.vault_child,
            drop_coins: value.drop_coins,
            spends: value
                .spends
                .into_iter()
                .map(|spend| VerifiedSpend::from_sdk(ctx, clvm, spend))
                .collect::<Result<_>>()?,
            issuances: value.issuances.into_iter().map(Into::into).collect(),
            asserted_payments: value
                .asserted_payments
                .into_iter()
                .map(|payment| AssertedNotarizedPayment::from_sdk(clvm, payment))
                .collect(),
            received_payments: value
                .received_payments
                .into_iter()
                .map(|payment| AssertedPayment::from_sdk(clvm, payment))
                .collect(),
            external_payments: value
                .external_payments
                .into_iter()
                .map(|payment| AssertedPayment::from_sdk(clvm, payment))
                .collect(),
            asset_flows: value
                .asset_flows
                .into_iter()
                .map(|flow| AssetFlow::from_sdk(clvm, flow))
                .collect(),
            linked_offer: value
                .linked_offer
                .map(|offer| LinkedOffer::from_sdk(clvm, offer)),
            reserved_fee: value.reserved_fee,
            p2_puzzle_hashes: value.p2_puzzle_hashes,
            delegated_puzzle_hash: value.delegated_puzzle_hash,
        })
    }
}

#[derive(Clone)]
pub struct VerifiedSpend {
    pub asset: ParsedAsset,
    pub clawback: Option<ClawbackInfo>,
    pub custody: CustodyInfo,
    pub children: Vec<ParsedChild>,
    pub revoked: bool,
}

impl VerifiedSpend {
    fn from_sdk(
        ctx: &mut SpendContext,
        clvm: &SharedContext,
        value: sdk::VerifiedSpend,
    ) -> Result<Self> {
        Ok(Self {
            asset: ParsedAsset::from_sdk(clvm, value.asset),
            clawback: value.clawback.map(Into::into),
            custody: CustodyInfo::from_sdk(ctx, clvm, value.custody)?,
            children: value
                .children
                .into_iter()
                .map(|child| ParsedChild::from_sdk(ctx, clvm, child))
                .collect::<Result<_>>()?,
            revoked: value.revoked,
        })
    }
}

#[derive(Clone)]
pub enum ParsedAsset {
    Xch(Coin),
    Cat(Cat),
    Nft(Nft),
    Bulletin(Bulletin),
}

impl ParsedAsset {
    fn from_sdk(clvm: &SharedContext, value: sdk::ParsedAsset) -> Self {
        match value {
            sdk::ParsedAsset::Xch(coin) => Self::Xch(coin),
            sdk::ParsedAsset::Cat(cat) => Self::Cat(cat),
            sdk::ParsedAsset::Nft(nft) => Self::Nft(nft.as_program(clvm)),
            sdk::ParsedAsset::Bulletin(bulletin) => Self::Bulletin(bulletin),
        }
    }

    pub fn coin(&self) -> Result<Coin> {
        Ok(match self {
            Self::Xch(coin) => *coin,
            Self::Cat(cat) => cat.coin,
            Self::Nft(nft) => nft.coin,
            Self::Bulletin(bulletin) => bulletin.coin,
        })
    }

    pub fn as_xch(&self) -> Result<Option<Coin>> {
        Ok(match self {
            Self::Xch(coin) => Some(*coin),
            _ => None,
        })
    }

    pub fn as_cat(&self) -> Result<Option<Cat>> {
        Ok(match self {
            Self::Cat(cat) => Some(*cat),
            _ => None,
        })
    }

    pub fn as_nft(&self) -> Result<Option<Nft>> {
        Ok(match self {
            Self::Nft(nft) => Some(nft.clone()),
            _ => None,
        })
    }

    pub fn as_bulletin(&self) -> Result<Option<Bulletin>> {
        Ok(match self {
            Self::Bulletin(bulletin) => Some(bulletin.clone()),
            _ => None,
        })
    }
}

#[derive(Clone)]
pub struct ClawbackInfo {
    pub clawback: ClawbackV2,
    pub path: ClawbackPath,
}

impl From<sdk::ClawbackInfo> for ClawbackInfo {
    fn from(value: sdk::ClawbackInfo) -> Self {
        Self {
            clawback: value.clawback,
            path: value.path,
        }
    }
}

#[derive(Clone)]
pub enum CustodyInfo {
    P2Singleton(P2SingletonInfo),
    DelegatedConditions(Vec<Program>),
    P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo),
}

impl CustodyInfo {
    fn from_sdk(
        ctx: &mut SpendContext,
        clvm: &SharedContext,
        value: sdk::CustodyInfo,
    ) -> Result<Self> {
        Ok(match value {
            sdk::CustodyInfo::P2Singleton(info) => Self::P2Singleton(P2SingletonInfo {
                launcher_id: info.launcher_id,
                nonce: info.nonce,
                conditions: conditions_to_programs(ctx, clvm, &info.conditions)?,
                p2_puzzle_hash: info.p2_puzzle_hash,
            }),
            sdk::CustodyInfo::DelegatedConditions(conditions) => {
                Self::DelegatedConditions(conditions_to_programs(ctx, clvm, &conditions)?)
            }
            sdk::CustodyInfo::P2ConditionsOrSingleton(info) => {
                Self::P2ConditionsOrSingleton(P2ConditionsOrSingletonInfo {
                    launcher_id: info.launcher_id,
                    nonce: info.nonce,
                    conditions: conditions_to_programs(ctx, clvm, &info.conditions)?,
                    p2_puzzle_hash: info.p2_puzzle_hash,
                })
            }
        })
    }

    pub fn as_p2_singleton(&self) -> Result<Option<P2SingletonInfo>> {
        Ok(match self {
            Self::P2Singleton(info) => Some(info.clone()),
            _ => None,
        })
    }

    pub fn as_delegated_conditions(&self) -> Result<Option<Vec<Program>>> {
        Ok(match self {
            Self::DelegatedConditions(conditions) => Some(conditions.clone()),
            _ => None,
        })
    }

    pub fn as_p2_conditions_or_singleton(&self) -> Result<Option<P2ConditionsOrSingletonInfo>> {
        Ok(match self {
            Self::P2ConditionsOrSingleton(info) => Some(info.clone()),
            _ => None,
        })
    }
}

#[derive(Clone)]
pub struct P2SingletonInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub conditions: Vec<Program>,
    pub p2_puzzle_hash: Bytes32,
}

#[derive(Clone)]
pub struct P2ConditionsOrSingletonInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub conditions: Vec<Program>,
    pub p2_puzzle_hash: Bytes32,
}

#[derive(Clone)]
pub struct ParsedChild {
    pub asset: ParsedAsset,
    pub memos: ParsedMemos,
    pub transfer_type: TransferType,
}

impl ParsedChild {
    fn from_sdk(
        ctx: &mut SpendContext,
        clvm: &SharedContext,
        value: sdk::ParsedChild,
    ) -> Result<Self> {
        Ok(Self {
            asset: ParsedAsset::from_sdk(clvm, value.asset),
            memos: ParsedMemos::from_sdk(ctx, clvm, value.memos)?,
            transfer_type: TransferType::from_sdk(ctx, clvm, value.transfer_type)?,
        })
    }
}

#[derive(Clone)]
pub struct ParsedMemos {
    pub p2_puzzle_hash: Bytes32,
    pub clawback: Option<ClawbackV2>,
    pub human_readable_memos: Vec<String>,
    pub fixed_conditions: Option<Vec<Program>>,
}

impl ParsedMemos {
    fn from_sdk(
        ctx: &mut SpendContext,
        clvm: &SharedContext,
        value: sdk::ParsedMemos,
    ) -> Result<Self> {
        Ok(Self {
            p2_puzzle_hash: value.p2_puzzle_hash,
            clawback: value.clawback,
            human_readable_memos: value.human_readable_memos,
            fixed_conditions: value
                .fixed_conditions
                .map(|conditions| conditions_to_programs(ctx, clvm, &conditions))
                .transpose()?,
        })
    }
}

#[derive(Clone)]
pub enum TransferType {
    Sent,
    Burned,
    Offered,
    OfferPreSplit(OfferPreSplitInfo),
}

impl TransferType {
    fn from_sdk(
        ctx: &mut SpendContext,
        clvm: &SharedContext,
        value: sdk::TransferType,
    ) -> Result<Self> {
        Ok(match value {
            sdk::TransferType::Sent => Self::Sent,
            sdk::TransferType::Burned => Self::Burned,
            sdk::TransferType::Offered => Self::Offered,
            sdk::TransferType::OfferPreSplit(info) => Self::OfferPreSplit(OfferPreSplitInfo {
                launcher_id: info.launcher_id,
                nonce: info.nonce,
                fixed_conditions: conditions_to_programs(ctx, clvm, &info.fixed_conditions)?,
                settlement_amount: info.settlement_amount,
            }),
        })
    }

    pub fn is_sent(&self) -> Result<bool> {
        Ok(matches!(self, Self::Sent))
    }

    pub fn is_burned(&self) -> Result<bool> {
        Ok(matches!(self, Self::Burned))
    }

    pub fn is_offered(&self) -> Result<bool> {
        Ok(matches!(self, Self::Offered))
    }

    pub fn as_offer_pre_split(&self) -> Result<Option<OfferPreSplitInfo>> {
        Ok(match self {
            Self::OfferPreSplit(info) => Some(info.clone()),
            _ => None,
        })
    }
}

#[derive(Clone)]
pub struct OfferPreSplitInfo {
    pub launcher_id: Bytes32,
    pub nonce: usize,
    pub fixed_conditions: Vec<Program>,
    pub settlement_amount: u64,
}

#[derive(Clone)]
pub struct Issuance {
    pub coin_id: Bytes32,
    pub asset_id: Bytes32,
    pub extra_delta: i64,
    pub kind: IssuanceKind,
}

impl From<sdk::Issuance> for Issuance {
    fn from(value: sdk::Issuance) -> Self {
        Self {
            coin_id: value.coin_id,
            asset_id: value.asset_id,
            extra_delta: value.extra_delta,
            kind: value.kind.into(),
        }
    }
}

#[derive(Clone)]
pub enum IssuanceKind {
    EverythingWithSingleton(EverythingWithSingletonIssuance),
    Unknown,
}

impl From<sdk::IssuanceKind> for IssuanceKind {
    fn from(value: sdk::IssuanceKind) -> Self {
        match value {
            sdk::IssuanceKind::EverythingWithSingleton {
                singleton_struct_hash,
                nonce,
            } => Self::EverythingWithSingleton(EverythingWithSingletonIssuance {
                singleton_struct_hash,
                nonce,
            }),
            sdk::IssuanceKind::Unknown => Self::Unknown,
        }
    }
}

impl IssuanceKind {
    pub fn as_everything_with_singleton(&self) -> Result<Option<EverythingWithSingletonIssuance>> {
        Ok(match self {
            Self::EverythingWithSingleton(issuance) => Some(issuance.clone()),
            Self::Unknown => None,
        })
    }

    pub fn is_unknown(&self) -> Result<bool> {
        Ok(matches!(self, Self::Unknown))
    }
}

#[derive(Clone)]
pub struct EverythingWithSingletonIssuance {
    pub singleton_struct_hash: Bytes32,
    pub nonce: Bytes,
}

#[derive(Clone)]
pub enum ClearSigningAsset {
    Xch,
    Cat(ClearSigningCat),
    Nft(ClearSigningNft),
}

impl ClearSigningAsset {
    fn from_sdk(clvm: &SharedContext, value: sdk::ClearSigningAsset) -> Self {
        match value {
            sdk::ClearSigningAsset::Xch => Self::Xch,
            sdk::ClearSigningAsset::Cat {
                asset_id,
                hidden_puzzle_hash,
            } => Self::Cat(ClearSigningCat {
                asset_id,
                hidden_puzzle_hash,
            }),
            sdk::ClearSigningAsset::Nft {
                launcher_id,
                metadata,
                metadata_updater_puzzle_hash,
                royalty_puzzle_hash,
                royalty_basis_points,
            } => Self::Nft(ClearSigningNft {
                launcher_id,
                metadata: metadata.as_program(clvm),
                metadata_updater_puzzle_hash,
                royalty_puzzle_hash,
                royalty_basis_points,
            }),
        }
    }

    pub fn is_xch(&self) -> Result<bool> {
        Ok(matches!(self, Self::Xch))
    }

    pub fn as_cat(&self) -> Result<Option<ClearSigningCat>> {
        Ok(match self {
            Self::Cat(cat) => Some(cat.clone()),
            _ => None,
        })
    }

    pub fn as_nft(&self) -> Result<Option<ClearSigningNft>> {
        Ok(match self {
            Self::Nft(nft) => Some(nft.clone()),
            _ => None,
        })
    }
}

#[derive(Clone)]
pub struct ClearSigningCat {
    pub asset_id: Bytes32,
    pub hidden_puzzle_hash: Option<Bytes32>,
}

#[derive(Clone)]
pub struct ClearSigningNft {
    pub launcher_id: Bytes32,
    pub metadata: Program,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
}

#[derive(Clone)]
pub struct AssertedNotarizedPayment {
    pub asset: ClearSigningAsset,
    pub notarized_payment: NotarizedPayment,
}

impl AssertedNotarizedPayment {
    fn from_sdk(clvm: &SharedContext, value: sdk::AssertedNotarizedPayment) -> Self {
        Self {
            asset: ClearSigningAsset::from_sdk(clvm, value.asset),
            notarized_payment: value.notarized_payment.as_program(clvm),
        }
    }
}

#[derive(Clone)]
pub struct AssertedPayment {
    pub asset: ClearSigningAsset,
    pub nonce: Bytes32,
    pub payment: Payment,
    pub royalty_basis_points: Option<u16>,
}

impl AssertedPayment {
    fn from_sdk(clvm: &SharedContext, value: sdk::AssertedPayment) -> Self {
        Self {
            asset: ClearSigningAsset::from_sdk(clvm, value.asset),
            nonce: value.nonce,
            payment: value.payment.as_program(clvm),
            royalty_basis_points: value.royalty_basis_points,
        }
    }
}

#[derive(Clone)]
pub struct AssetFlow {
    pub asset: ClearSigningAsset,
    pub input_amount: u128,
    pub output_amount: u128,
    pub issued_amount: u128,
    pub melted_amount: u128,
    pub received_amount: u128,
    pub paid_amount: u128,
    pub unaccounted_amount: u128,
}

impl AssetFlow {
    fn from_sdk(clvm: &SharedContext, value: sdk::AssetFlow) -> Self {
        Self {
            asset: ClearSigningAsset::from_sdk(clvm, value.asset),
            input_amount: value.input_amount,
            output_amount: value.output_amount,
            issued_amount: value.issued_amount,
            melted_amount: value.melted_amount,
            received_amount: value.received_amount,
            paid_amount: value.paid_amount,
            unaccounted_amount: value.unaccounted_amount,
        }
    }
}

#[derive(Clone)]
pub struct LinkedOffer {
    pub reserved_fee: u64,
    pub received_payments: Vec<AssertedPayment>,
    pub external_payments: Vec<AssertedPayment>,
}

impl LinkedOffer {
    fn from_sdk(clvm: &SharedContext, value: sdk::LinkedOffer) -> Self {
        Self {
            reserved_fee: value.reserved_fee,
            received_payments: value
                .received_payments
                .into_iter()
                .map(|payment| AssertedPayment::from_sdk(clvm, payment))
                .collect(),
            external_payments: value
                .external_payments
                .into_iter()
                .map(|payment| AssertedPayment::from_sdk(clvm, payment))
                .collect(),
        }
    }
}
