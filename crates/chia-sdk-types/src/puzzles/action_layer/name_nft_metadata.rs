use std::fmt::Debug;

use chia_protocol::Bytes32;
use clvm_traits::{ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError, Raw, ToClvm, ToClvmError};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NameNftMetadata {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub did_launcher_id: Option<Bytes32>,
    pub receive_puzzle_hash: Option<Bytes32>,
    pub image_uris: Vec<String>,
    pub image_hash: Bytes32,
    pub metadata_uris: Vec<String>,
    pub metadata_hash: Bytes32,
    pub license_uris: Vec<String>,
    pub license_hash: Bytes32,
}

impl<N, D: ClvmDecoder<Node = N>> FromClvm<D> for NameNftMetadata {
    fn from_clvm(decoder: &D, node: N) -> Result<Self, FromClvmError> {
        let items: Vec<(String, Raw<N>)> = FromClvm::from_clvm(decoder, node)?;
        let mut metadata = Self::default();

        for (key, Raw(ptr)) in items {
            match key.as_str() {
                "dn" => metadata.display_name = FromClvm::from_clvm(decoder, ptr)?,
                "d" => metadata.description = FromClvm::from_clvm(decoder, ptr)?,
                "did" => metadata.did_launcher_id = FromClvm::from_clvm(decoder, ptr)?,
                "ph" => metadata.receive_puzzle_hash = FromClvm::from_clvm(decoder, ptr)?,
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

impl<N, E: ClvmEncoder<Node = N>> ToClvm<E> for NameNftMetadata {
    fn to_clvm(&self, encoder: &mut E) -> Result<N, ToClvmError> {
        let mut items: Vec<(&str, Raw<N>)> = Vec::new();

        if let Some(display_name) = &self.display_name {
            items.push(("dn", Raw(display_name.to_clvm(encoder)?)));
        }
        if let Some(description) = &self.description {
            items.push(("d", Raw(description.to_clvm(encoder)?)));
        }
        if let Some(did_launcher_id) = &self.did_launcher_id {
            items.push(("did", Raw(did_launcher_id.to_clvm(encoder)?)));
        }
        if let Some(receive_puzzle_hash) = &self.receive_puzzle_hash {
            items.push(("ph", Raw(receive_puzzle_hash.to_clvm(encoder)?)));
        }

        items.extend(vec![
            ("u", Raw(self.image_uris.to_clvm(encoder)?)),
            ("h", Raw(self.image_hash.to_clvm(encoder)?)),
            ("mu", Raw(self.metadata_uris.to_clvm(encoder)?)),
            ("mh", Raw(self.metadata_hash.to_clvm(encoder)?)),
            ("lu", Raw(self.license_uris.to_clvm(encoder)?)),
            ("lh", Raw(self.license_hash.to_clvm(encoder)?)),
        ]);

        items.to_clvm(encoder)
    }
}
