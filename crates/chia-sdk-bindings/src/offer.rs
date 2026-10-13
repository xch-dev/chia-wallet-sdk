use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_protocol::{Bytes32, Coin, SpendBundle};
use chia_puzzle_types::{
    Memos,
    offer::{NotarizedPayment as SdkNotarizedPayment, Payment as SdkPayment},
};
use chia_sdk_driver::{self as sdk, Cat, SpendContext};
use chia_sdk_types::conditions::TradePrice;

use crate::{Action, AsProgram, AsPtr, Clvm, Program, RoyaltyInfo};

pub fn encode_offer(spend_bundle: SpendBundle) -> Result<String> {
    Ok(chia_sdk_driver::encode_offer(&spend_bundle)?)
}

pub fn decode_offer(offer: String) -> Result<SpendBundle> {
    Ok(chia_sdk_driver::decode_offer(&offer)?)
}

#[derive(Clone)]
pub struct NotarizedPayment {
    pub nonce: Bytes32,
    pub payments: Vec<Payment>,
}

impl AsProgram for SdkNotarizedPayment {
    type AsProgram = NotarizedPayment;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        NotarizedPayment {
            nonce: self.nonce,
            payments: self.payments.iter().map(|p| p.as_program(clvm)).collect(),
        }
    }
}

impl From<NotarizedPayment> for SdkNotarizedPayment {
    fn from(notarized_payment: NotarizedPayment) -> Self {
        Self::new(
            notarized_payment.nonce,
            notarized_payment
                .payments
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }
}

#[derive(Clone)]
pub struct Payment {
    pub puzzle_hash: Bytes32,
    pub amount: u64,
    pub memos: Option<Program>,
}

impl AsProgram for SdkPayment {
    type AsProgram = Payment;

    fn as_program(&self, clvm: &Arc<Mutex<SpendContext>>) -> Self::AsProgram {
        Payment {
            puzzle_hash: self.puzzle_hash,
            amount: self.amount,
            memos: match self.memos {
                Memos::Some(memos) => Some(memos.as_program(clvm)),
                Memos::None => None,
            },
        }
    }
}

impl From<Payment> for SdkPayment {
    fn from(payment: Payment) -> Self {
        Self::new(
            payment.puzzle_hash,
            payment.amount,
            payment
                .memos
                .as_ref()
                .map_or(Memos::None, |m| Memos::Some(m.1)),
        )
    }
}

#[derive(Clone)]
pub struct RequestedPayments {
    pub(crate) inner: Arc<Mutex<sdk::RequestedPayments>>,
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
}

impl RequestedPayments {
    pub fn new(clvm: Clvm) -> Result<Self> {
        Ok(Self::from_sdk(sdk::RequestedPayments::new(), &clvm.0))
    }

    pub(crate) fn from_sdk(
        requested_payments: sdk::RequestedPayments,
        clvm: &Arc<Mutex<SpendContext>>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(requested_payments)),
            clvm: clvm.clone(),
        }
    }

    pub(crate) fn to_sdk(&self) -> sdk::RequestedPayments {
        self.inner.lock().unwrap().clone()
    }

    pub fn add_xch(&self, notarized_payment: NotarizedPayment) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .xch
            .push(notarized_payment.into());
        Ok(())
    }

    pub fn add_cat(&self, asset_id: Bytes32, notarized_payment: NotarizedPayment) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .cats
            .entry(asset_id)
            .or_default()
            .push(notarized_payment.into());
        Ok(())
    }

    pub fn add_nft(&self, launcher_id: Bytes32, notarized_payment: NotarizedPayment) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .nfts
            .entry(launcher_id)
            .or_default()
            .push(notarized_payment.into());
        Ok(())
    }

    pub fn add_option(
        &self,
        launcher_id: Bytes32,
        notarized_payment: NotarizedPayment,
    ) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .options
            .entry(launcher_id)
            .or_default()
            .push(notarized_payment.into());
        Ok(())
    }

    pub fn xch(&self) -> Result<Vec<NotarizedPayment>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .xch
            .iter()
            .map(|np| np.as_program(&self.clvm))
            .collect())
    }

    pub fn cat_asset_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self.inner.lock().unwrap().cats.keys().copied().collect())
    }

    pub fn cats(&self, asset_id: Bytes32) -> Result<Vec<NotarizedPayment>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .cats
            .get(&asset_id)
            .into_iter()
            .flatten()
            .map(|np| np.as_program(&self.clvm))
            .collect())
    }

    pub fn nft_launcher_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self.inner.lock().unwrap().nfts.keys().copied().collect())
    }

    pub fn nfts(&self, launcher_id: Bytes32) -> Result<Vec<NotarizedPayment>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .nfts
            .get(&launcher_id)
            .into_iter()
            .flatten()
            .map(|np| np.as_program(&self.clvm))
            .collect())
    }

    pub fn option_launcher_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self.inner.lock().unwrap().options.keys().copied().collect())
    }

    pub fn options(&self, launcher_id: Bytes32) -> Result<Vec<NotarizedPayment>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .options
            .get(&launcher_id)
            .into_iter()
            .flatten()
            .map(|np| np.as_program(&self.clvm))
            .collect())
    }

    pub fn amounts(&self) -> Result<OfferAmounts> {
        Ok(OfferAmounts::from_sdk(self.inner.lock().unwrap().amounts()))
    }

    /// The settle actions that pay every requested payment, for the taker of an offer.
    pub fn actions(&self) -> Result<Vec<Action>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .actions()
            .into_iter()
            .map(Action::from)
            .collect())
    }

    /// The conditions that assert every requested payment was made, for the maker of an offer.
    /// These should be added as required conditions of the maker's spends.
    pub fn assertions(&self, asset_info: AssetInfo) -> Result<Vec<Program>> {
        let requested_payments = self.to_sdk();
        let asset_info = asset_info.to_sdk();
        let mut ctx = self.clvm.lock().unwrap();

        let mut conditions = Vec::new();

        for assertion in requested_payments.assertions(&mut ctx, &asset_info)? {
            conditions.push(Program(self.clvm.clone(), ctx.alloc(&assertion)?));
        }

        Ok(conditions)
    }
}

