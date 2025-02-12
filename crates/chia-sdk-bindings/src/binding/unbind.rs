use super::Result;

pub(crate) trait Unbind: Sized {
    type Bound;

    fn unbind(value: Self::Bound) -> Result<Self>;
}
