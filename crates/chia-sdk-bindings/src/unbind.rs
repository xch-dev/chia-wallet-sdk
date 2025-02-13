use super::Result;

pub trait Unbind: Sized {
    type Bound;

    fn unbind(value: Self::Bound) -> Result<Self>;
}

impl Unbind for String {
    type Bound = String;

    fn unbind(value: Self::Bound) -> Result<Self> {
        Ok(value)
    }
}