#[derive(Clone)]
pub struct AssetInfo {
    pub(crate) inner: Arc<Mutex<sdk::AssetInfo>>,
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
}

impl AssetInfo {
    pub fn new(clvm: Clvm) -> Result<Self> {
        Ok(Self::from_sdk(sdk::AssetInfo::new(), &clvm.0))
    }

    pub(crate) fn from_sdk(asset_info: sdk::AssetInfo, clvm: &Arc<Mutex<SpendContext>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(asset_info)),
            clvm: clvm.clone(),
        }
    }

    pub(crate) fn to_sdk(&self) -> sdk::AssetInfo {
        self.inner.lock().unwrap().clone()
    }

    pub fn insert_cat(&self, asset_id: Bytes32, hidden_puzzle_hash: Option<Bytes32>) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .insert_cat(asset_id, sdk::CatAssetInfo::new(hidden_puzzle_hash))?;
        Ok(())
    }

    pub fn insert_nft(
        &self,
        launcher_id: Bytes32,
        metadata: Program,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
    ) -> Result<()> {
        let metadata = metadata.as_ptr(&self.clvm.lock().unwrap());

        self.inner.lock().unwrap().insert_nft(
            launcher_id,
            sdk::NftAssetInfo::new(
                metadata,
                metadata_updater_puzzle_hash,
                royalty_puzzle_hash,
                royalty_basis_points,
            ),
        )?;
        Ok(())
    }

    pub fn insert_option(
        &self,
        launcher_id: Bytes32,
        underlying_coin_id: Bytes32,
        underlying_delegated_puzzle_hash: Bytes32,
    ) -> Result<()> {
        self.inner.lock().unwrap().insert_option(
            launcher_id,
            sdk::OptionAssetInfo::new(underlying_coin_id, underlying_delegated_puzzle_hash),
        )?;
        Ok(())
    }

    pub fn cat_asset_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self.inner.lock().unwrap().cats().copied().collect())
    }

    pub fn cat_hidden_puzzle_hash(&self, asset_id: Bytes32) -> Result<Option<Bytes32>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .cat(asset_id)
            .and_then(|info| info.hidden_puzzle_hash))
    }

    pub fn nft_launcher_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self.inner.lock().unwrap().nfts().copied().collect())
    }

    pub fn nft_royalty(&self, launcher_id: Bytes32) -> Result<Option<RoyaltyInfo>> {
        Ok(self.inner.lock().unwrap().nft(launcher_id).map(|info| {
            RoyaltyInfo::new(
                launcher_id,
                info.royalty_puzzle_hash,
                info.royalty_basis_points,
            )
        }))
    }
}

#[derive(Clone)]
pub struct OfferAmounts(pub(crate) Arc<Mutex<sdk::OfferAmounts>>);

impl OfferAmounts {
    pub fn new() -> Result<Self> {
        Ok(Self::from_sdk(sdk::OfferAmounts::new()))
    }

    pub(crate) fn from_sdk(amounts: sdk::OfferAmounts) -> Self {
        Self(Arc::new(Mutex::new(amounts)))
    }

    pub(crate) fn to_sdk(&self) -> sdk::OfferAmounts {
        self.0.lock().unwrap().clone()
    }

    pub fn xch(&self) -> Result<u128> {
        Ok(self.0.lock().unwrap().xch)
    }

    pub fn set_xch(&self, amount: u128) -> Result<()> {
        self.0.lock().unwrap().xch = amount;
        Ok(())
    }

    pub fn asset_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self.0.lock().unwrap().cats.keys().copied().collect())
    }

    pub fn cat(&self, asset_id: Bytes32) -> Result<u128> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .cats
            .get(&asset_id)
            .copied()
            .unwrap_or(0))
    }

    pub fn set_cat(&self, asset_id: Bytes32, amount: u128) -> Result<()> {
        self.0.lock().unwrap().cats.insert(asset_id, amount);
        Ok(())
    }
}

