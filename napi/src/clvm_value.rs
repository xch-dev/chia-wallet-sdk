use chia::clvm_traits::ToClvm;
use clvmr::{Allocator, NodePtr};
use napi::bindgen_prelude::*;

use crate::{
    traits::{js_err, IntoRust},
    Program, PublicKey, Signature,
};

pub(crate) type ClvmValue = Either9<
    f64,
    BigInt,
    String,
    bool,
    ClassInstance<Program>,
    Uint8Array,
    ClassInstance<PublicKey>,
    ClassInstance<Signature>,
    Array,
>;

pub(crate) trait Allocate {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr>;
}

impl Allocate for ClvmValue {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        match self {
            Either9::A(value) => value.allocate(allocator),
            Either9::B(value) => value.allocate(allocator),
            Either9::C(value) => value.allocate(allocator),
            Either9::D(value) => value.allocate(allocator),
            Either9::E(value) => value.allocate(allocator),
            Either9::F(value) => value.allocate(allocator),
            Either9::G(value) => value.allocate(allocator),
            Either9::H(value) => value.allocate(allocator),
            Either9::I(value) => value.allocate(allocator),
        }
    }
}

impl Allocate for f64 {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        if self.is_infinite() {
            return Err(Error::from_reason("Value is infinite".to_string()));
        }

        if self.is_nan() {
            return Err(Error::from_reason("Value is NaN".to_string()));
        }

        if self.fract() != 0.0 {
            return Err(Error::from_reason(
                "Value has a fractional part".to_string(),
            ));
        }

        if self > 9_007_199_254_740_991.0 {
            return Err(Error::from_reason(
                "Value is larger than MAX_SAFE_INTEGER".to_string(),
            ));
        }

        if self < -9_007_199_254_740_991.0 {
            return Err(Error::from_reason(
                "Value is smaller than MIN_SAFE_INTEGER".to_string(),
            ));
        }

        let value = self as i64;

        if (0..=67_108_863).contains(&value) {
            allocator.new_small_number(value as u32).map_err(js_err)
        } else {
            allocator.new_number(value.into()).map_err(js_err)
        }
    }
}

impl Allocate for BigInt {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        let value = self.into_rust()?;
        allocator.new_number(value).map_err(js_err)
    }
}

impl Allocate for String {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        allocator.new_atom(self.as_bytes()).map_err(js_err)
    }
}

impl Allocate for bool {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        allocator.new_small_number(u32::from(self)).map_err(js_err)
    }
}

impl Allocate for Uint8Array {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        let value: Vec<u8> = self.into_rust()?;
        allocator.new_atom(&value).map_err(js_err)
    }
}

impl Allocate for Array {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        let mut items = Vec::with_capacity(self.len() as usize);

        for i in 0..self.len() {
            let Some(item) = self.get::<ClvmValue>(i)? else {
                return Err(Error::from_reason(format!("Item at index {i} is missing")));
            };

            items.push(item.allocate(allocator)?);
        }

        items.to_clvm(allocator).map_err(js_err)
    }
}

impl Allocate for ClassInstance<Program> {
    fn allocate(self, _allocator: &mut Allocator) -> Result<NodePtr> {
        Ok(self.ptr)
    }
}

impl Allocate for ClassInstance<PublicKey> {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        self.0.to_clvm(allocator).map_err(js_err)
    }
}

impl Allocate for ClassInstance<Signature> {
    fn allocate(self, allocator: &mut Allocator) -> Result<NodePtr> {
        self.0.to_clvm(allocator).map_err(js_err)
    }
}
