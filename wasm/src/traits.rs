use chia_sdk_bindings::{AddressInfo, Bytes, BytesImpl, Coin, CoinSpend, Error, Program, Result};

pub trait IntoRust<T> {
    fn rust(self) -> Result<T>;
}

pub trait IntoJs {
    type Js;

    fn js(self) -> Result<Self::Js>;
}

impl<const N: usize> IntoRust<BytesImpl<N>> for Vec<u8> {
    fn rust(self) -> Result<BytesImpl<N>> {
        if self.len() != N {
            return Err(Error::WrongLength {
                expected: N,
                found: self.len(),
            });
        }
        Ok(BytesImpl::new(self.try_into().unwrap()))
    }
}

impl<const N: usize> IntoJs for BytesImpl<N> {
    type Js = Vec<u8>;

    fn js(self) -> Result<Self::Js> {
        Ok(self.into())
    }
}

impl IntoRust<Bytes> for Vec<u8> {
    fn rust(self) -> Result<Bytes> {
        Ok(Bytes::new(self))
    }
}

impl IntoJs for Bytes {
    type Js = Vec<u8>;

    fn js(self) -> Result<Self::Js> {
        Ok(self.into())
    }
}

impl IntoRust<Program> for Vec<u8> {
    fn rust(self) -> Result<Program> {
        Ok(Program::from(self))
    }
}

impl IntoJs for Program {
    type Js = Vec<u8>;

    fn js(self) -> Result<Self::Js> {
        Ok(self.into())
    }
}

impl IntoRust<AddressInfo> for crate::AddressInfo {
    fn rust(self) -> Result<AddressInfo> {
        Ok(AddressInfo {
            puzzle_hash: self.puzzle_hash.rust()?,
            prefix: self.prefix,
        })
    }
}

impl IntoJs for AddressInfo {
    type Js = crate::AddressInfo;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            puzzle_hash: self.puzzle_hash.js()?,
            prefix: self.prefix,
        })
    }
}

impl IntoRust<CoinSpend> for crate::CoinSpend {
    fn rust(self) -> Result<CoinSpend> {
        Ok(CoinSpend {
            coin: self.coin.rust()?,
            puzzle_reveal: self.puzzle_reveal.rust()?,
            solution: self.solution.rust()?,
        })
    }
}

impl IntoJs for CoinSpend {
    type Js = crate::CoinSpend;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            coin: self.coin.js()?,
            puzzle_reveal: self.puzzle_reveal.js()?,
            solution: self.solution.js()?,
        })
    }
}

impl IntoRust<Coin> for crate::Coin {
    fn rust(self) -> Result<Coin> {
        Ok(Coin {
            parent_coin_info: self.parent_coin_info.rust()?,
            puzzle_hash: self.puzzle_hash.rust()?,
            amount: self.amount,
        })
    }
}

impl IntoJs for Coin {
    type Js = crate::Coin;

    fn js(self) -> Result<Self::Js> {
        Ok(Self::Js {
            parent_coin_info: self.parent_coin_info.js()?,
            puzzle_hash: self.puzzle_hash.js()?,
            amount: self.amount,
        })
    }
}
