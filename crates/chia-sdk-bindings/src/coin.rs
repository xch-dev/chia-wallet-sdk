use bindy::Result;
use chia_protocol::{Bytes, Bytes32, Coin, SpendBundle};
use chia_traits::Streamable;

pub trait CoinExt {
    fn coin_id(&self) -> Result<Bytes32>;
}

impl CoinExt for Coin {
    fn coin_id(&self) -> Result<Bytes32> {
        Ok(self.coin_id())
    }
}

pub trait SpendBundleExt: Sized {
    fn to_bytes(&self) -> Result<Bytes>;
    fn from_bytes(bytes: Bytes) -> Result<Self>;
    fn hash(&self) -> Result<Bytes32>;
}

impl SpendBundleExt for SpendBundle {
    fn to_bytes(&self) -> Result<Bytes> {
        Ok(Streamable::to_bytes(self)?.into())
    }

    fn from_bytes(bytes: Bytes) -> Result<Self> {
        Ok(Streamable::from_bytes(&bytes)?)
    }

    fn hash(&self) -> Result<Bytes32> {
        Ok(Streamable::hash(self).into())
    }
}
