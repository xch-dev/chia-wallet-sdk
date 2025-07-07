use std::fmt::Debug;

use chia_protocol::Bytes32;
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatNftMetadata {
    pub ticker: String,
    pub name: String,
    pub description: String,
    pub precision: u8,
    pub hidden_puzzle_hash: Option<Bytes32>,
    pub image_uris: Vec<String>,
    pub image_hash: Bytes32,
    pub metadata_uris: Vec<String>,
    pub metadata_hash: Option<Bytes32>,
    pub license_uris: Vec<String>,
    pub license_hash: Option<Bytes32>,
}

impl Default for CatNftMetadata {
    fn default() -> Self {
        Self {
            ticker: "???".to_string(),
            name: "Unknown CAT".to_string(),
            description: "(no description provided)".to_string(),
            precision: 3,
            hidden_puzzle_hash: None,
            image_uris: Vec::default(),
            image_hash: Bytes32::default(),
            metadata_uris: Vec::default(),
            metadata_hash: None,
            license_uris: Vec::default(),
            license_hash: None,
        }
    }
}

impl CatNftMetadata {
    pub fn pretty_print(&self, prefix: &str) {
        println!("{}Ticker: {}", prefix, self.ticker);
        println!("{}Name: {}", prefix, self.name);
        println!("{}Description: {}", prefix, self.description);
        println!("{}Precision: {}", prefix, self.precision);
        println!(
            "{}Hidden Puzzle Hash: {:?}",
            prefix, self.hidden_puzzle_hash
        );
        println!("{}Image URIs: {}", prefix, self.image_uris.join(", "));
        println!("{}Image Hash: {}", prefix, self.image_hash);

        if !self.metadata_uris.is_empty() {
            println!("{}Metadata URIs: {}", prefix, self.metadata_uris.join(", "));
            if let Some(metadata_hash) = self.metadata_hash {
                println!("{prefix}Metadata Hash: {metadata_hash}");
            } else {
                println!("{prefix}Metadata Hash: None");
            }
        }

        if !self.license_uris.is_empty() {
            println!("{}License URIs: {}", prefix, self.license_uris.join(", "));
            if let Some(license_hash) = self.license_hash {
                println!("{prefix}License Hash: {license_hash}");
            } else {
                println!("{prefix}License Hash: None");
            }
        }
    }
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for CatNftMetadata {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let items: Vec<(String, Raw<N>)> = FromClvm::from_clvm(decoder, node)?;
        let mut metadata = Self::default();

        for (key, Raw(ptr)) in items {
            match key.as_str() {
                "t" => metadata.ticker = FromClvm::from_clvm(decoder, ptr)?,
                "n" => metadata.name = FromClvm::from_clvm(decoder, ptr)?,
                "d" => metadata.description = FromClvm::from_clvm(decoder, ptr)?,
                "p" => metadata.precision = FromClvm::from_clvm(decoder, ptr)?,
                "hph" => metadata.hidden_puzzle_hash = Some(FromClvm::from_clvm(decoder, ptr)?),
                "u" => metadata.image_uris = FromClvm::from_clvm(decoder, ptr)?,
                "h" => metadata.image_hash = FromClvm::from_clvm(decoder, ptr)?,
                "mu" => metadata.metadata_uris = FromClvm::from_clvm(decoder, ptr)?,
                "mh" => metadata.metadata_hash = FromClvm::from_clvm(decoder, ptr)?,
                "lu" => metadata.license_uris = FromClvm::from_clvm(decoder, ptr)?,
                "lh" => metadata.license_hash = FromClvm::from_clvm(decoder, ptr)?,
                _ => (),
            }
        }

        Ok(metadata)
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for CatNftMetadata {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let mut items: Vec<(&str, Raw<N>)> = vec![
            ("t", Raw(self.ticker.to_clvm(encoder)?)),
            ("n", Raw(self.name.to_clvm(encoder)?)),
        ];

        if !self.description.is_empty() {
            items.push(("d", Raw(self.description.to_clvm(encoder)?)));
        }

        if self.precision != 3 {
            items.push(("p", Raw(self.precision.to_clvm(encoder)?)));
        }

        if let Some(hidden_puzzle_hash) = self.hidden_puzzle_hash {
            items.push(("hph", Raw(hidden_puzzle_hash.to_clvm(encoder)?)));
        }

        items.push(("u", Raw(self.image_uris.to_clvm(encoder)?)));
        items.push(("h", Raw(self.image_hash.to_clvm(encoder)?)));

        if !self.metadata_uris.is_empty() {
            items.push(("mu", Raw(self.metadata_uris.to_clvm(encoder)?)));
            items.push(("mh", Raw(self.metadata_hash.to_clvm(encoder)?)));
        }

        if !self.license_uris.is_empty() {
            items.push(("lu", Raw(self.license_uris.to_clvm(encoder)?)));
            items.push(("lh", Raw(self.license_hash.to_clvm(encoder)?)));
        }

        items.to_clvm(encoder)
    }
}
