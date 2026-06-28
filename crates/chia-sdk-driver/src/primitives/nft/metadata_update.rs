use chia_sdk_types::puzzles::NftMetadataUpdater;

use crate::{DriverError, Spend, SpendContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataUpdate {
    pub kind: UriKind,
    pub uri: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UriKind {
    Data,
    Metadata,
    License,
}

impl MetadataUpdate {
    pub fn spend(&self, ctx: &mut SpendContext) -> Result<Spend, DriverError> {
        let solution = ctx.alloc(&match self.kind {
            UriKind::Data => ("u", &self.uri),
            UriKind::Metadata => ("mu", &self.uri),
            UriKind::License => ("lu", &self.uri),
        })?;
        Ok(Spend::new(ctx.alloc_mod::<NftMetadataUpdater>()?, solution))
    }
}
