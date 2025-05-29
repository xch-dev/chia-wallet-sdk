use bindy::Result;
use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_driver::{member_puzzle_hash, MofN};
use chia_sdk_types::{
    puzzles::{
        BlsMember, FixedPuzzleMember, K1Member, K1MemberPuzzleAssert, PasskeyMember,
        PasskeyMemberPuzzleAssert, R1Member, R1MemberPuzzleAssert, SingletonMember,
    },
    Mod,
};
use clvm_utils::TreeHash;

use crate::{K1PublicKey, R1PublicKey};

use super::{convert_restrictions, Restriction};

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
    Ok(member_puzzle_hash(
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

pub fn bls_member_hash(config: MemberConfig, public_key: PublicKey) -> Result<TreeHash> {
    member_hash(config, BlsMember::new(public_key).curry_tree_hash())
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

pub fn singleton_member_hash(config: MemberConfig, launcher_id: Bytes32) -> Result<TreeHash> {
    member_hash(config, SingletonMember::new(launcher_id).curry_tree_hash())
}

pub fn fixed_member_hash(config: MemberConfig, fixed_puzzle_hash: Bytes32) -> Result<TreeHash> {
    member_hash(
        config,
        FixedPuzzleMember::new(fixed_puzzle_hash).curry_tree_hash(),
    )
}

pub fn custom_member_hash(config: MemberConfig, inner_hash: TreeHash) -> Result<TreeHash> {
    member_hash(config, inner_hash)
}
