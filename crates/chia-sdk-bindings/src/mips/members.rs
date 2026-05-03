use bindy::Result;
use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_driver::{MofN, P2Eip712MessageLayer, mips_puzzle_hash};
use chia_sdk_types::{
    Mod,
    puzzles::{
        BlsMember, BlsMemberPuzzleAssert, Eip712Member, FixedPuzzleMember, K1Member,
        K1MemberPuzzleAssert, PasskeyMember, PasskeyMemberPuzzleAssert, R1Member,
        R1MemberPuzzleAssert, SingletonMember, SingletonMemberWithMode,
    },
};
use clvm_utils::TreeHash;

use crate::{K1PublicKey, R1PublicKey};

use super::{Restriction, convert_restrictions};

#[derive(Default, Clone)]
pub struct MemberConfig {
    pub top_level: bool,
    pub nonce: u32,
    pub restrictions: Vec<Restriction>,
}

impl MemberConfig {
    pub fn new() -> Result<Self> {
        Ok(Self::default())
    }

    pub fn with_top_level(&self, top_level: bool) -> Result<Self> {
        Ok(Self {
            top_level,
            nonce: self.nonce,
            restrictions: self.restrictions.clone(),
        })
    }

    pub fn with_nonce(&self, nonce: u32) -> Result<Self> {
        Ok(Self {
            top_level: self.top_level,
            nonce,
            restrictions: self.restrictions.clone(),
        })
    }

    pub fn with_restrictions(&self, restrictions: Vec<Restriction>) -> Result<Self> {
        Ok(Self {
            top_level: self.top_level,
            nonce: self.nonce,
            restrictions,
        })
    }
}

fn member_hash(config: MemberConfig, inner_hash: TreeHash) -> Result<TreeHash> {
    Ok(mips_puzzle_hash(
        config.nonce.try_into().unwrap(),
        convert_restrictions(config.restrictions),
        inner_hash,
        config.top_level,
    ))
}

pub fn m_of_n_hash(config: MemberConfig, required: u32, items: Vec<TreeHash>) -> Result<TreeHash> {
    member_hash(
        config,
        MofN::new(required.try_into().unwrap(), items).inner_puzzle_hash(),
    )
}

pub fn k1_member_hash(
    config: MemberConfig,
    public_key: K1PublicKey,
    fast_forward: bool,
) -> Result<TreeHash> {
    member_hash(
        config,
        if fast_forward {
            K1MemberPuzzleAssert::new(public_key.0).curry_tree_hash()
        } else {
            K1Member::new(public_key.0).curry_tree_hash()
        },
    )
}

pub fn r1_member_hash(
    config: MemberConfig,
    public_key: R1PublicKey,
    fast_forward: bool,
) -> Result<TreeHash> {
    member_hash(
        config,
        if fast_forward {
            R1MemberPuzzleAssert::new(public_key.0).curry_tree_hash()
        } else {
            R1Member::new(public_key.0).curry_tree_hash()
        },
    )
}

pub fn bls_member_hash(
    config: MemberConfig,
    public_key: PublicKey,
    fast_forward: bool,
) -> Result<TreeHash> {
    member_hash(
        config,
        if fast_forward {
            BlsMemberPuzzleAssert::new(public_key).curry_tree_hash()
        } else {
            BlsMember::new(public_key).curry_tree_hash()
        },
    )
}

pub fn passkey_member_hash(
    config: MemberConfig,
    public_key: R1PublicKey,
    fast_forward: bool,
) -> Result<TreeHash> {
    member_hash(
        config,
        if fast_forward {
            PasskeyMemberPuzzleAssert::new(public_key.0).curry_tree_hash()
        } else {
            PasskeyMember::new(public_key.0).curry_tree_hash()
        },
    )
}

pub fn singleton_member_hash(
    config: MemberConfig,
    launcher_id: Bytes32,
    fast_forward: bool,
) -> Result<TreeHash> {
    member_hash(
        config,
        if fast_forward {
            SingletonMemberWithMode::new(launcher_id, 0b010_010).curry_tree_hash()
        } else {
            SingletonMember::new(launcher_id).curry_tree_hash()
        },
    )
}

pub fn fixed_member_hash(config: MemberConfig, fixed_puzzle_hash: Bytes32) -> Result<TreeHash> {
    member_hash(
        config,
        FixedPuzzleMember::new(fixed_puzzle_hash).curry_tree_hash(),
    )
}

/// MIPS member hash for an EIP-712-controlled secp256k1 key (CHIP-0037).
///
/// `genesis_challenge` is the network's genesis challenge; the helper derives
/// the matching CHIP-0037 prefix-and-domain-separator and type hash so callers
/// don't have to compute the EIP-712 envelope themselves.
pub fn eip712_member_hash(
    config: MemberConfig,
    genesis_challenge: Bytes32,
    public_key: K1PublicKey,
) -> Result<TreeHash> {
    let prefix_and_domain = P2Eip712MessageLayer::prefix_and_domain_separator(genesis_challenge);
    let type_hash = P2Eip712MessageLayer::type_hash();
    member_hash(
        config,
        Eip712Member::new(prefix_and_domain, type_hash, public_key.0).curry_tree_hash(),
    )
}

pub fn custom_member_hash(config: MemberConfig, inner_hash: TreeHash) -> Result<TreeHash> {
    member_hash(config, inner_hash)
}
