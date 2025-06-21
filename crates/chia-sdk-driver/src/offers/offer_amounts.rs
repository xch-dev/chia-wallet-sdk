use chia_protocol::Bytes32;
use indexmap::IndexMap;

#[derive(Debug, Default, Clone)]
pub struct OfferAmounts {
    pub xch: u64,
    pub cats: IndexMap<Bytes32, u64>,
}

impl OfferAmounts {
    pub fn new() -> Self {
        Self::default()
    }
}
