use std::fmt::Debug;

use chia_protocol::Bytes32;
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};

/// On-chain metadata of a `CATalog` registration NFT, which describes a CAT.
///
/// It is encoded as a CLVM list of `(key . value)` pairs. Parsing starts from
/// [`CatNftMetadata::default`] and overwrites the fields whose keys are present, ignoring
/// unknown keys. When serializing, the description is omitted if empty, the precision
/// if it is 3, the hidden puzzle hash if `None`, and the metadata and license entries if
/// their URI lists are empty.
///
/// # Example
///
/// ```rust
/// use chia_protocol::Bytes32;
/// use chia_sdk_types::puzzles::CatNftMetadata;
/// use clvm_traits::{FromClvm, ToClvm};
/// use clvmr::Allocator;
///
/// let metadata = CatNftMetadata {
///     ticker: "SBX".to_string(),
///     name: "Spacebucks".to_string(),
///     description: "Space money".to_string(),
///     image_uris: vec!["https://example.com/sbx.png".to_string()],
///     image_hash: Bytes32::new([1; 32]),
///     ..Default::default()
/// };
///
/// let mut allocator = Allocator::new();
/// let ptr = metadata.to_clvm(&mut allocator)?;
/// assert_eq!(CatNftMetadata::from_clvm(&allocator, ptr)?, metadata);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatNftMetadata {
    /// Ticker symbol (key `t`). Defaults to `???`.
    pub ticker: String,
    /// Display name (key `n`). Defaults to `Unknown CAT`.
    pub name: String,
    /// Description (key `d`). Defaults to `(no description provided)`.
    pub description: String,
    /// Number of decimal places (key `p`). Defaults to 3.
    pub precision: u8,
    /// Hidden puzzle hash of the revocation layer, for revocable CATs (key `hph`).
    pub hidden_puzzle_hash: Option<Bytes32>,
    /// Image URIs (key `u`).
    pub image_uris: Vec<String>,
    /// Hash of the image (key `h`).
    pub image_hash: Bytes32,
    /// Off-chain metadata URIs (key `mu`).
    pub metadata_uris: Vec<String>,
    /// Hash of the off-chain metadata (key `mh`). Only serialized when
    /// [`metadata_uris`](Self::metadata_uris) is non-empty.
    pub metadata_hash: Option<Bytes32>,
    /// License URIs (key `lu`).
    pub license_uris: Vec<String>,
    /// Hash of the license (key `lh`). Only serialized when
    /// [`license_uris`](Self::license_uris) is non-empty.
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
    /// Prints the metadata to stdout, one field per line, with each line starting with
    /// `prefix`.
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