#[derive(Clone)]
pub struct Offer {
    pub(crate) inner: sdk::Offer,
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
}

impl Offer {
    /// Creates an offer from the maker's spend bundle, which must send the offered assets to the
    /// settlement puzzle and assert the requested payments.
    pub fn from_input_spend_bundle(
        clvm: Clvm,
        spend_bundle: SpendBundle,
        requested_payments: RequestedPayments,
        asset_info: AssetInfo,
    ) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();

        let inner = sdk::Offer::from_input_spend_bundle(
            &mut ctx,
            spend_bundle,
            requested_payments.to_sdk(),
            asset_info.to_sdk(),
        )?;

        Ok(Self {
            inner,
            clvm: clvm.0.clone(),
        })
    }

    /// Parses an offer spend bundle, such as one decoded with `decodeOffer`.
    pub fn from_spend_bundle(clvm: Clvm, spend_bundle: SpendBundle) -> Result<Self> {
        let mut ctx = clvm.0.lock().unwrap();

        let inner = sdk::Offer::from_spend_bundle(&mut ctx, &spend_bundle)?;

        Ok(Self {
            inner,
            clvm: clvm.0.clone(),
        })
    }

    /// The offer spend bundle, including the requested payments, which can be encoded with `encodeOffer`.
    pub fn to_spend_bundle(&self) -> Result<SpendBundle> {
        let mut ctx = self.clvm.lock().unwrap();
        Ok(self.inner.clone().to_spend_bundle(&mut ctx)?)
    }

    /// Combines the maker's spends with the taker's spend bundle into the final transaction.
    pub fn take(&self, spend_bundle: SpendBundle) -> Result<SpendBundle> {
        Ok(self.inner.clone().take(spend_bundle))
    }

    pub fn nonce(coin_ids: Vec<Bytes32>) -> Result<Bytes32> {
        Ok(sdk::Offer::nonce(coin_ids))
    }

    pub fn requested_payments(&self) -> Result<RequestedPayments> {
        Ok(RequestedPayments::from_sdk(
            self.inner.requested_payments().clone(),
            &self.clvm,
        ))
    }

    pub fn asset_info(&self) -> Result<AssetInfo> {
        Ok(AssetInfo::from_sdk(
            self.inner.asset_info().clone(),
            &self.clvm,
        ))
    }

    pub fn offered_amounts(&self) -> Result<OfferAmounts> {
        Ok(OfferAmounts::from_sdk(self.inner.offered_coins().amounts()))
    }

    pub fn offered_xch(&self) -> Result<Vec<Coin>> {
        Ok(self.inner.offered_coins().xch.clone())
    }

    pub fn offered_cats(&self, asset_id: Bytes32) -> Result<Vec<Cat>> {
        Ok(self
            .inner
            .offered_coins()
            .cats
            .get(&asset_id)
            .cloned()
            .unwrap_or_default())
    }

    pub fn offered_nft_launcher_ids(&self) -> Result<Vec<Bytes32>> {
        Ok(self.inner.offered_coins().nfts.keys().copied().collect())
    }

    /// The royalties of the requested NFTs, which are paid by the maker.
    pub fn offered_royalties(&self) -> Result<Vec<RoyaltyInfo>> {
        Ok(self.inner.offered_royalties())
    }

    /// The royalties of the offered NFTs, which are paid by the taker.
    pub fn requested_royalties(&self) -> Result<Vec<RoyaltyInfo>> {
        Ok(self.inner.requested_royalties())
    }

    pub fn offered_royalty_amounts(&self) -> Result<OfferAmounts> {
        Ok(OfferAmounts::from_sdk(
            self.inner.offered_royalty_amounts()?,
        ))
    }

    pub fn requested_royalty_amounts(&self) -> Result<OfferAmounts> {
        Ok(OfferAmounts::from_sdk(
            self.inner.requested_royalty_amounts()?,
        ))
    }
}

pub fn calculate_trade_price_amounts(
    amounts: OfferAmounts,
    royalty_nft_count: u32,
) -> Result<OfferAmounts> {
    Ok(OfferAmounts::from_sdk(sdk::calculate_trade_price_amounts(
        &amounts.to_sdk(),
        royalty_nft_count as usize,
    )))
}

pub fn calculate_trade_prices(
    trade_price_amounts: OfferAmounts,
    asset_info: AssetInfo,
) -> Result<Vec<TradePrice>> {
    Ok(sdk::calculate_trade_prices(
        &trade_price_amounts.to_sdk(),
        &asset_info.to_sdk(),
    )?)
}

pub fn calculate_royalty_payments(
    clvm: Clvm,
    trade_prices: OfferAmounts,
    royalties: Vec<RoyaltyInfo>,
) -> Result<RequestedPayments> {
    let payments = {
        let mut ctx = clvm.0.lock().unwrap();
        sdk::calculate_royalty_payments(&mut ctx, &trade_prices.to_sdk(), &royalties)?
    };

    Ok(RequestedPayments::from_sdk(payments, &clvm.0))
}
