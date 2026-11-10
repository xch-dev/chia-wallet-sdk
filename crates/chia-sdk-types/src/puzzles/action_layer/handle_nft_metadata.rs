use std::fmt::Debug;

use chia_protocol::Bytes32;
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};

/// Canonical on-chain metadata of an `XCHandles` handle NFT.
///
/// It is encoded as a CLVM list of `(key . value)` pairs containing only `dn`, `u`/`h`,
/// `mu`/`mh`, and `lu`/`lh`, in that order. Empty URI lists and `None` values are
/// omitted, so the default value serializes as exactly CLVM nil and parsing nil returns
/// the default. Unknown keys are ignored when parsing.
///
/// # Example
///
/// ```rust
/// use chia_sdk_types::puzzles::HandleNftMetadata;
/// use clvm_traits::{FromClvm, ToClvm};
/// use clvmr::{Allocator, serde::node_to_bytes};
///
/// let mut allocator = Allocator::new();
/// let ptr = HandleNftMetadata::default().to_clvm(&mut allocator)?;
/// assert_eq!(node_to_bytes(&allocator, ptr)?, [0x80]);
///
/// let metadata = HandleNftMetadata {
///     display_name: Some("alice".to_string()),
///     ..Default::default()
/// };
/// let ptr = metadata.to_clvm(&mut allocator)?;
/// assert_eq!(HandleNftMetadata::from_clvm(&allocator, ptr)?, metadata);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HandleNftMetadata {
    /// Display name of the handle (key `dn`).
    pub display_name: Option<String>,
    /// Image URIs (key `u`).
    pub image_uris: Vec<String>,
    /// Hash of the image (key `h`).
    pub image_hash: Option<Bytes32>,
    /// Off-chain metadata URIs (key `mu`).
    pub metadata_uris: Vec<String>,
    /// Hash of the off-chain metadata (key `mh`).
    pub metadata_hash: Option<Bytes32>,
    /// License URIs (key `lu`).
    pub license_uris: Vec<String>,
    /// Hash of the license (key `lh`).
    pub license_hash: Option<Bytes32>,
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for HandleNftMetadata {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let items: Vec<(String, Raw<N>)> = FromClvm::from_clvm(decoder, node)?;
        let mut metadata = Self::default();

        for (key, Raw(ptr)) in items {
            match key.as_str() {
                "dn" => metadata.display_name = FromClvm::from_clvm(decoder, ptr)?,
                "u" => metadata.image_uris = FromClvm::from_clvm(decoder, ptr)?,
                "h" => metadata.image_hash = Some(FromClvm::from_clvm(decoder, ptr)?),
                "mu" => metadata.metadata_uris = FromClvm::from_clvm(decoder, ptr)?,
                "mh" => metadata.metadata_hash = Some(FromClvm::from_clvm(decoder, ptr)?),
                "lu" => metadata.license_uris = FromClvm::from_clvm(decoder, ptr)?,
                "lh" => metadata.license_hash = Some(FromClvm::from_clvm(decoder, ptr)?),
                _ => (),
            }
        }

        Ok(metadata)
    }
}

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for HandleNftMetadata {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let mut items: Vec<(&str, Raw<N>)> = Vec::new();

        if let Some(display_name) = &self.display_name {
            items.push(("dn", Raw(display_name.to_clvm(encoder)?)));
        }

        if !self.image_uris.is_empty() {
            items.push(("u", Raw(self.image_uris.to_clvm(encoder)?)));
        }
        if let Some(image_hash) = self.image_hash {
            items.push(("h", Raw(image_hash.to_clvm(encoder)?)));
        }

        if !self.metadata_uris.is_empty() {
            items.push(("mu", Raw(self.metadata_uris.to_clvm(encoder)?)));
        }
        if let Some(metadata_hash) = self.metadata_hash {
            items.push(("mh", Raw(metadata_hash.to_clvm(encoder)?)));
        }

        if !self.license_uris.is_empty() {
            items.push(("lu", Raw(self.license_uris.to_clvm(encoder)?)));
        }
        if let Some(license_hash) = self.license_hash {
            items.push(("lh", Raw(license_hash.to_clvm(encoder)?)));
        }

        items.to_clvm(encoder)
    }
}

#[cfg(test)]
mod tests {
    use clvmr::{
        Allocator,
        serde::{node_from_bytes, node_to_bytes},
    };
    use hex_literal::hex;

    use super::*;

    /// Exact CLVM nil.
    const BLANK_HANDLE_NFT_METADATA: [u8; 1] = hex!("80");

    /// Representative fully populated Handle NFT Metadata in canonical field order:
    /// `dn`, `u`, `h`, `mu`, `mh`, `lu`, `lh`.
    const POPULATED_HANDLE_NFT_METADATA: [u8; 228] = hex!(
        "
        ffff82646e85616c696365
        ffff75ff9968747470733a2f2f6578616d706c652e636f6d2f612e706e6780
        ffff68a01111111111111111111111111111111111111111111111111111111111111111
        ffff826d75ff9a68747470733a2f2f6578616d706c652e636f6d2f612e6a736f6e80
        ffff826d68a02222222222222222222222222222222222222222222222222222222222222222
        ffff826c75ff9f68747470733a2f2f6578616d706c652e636f6d2f6c6963656e73652e74787480
        ffff826c68a0333333333333333333333333333333333333333333333333333333333333333380
        "
    );

    fn representative_populated() -> HandleNftMetadata {
        HandleNftMetadata {
            display_name: Some("alice".to_string()),
            image_uris: vec!["https://example.com/a.png".to_string()],
            image_hash: Some(Bytes32::from([0x11; 32])),
            metadata_uris: vec!["https://example.com/a.json".to_string()],
            metadata_hash: Some(Bytes32::from([0x22; 32])),
            license_uris: vec!["https://example.com/license.txt".to_string()],
            license_hash: Some(Bytes32::from([0x33; 32])),
        }
    }

    #[test]
    fn default_serializes_to_exact_clvm_nil() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ptr = HandleNftMetadata::default().to_clvm(&mut allocator)?;
        assert_eq!(node_to_bytes(&allocator, ptr)?, BLANK_HANDLE_NFT_METADATA);
        Ok(())
    }

    #[test]
    fn exact_clvm_nil_parses_to_default() -> anyhow::Result<()> {
        let mut allocator = Allocator::new();
        let ptr = node_from_bytes(&mut allocator, &BLANK_HANDLE_NFT_METADATA)?;
        let parsed = HandleNftMetadata::from_clvm(&allocator, ptr)?;
        assert_eq!(parsed, HandleNftMetadata::default());
        Ok(())
    }

    #[test]
    fn populated_round_trips_with_locked_byte_encoding() -> anyhow::Result<()> {
        let metadata = representative_populated();

        let mut allocator = Allocator::new();
        let ptr = metadata.to_clvm(&mut allocator)?;
        let bytes = node_to_bytes(&allocator, ptr)?;
        assert_eq!(bytes, POPULATED_HANDLE_NFT_METADATA);

        let mut allocator = Allocator::new();
        let ptr = node_from_bytes(&mut allocator, &POPULATED_HANDLE_NFT_METADATA)?;
        let parsed = HandleNftMetadata::from_clvm(&allocator, ptr)?;
        assert_eq!(parsed, metadata);

        let mut allocator = Allocator::new();
        let round_trip_ptr = parsed.to_clvm(&mut allocator)?;
        assert_eq!(
            node_to_bytes(&allocator, round_trip_ptr)?,
            POPULATED_HANDLE_NFT_METADATA
        );
        Ok(())
    }
}
