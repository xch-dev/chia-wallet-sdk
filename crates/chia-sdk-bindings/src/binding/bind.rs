use super::Result;

pub(crate) trait Bind<T> {
    fn bind(self) -> Result<T>;
}
