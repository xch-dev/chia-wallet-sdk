use chia_protocol::Coin;
use indexmap::IndexMap;

use crate::{
    Cat, Did, FungibleSpends, HashedPtr, Id, Nft, OptionContract, SingletonSpends, SpendKind,
    Spendable,
};

#[derive(Debug, Default, Clone)]
pub struct Spends {
    pub xch: FungibleSpends<Coin>,
    pub cats: IndexMap<Id, FungibleSpends<Cat>>,
    pub dids: IndexMap<Id, SingletonSpends<Did<HashedPtr>>>,
    pub nfts: IndexMap<Id, SingletonSpends<Nft<HashedPtr>>>,
    pub options: IndexMap<Id, SingletonSpends<OptionContract>>,
}

impl Spends {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_xch(&mut self, coin: Coin, spend: SpendKind) {
        self.xch.items.push(Spendable::new(coin, spend));
    }

    pub fn add_cat(&mut self, cat: Cat, spend: SpendKind) {
        self.cats
            .entry(Id::Existing(cat.info.asset_id))
            .or_default()
            .items
            .push(Spendable::new(cat, spend));
    }

    pub fn add_did(&mut self, did: Did<HashedPtr>, spend: SpendKind) {
        self.dids.insert(
            Id::Existing(did.info.launcher_id),
            SingletonSpends::new(did, spend),
        );
    }

    pub fn add_nft(&mut self, nft: Nft<HashedPtr>, spend: SpendKind) {
        self.nfts.insert(
            Id::Existing(nft.info.launcher_id),
            SingletonSpends::new(nft, spend),
        );
    }

    pub fn add_option(&mut self, option: OptionContract, spend: SpendKind) {
        self.options.insert(
            Id::Existing(option.info.launcher_id),
            SingletonSpends::new(option, spend),
        );
    }
}
