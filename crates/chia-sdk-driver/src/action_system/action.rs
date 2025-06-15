use crate::{DriverError, SendAction, SpendContext, Spends};

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Send(SendAction),
}

pub trait SpendAction {
    fn spend(&self, ctx: &mut SpendContext, spends: &mut Spends) -> Result<(), DriverError>;
}
