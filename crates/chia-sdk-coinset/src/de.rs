use chia::{
    bls::{G1Element, G2Element},
    protocol::{
        Bytes, Bytes100, Bytes32, ChallengeChainSubSlot, ClassgroupElement, Coin, CoinSpend,
        EndOfSubSlotBundle, Foliage, FoliageBlockData, FoliageTransactionBlock,
        InfusedChallengeChainSubSlot, PoolTarget, ProofOfSpace, RewardChainBlock,
        RewardChainSubSlot, SpendBundle, SubEpochSummary, SubSlotProofs, TransactionsInfo, VDFInfo,
        VDFProof,
    },
};
use serde::Deserialize;

pub mod hex_string_to_bytes32 {
    use chia::protocol::Bytes32;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes32, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        let bytes = <[u8; 32]>::from_hex(s.replace("0x", "")).map_err(serde::de::Error::custom)?;
        Ok(Bytes32::new(bytes))
    }
}

pub mod hex_string_to_bytes100 {
    use chia::protocol::Bytes100;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes100, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        let bytes = <[u8; 100]>::from_hex(s.replace("0x", "")).map_err(serde::de::Error::custom)?;
        Ok(Bytes100::new(bytes))
    }
}

pub mod hex_string_to_bytes {
    use chia::protocol::Bytes;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        let bytes = Vec::<u8>::from_hex(s.replace("0x", "")).map_err(serde::de::Error::custom)?;
        Ok(Bytes::new(bytes))
    }
}

pub mod hex_string_to_bytes_maybe {
    use chia::protocol::Bytes;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Bytes>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if let Ok(s) = String::deserialize(deserializer) {
            if s.is_empty() {
                return Ok(None);
            }

            let bytes =
                Vec::<u8>::from_hex(s.replace("0x", "")).map_err(serde::de::Error::custom)?;
            return Ok(Some(Bytes::new(bytes)));
        }

        Ok(None)
    }
}

pub mod hex_string_to_bytes32_maybe {
    use chia::protocol::Bytes32;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Bytes32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if let Ok(s) = String::deserialize(deserializer) {
            if s.len() != 64 && s.len() != 66 {
                return Ok(None);
            }

            let bytes =
                <[u8; 32]>::from_hex(s.replace("0x", "")).map_err(serde::de::Error::custom)?;
            return Ok(Some(Bytes32::new(bytes)));
        }

        Ok(None)
    }
}

pub mod hex_string_to_bytes32_list_maybe {
    use chia::protocol::Bytes32;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<Bytes32>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let l = Option::<Vec<String>>::deserialize(deserializer)?;
        Ok(l.map(|l| {
            l.into_iter()
                .map(|s| Bytes32::new(<[u8; 32]>::from_hex(s.replace("0x", "")).unwrap()))
                .collect()
        }))
    }
}

#[derive(Deserialize, Debug)]
pub struct DeserializableCoin {
    pub amount: u64,
    #[serde(with = "hex_string_to_bytes32")]
    pub parent_coin_info: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    pub puzzle_hash: Bytes32,
}

pub mod deserialize_coin {
    use chia::protocol::Coin;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableCoin;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Coin, D::Error>
    where
        D: Deserializer<'de>,
    {
        let coin = DeserializableCoin::deserialize(deserializer)?;
        Ok(Coin::new(
            coin.parent_coin_info,
            coin.puzzle_hash,
            coin.amount,
        ))
    }
}

pub mod deserialize_coins {
    use chia::protocol::Coin;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableCoin;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Coin>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let coins = Vec::<DeserializableCoin>::deserialize(deserializer)?;
        Ok(coins
            .into_iter()
            .map(|c| Coin::new(c.parent_coin_info, c.puzzle_hash, c.amount))
            .collect())
    }
}

pub mod deserialize_coins_maybe {
    use chia::protocol::Coin;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableCoin;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<Coin>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let coins = Option::<Vec<DeserializableCoin>>::deserialize(deserializer)?;
        Ok(coins.map(|c| {
            c.into_iter()
                .map(|c| Coin::new(c.parent_coin_info, c.puzzle_hash, c.amount))
                .collect()
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableVDFProof {
    pub witness_type: u8,
    #[serde(with = "hex_string_to_bytes")]
    pub witness: Bytes,
    pub normalized_to_identity: bool,
}

pub mod deserialize_vdf_proof {
    use chia::protocol::VDFProof;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableVDFProof;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VDFProof, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableVDFProof::deserialize(deserializer)?;
        Ok(VDFProof {
            witness_type: helper.witness_type,
            witness: helper.witness,
            normalized_to_identity: helper.normalized_to_identity,
        })
    }
}

pub mod deserialize_vdf_proof_maybe {
    use chia::protocol::VDFProof;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableVDFProof;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<VDFProof>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableVDFProof>::deserialize(deserializer)?;
        Ok(helper.map(|v| VDFProof {
            witness_type: v.witness_type,
            witness: v.witness,
            normalized_to_identity: v.normalized_to_identity,
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableClassgroupElement {
    #[serde(with = "hex_string_to_bytes100")]
    data: Bytes100,
}

pub mod deserialize_classgroup_element {
    use chia::protocol::ClassgroupElement;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableClassgroupElement;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ClassgroupElement, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableClassgroupElement::deserialize(deserializer)?;
        Ok(ClassgroupElement::new(helper.data))
    }
}

pub mod deserialize_classgroup_element_maybe {
    use chia::protocol::ClassgroupElement;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableClassgroupElement;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<ClassgroupElement>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableClassgroupElement>::deserialize(deserializer)?;
        Ok(helper.map(|h| ClassgroupElement::new(h.data)))
    }
}

#[derive(Deserialize)]
pub struct DeserializableVDFInfo {
    #[serde(with = "hex_string_to_bytes32")]
    challenge: Bytes32,
    number_of_iterations: u64,
    #[serde(with = "deserialize_classgroup_element")]
    output: ClassgroupElement,
}

pub mod deserialize_vdf_info {
    use chia::protocol::VDFInfo;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableVDFInfo;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VDFInfo, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableVDFInfo::deserialize(deserializer)?;
        Ok(VDFInfo {
            challenge: helper.challenge,
            number_of_iterations: helper.number_of_iterations,
            output: helper.output,
        })
    }
}

pub mod deserialize_vdf_info_maybe {
    use chia::protocol::VDFInfo;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableVDFInfo;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<VDFInfo>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableVDFInfo>::deserialize(deserializer)?;
        Ok(helper.map(|h| VDFInfo {
            challenge: h.challenge,
            number_of_iterations: h.number_of_iterations,
            output: h.output,
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableChallengeChainSubSlot {
    #[serde(with = "deserialize_vdf_info")]
    challenge_chain_end_of_slot_vdf: VDFInfo,
    #[serde(with = "hex_string_to_bytes32_maybe")]
    infused_challenge_chain_sub_slot_hash: Option<Bytes32>,
    #[serde(with = "hex_string_to_bytes32_maybe")]
    subepoch_summary_hash: Option<Bytes32>,
    new_sub_slot_iters: Option<u64>,
    new_difficulty: Option<u64>,
}

pub mod deserialize_challenge_chain_sub_slot {
    use chia::protocol::ChallengeChainSubSlot;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableChallengeChainSubSlot;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ChallengeChainSubSlot, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableChallengeChainSubSlot::deserialize(deserializer)?;
        Ok(ChallengeChainSubSlot {
            challenge_chain_end_of_slot_vdf: helper.challenge_chain_end_of_slot_vdf,
            infused_challenge_chain_sub_slot_hash: helper.infused_challenge_chain_sub_slot_hash,
            subepoch_summary_hash: helper.subepoch_summary_hash,
            new_sub_slot_iters: helper.new_sub_slot_iters,
            new_difficulty: helper.new_difficulty,
        })
    }
}

#[derive(Deserialize)]
pub struct DeserializableInfusedChallengeChainSubSlot {
    #[serde(with = "deserialize_vdf_info")]
    infused_challenge_chain_end_of_slot_vdf: VDFInfo,
}

pub mod deserialize_infused_challenge_chain_sub_slot_maybe {
    use chia::protocol::InfusedChallengeChainSubSlot;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableInfusedChallengeChainSubSlot;

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<InfusedChallengeChainSubSlot>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper =
            Option::<DeserializableInfusedChallengeChainSubSlot>::deserialize(deserializer)?;
        Ok(helper.map(|v| InfusedChallengeChainSubSlot {
            infused_challenge_chain_end_of_slot_vdf: v.infused_challenge_chain_end_of_slot_vdf,
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableRewardChainSubSlot {
    #[serde(with = "deserialize_vdf_info")]
    end_of_slot_vdf: VDFInfo,
    #[serde(with = "hex_string_to_bytes32")]
    challenge_chain_sub_slot_hash: Bytes32,
    #[serde(with = "hex_string_to_bytes32_maybe")]
    infused_challenge_chain_sub_slot_hash: Option<Bytes32>,
    deficit: u8,
}

pub mod deserialize_reward_chain_sub_slot {
    use chia::protocol::RewardChainSubSlot;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableRewardChainSubSlot;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<RewardChainSubSlot, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableRewardChainSubSlot::deserialize(deserializer)?;
        Ok(RewardChainSubSlot {
            end_of_slot_vdf: helper.end_of_slot_vdf,
            challenge_chain_sub_slot_hash: helper.challenge_chain_sub_slot_hash,
            infused_challenge_chain_sub_slot_hash: helper.infused_challenge_chain_sub_slot_hash,
            deficit: helper.deficit,
        })
    }
}

#[derive(Deserialize)]
pub struct DeserializableSubSlotProofs {
    #[serde(with = "deserialize_vdf_proof")]
    challenge_chain_slot_proof: VDFProof,
    #[serde(with = "deserialize_vdf_proof_maybe")]
    infused_challenge_chain_slot_proof: Option<VDFProof>,
    #[serde(with = "deserialize_vdf_proof")]
    reward_chain_slot_proof: VDFProof,
}

pub mod deserialize_sub_slot_proofs {
    use chia::protocol::SubSlotProofs;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableSubSlotProofs;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SubSlotProofs, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableSubSlotProofs::deserialize(deserializer)?;
        Ok(SubSlotProofs {
            challenge_chain_slot_proof: helper.challenge_chain_slot_proof,
            infused_challenge_chain_slot_proof: helper.infused_challenge_chain_slot_proof,
            reward_chain_slot_proof: helper.reward_chain_slot_proof,
        })
    }
}

#[derive(Deserialize)]
pub struct DeserializableEndOfSubSlotBundle {
    #[serde(with = "deserialize_challenge_chain_sub_slot")]
    pub challenge_chain: ChallengeChainSubSlot,
    #[serde(with = "deserialize_infused_challenge_chain_sub_slot_maybe")]
    pub infused_challenge_chain: Option<InfusedChallengeChainSubSlot>,
    #[serde(with = "deserialize_reward_chain_sub_slot")]
    pub reward_chain: RewardChainSubSlot,
    #[serde(with = "deserialize_sub_slot_proofs")]
    pub proofs: SubSlotProofs,
}

pub mod deserialize_end_of_sub_slot_bundles {
    use chia::protocol::EndOfSubSlotBundle;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableEndOfSubSlotBundle;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<EndOfSubSlotBundle>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Vec::<DeserializableEndOfSubSlotBundle>::deserialize(deserializer)?;
        Ok(helper
            .into_iter()
            .map(|v| EndOfSubSlotBundle {
                challenge_chain: v.challenge_chain,
                infused_challenge_chain: v.infused_challenge_chain,
                reward_chain: v.reward_chain,
                proofs: v.proofs,
            })
            .collect())
    }
}

pub mod deserialize_to_g1element {
    use chia::bls::G1Element;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<G1Element, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        let bytes = <[u8; 48]>::from_hex(s.replace("0x", "")).map_err(serde::de::Error::custom)?;
        G1Element::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }
}

pub mod deserialize_to_g1element_maybe {
    use chia::bls::G1Element;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<G1Element>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<&str> = Deserialize::deserialize(deserializer)?;
        if s.is_none() || s.unwrap().is_empty() {
            return Ok(None);
        }

        let bytes =
            <[u8; 48]>::from_hex(s.unwrap().replace("0x", "")).map_err(serde::de::Error::custom)?;
        Ok(Some(
            G1Element::from_bytes(&bytes).map_err(serde::de::Error::custom)?,
        ))
    }
}

#[derive(Deserialize)]
pub struct DeserializableProofOfSpace {
    #[serde(with = "hex_string_to_bytes32")]
    challenge: Bytes32,
    #[serde(with = "deserialize_to_g1element_maybe")]
    pool_public_key: Option<G1Element>,
    #[serde(with = "hex_string_to_bytes32_maybe")]
    pool_contract_puzzle_hash: Option<Bytes32>,
    #[serde(with = "deserialize_to_g1element")]
    plot_public_key: G1Element,
    size: u8,
    #[serde(with = "hex_string_to_bytes")]
    proof: Bytes,
}

pub mod deserialize_proof_of_space {
    use chia::protocol::ProofOfSpace;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableProofOfSpace;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ProofOfSpace, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableProofOfSpace::deserialize(deserializer)?;
        Ok(ProofOfSpace {
            challenge: helper.challenge,
            pool_public_key: helper.pool_public_key,
            pool_contract_puzzle_hash: helper.pool_contract_puzzle_hash,
            plot_public_key: helper.plot_public_key,
            size: helper.size,
            proof: helper.proof,
        })
    }
}

pub mod deserialize_g2element {
    use chia::bls::G2Element;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<G2Element, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        let bytes = <[u8; 96]>::from_hex(s.replace("0x", "")).map_err(serde::de::Error::custom)?;
        G2Element::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }
}

pub mod deserialize_g2element_maybe {
    use chia::bls::G2Element;
    use hex::FromHex;
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<G2Element>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<&str> = Deserialize::deserialize(deserializer)?;
        if s.is_none() || s.unwrap().is_empty() {
            return Ok(None);
        }

        let bytes =
            <[u8; 96]>::from_hex(s.unwrap().replace("0x", "")).map_err(serde::de::Error::custom)?;
        Ok(Some(
            G2Element::from_bytes(&bytes).map_err(serde::de::Error::custom)?,
        ))
    }
}

#[derive(Deserialize)]
pub struct DeserializableRewardChainBlock {
    weight: u128,
    height: u32,
    total_iters: u128,
    signage_point_index: u8,
    #[serde(with = "hex_string_to_bytes32")]
    pos_ss_cc_challenge_hash: Bytes32,
    #[serde(with = "deserialize_proof_of_space")]
    proof_of_space: ProofOfSpace,
    #[serde(with = "deserialize_vdf_info_maybe")]
    challenge_chain_sp_vdf: Option<VDFInfo>, // Not present for first sp in slot
    #[serde(with = "deserialize_g2element")]
    challenge_chain_sp_signature: G2Element,
    #[serde(with = "deserialize_vdf_info")]
    challenge_chain_ip_vdf: VDFInfo,
    #[serde(with = "deserialize_vdf_info_maybe")]
    reward_chain_sp_vdf: Option<VDFInfo>, // Not present for first sp in slot
    #[serde(with = "deserialize_g2element")]
    reward_chain_sp_signature: G2Element,
    #[serde(with = "deserialize_vdf_info")]
    reward_chain_ip_vdf: VDFInfo,
    #[serde(with = "deserialize_vdf_info_maybe")]
    infused_challenge_chain_ip_vdf: Option<VDFInfo>, // Iff deficit < 16
    is_transaction_block: bool,
}

pub mod deserialize_reward_chain_block {
    use chia::protocol::RewardChainBlock;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableRewardChainBlock;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<RewardChainBlock, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableRewardChainBlock::deserialize(deserializer)?;
        Ok(RewardChainBlock {
            weight: helper.weight,
            height: helper.height,
            total_iters: helper.total_iters,
            signage_point_index: helper.signage_point_index,
            pos_ss_cc_challenge_hash: helper.pos_ss_cc_challenge_hash,
            proof_of_space: helper.proof_of_space,
            challenge_chain_sp_vdf: helper.challenge_chain_sp_vdf,
            challenge_chain_sp_signature: helper.challenge_chain_sp_signature,
            challenge_chain_ip_vdf: helper.challenge_chain_ip_vdf,
            reward_chain_sp_vdf: helper.reward_chain_sp_vdf,
            reward_chain_sp_signature: helper.reward_chain_sp_signature,
            reward_chain_ip_vdf: helper.reward_chain_ip_vdf,
            infused_challenge_chain_ip_vdf: helper.infused_challenge_chain_ip_vdf,
            is_transaction_block: helper.is_transaction_block,
        })
    }
}

#[derive(Deserialize)]
pub struct DeserializablePoolTarget {
    #[serde(with = "hex_string_to_bytes32")]
    puzzle_hash: Bytes32,
    max_height: u32,
}

pub mod deserialize_pool_target {
    use chia::protocol::PoolTarget;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializablePoolTarget;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PoolTarget, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializablePoolTarget::deserialize(deserializer)?;
        Ok(PoolTarget {
            puzzle_hash: helper.puzzle_hash,
            max_height: helper.max_height,
        })
    }
}

#[derive(Deserialize)]
pub struct DeserializableFoliageBlockData {
    #[serde(with = "hex_string_to_bytes32")]
    unfinished_reward_block_hash: Bytes32,
    #[serde(with = "deserialize_pool_target")]
    pool_target: PoolTarget,
    #[serde(with = "deserialize_g2element_maybe")]
    pool_signature: Option<G2Element>,
    #[serde(with = "hex_string_to_bytes32")]
    farmer_reward_puzzle_hash: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    extension_data: Bytes32,
}

pub mod deserialize_foliage_block_data {
    use chia::protocol::FoliageBlockData;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableFoliageBlockData;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<FoliageBlockData, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableFoliageBlockData::deserialize(deserializer)?;
        Ok(FoliageBlockData {
            unfinished_reward_block_hash: helper.unfinished_reward_block_hash,
            pool_target: helper.pool_target,
            pool_signature: helper.pool_signature,
            farmer_reward_puzzle_hash: helper.farmer_reward_puzzle_hash,
            extension_data: helper.extension_data,
        })
    }
}

#[derive(Deserialize)]
pub struct DeserializableFoliage {
    #[serde(with = "hex_string_to_bytes32")]
    prev_block_hash: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    reward_block_hash: Bytes32,
    #[serde(with = "deserialize_foliage_block_data")]
    foliage_block_data: FoliageBlockData,
    #[serde(with = "deserialize_g2element")]
    foliage_block_data_signature: G2Element,
    #[serde(with = "hex_string_to_bytes32_maybe")]
    foliage_transaction_block_hash: Option<Bytes32>,
    #[serde(with = "deserialize_g2element_maybe")]
    foliage_transaction_block_signature: Option<G2Element>,
}

pub mod deserialize_foliage {
    use chia::protocol::Foliage;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableFoliage;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Foliage, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableFoliage::deserialize(deserializer)?;
        Ok(Foliage {
            prev_block_hash: helper.prev_block_hash,
            reward_block_hash: helper.reward_block_hash,
            foliage_block_data: helper.foliage_block_data,
            foliage_block_data_signature: helper.foliage_block_data_signature,
            foliage_transaction_block_hash: helper.foliage_transaction_block_hash,
            foliage_transaction_block_signature: helper.foliage_transaction_block_signature,
        })
    }
}

#[derive(Deserialize)]
pub struct DeserializableFoliageTransactionBlock {
    #[serde(with = "hex_string_to_bytes32")]
    prev_transaction_block_hash: Bytes32,
    timestamp: u64,
    #[serde(with = "hex_string_to_bytes32")]
    filter_hash: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    additions_root: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    removals_root: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    transactions_info_hash: Bytes32,
}

pub mod deserialize_foliage_transaction_block_maybe {
    use chia::protocol::FoliageTransactionBlock;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableFoliageTransactionBlock;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<FoliageTransactionBlock>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableFoliageTransactionBlock>::deserialize(deserializer)?;
        Ok(helper.map(|h| FoliageTransactionBlock {
            prev_transaction_block_hash: h.prev_transaction_block_hash,
            timestamp: h.timestamp,
            filter_hash: h.filter_hash,
            additions_root: h.additions_root,
            removals_root: h.removals_root,
            transactions_info_hash: h.transactions_info_hash,
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableTransactionsInfo {
    #[serde(with = "hex_string_to_bytes32")]
    generator_root: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    generator_refs_root: Bytes32,
    #[serde(with = "deserialize_g2element")]
    aggregated_signature: G2Element,
    fees: u64,
    cost: u64,
    #[serde(with = "deserialize_coins")]
    reward_claims_incorporated: Vec<Coin>,
}

pub mod deserialize_transactions_info_maybe {
    use chia::protocol::TransactionsInfo;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableTransactionsInfo;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<TransactionsInfo>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableTransactionsInfo>::deserialize(deserializer)?;
        Ok(helper.map(|h| TransactionsInfo {
            generator_root: h.generator_root,
            generator_refs_root: h.generator_refs_root,
            aggregated_signature: h.aggregated_signature,
            fees: h.fees,
            cost: h.cost,
            reward_claims_incorporated: h.reward_claims_incorporated,
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableFullBlock {
    #[serde(with = "deserialize_end_of_sub_slot_bundles")]
    finished_sub_slots: Vec<EndOfSubSlotBundle>,
    #[serde(with = "deserialize_reward_chain_block")]
    reward_chain_block: RewardChainBlock,
    #[serde(with = "deserialize_vdf_proof_maybe")]
    challenge_chain_sp_proof: Option<VDFProof>,
    #[serde(with = "deserialize_vdf_proof")]
    challenge_chain_ip_proof: VDFProof,
    #[serde(with = "deserialize_vdf_proof_maybe")]
    reward_chain_sp_proof: Option<VDFProof>,
    #[serde(with = "deserialize_vdf_proof")]
    reward_chain_ip_proof: VDFProof,
    #[serde(with = "deserialize_vdf_proof_maybe")]
    infused_challenge_chain_ip_proof: Option<VDFProof>,
    #[serde(with = "deserialize_foliage")]
    foliage: Foliage,
    #[serde(with = "deserialize_foliage_transaction_block_maybe")]
    foliage_transaction_block: Option<FoliageTransactionBlock>,
    #[serde(with = "deserialize_transactions_info_maybe")]
    transactions_info: Option<TransactionsInfo>,
    #[serde(with = "hex_string_to_bytes_maybe")]
    transactions_generator: Option<Bytes>,
    transactions_generator_ref_list: Vec<u32>,
}

pub mod deserialize_full_block_maybe {
    use super::*;

    use chia::protocol::{FullBlock, Program};
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<FullBlock>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableFullBlock>::deserialize(deserializer)?;
        Ok(helper.map(|h| FullBlock {
            finished_sub_slots: h.finished_sub_slots,
            reward_chain_block: h.reward_chain_block,
            challenge_chain_sp_proof: h.challenge_chain_sp_proof,
            challenge_chain_ip_proof: h.challenge_chain_ip_proof,
            reward_chain_sp_proof: h.reward_chain_sp_proof,
            reward_chain_ip_proof: h.reward_chain_ip_proof,
            infused_challenge_chain_ip_proof: h.infused_challenge_chain_ip_proof,
            foliage: h.foliage,
            foliage_transaction_block: h.foliage_transaction_block,
            transactions_info: h.transactions_info,
            transactions_generator: h.transactions_generator.map(Program::from),
            transactions_generator_ref_list: h.transactions_generator_ref_list,
        }))
    }
}

pub mod deserialize_full_blocks_maybe {
    use chia::protocol::{FullBlock, Program};
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableFullBlock;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<FullBlock>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<Vec<DeserializableFullBlock>>::deserialize(deserializer)?;
        Ok(helper.map(|h| {
            h.into_iter()
                .map(|h| FullBlock {
                    finished_sub_slots: h.finished_sub_slots,
                    reward_chain_block: h.reward_chain_block,
                    challenge_chain_sp_proof: h.challenge_chain_sp_proof,
                    challenge_chain_ip_proof: h.challenge_chain_ip_proof,
                    reward_chain_sp_proof: h.reward_chain_sp_proof,
                    reward_chain_ip_proof: h.reward_chain_ip_proof,
                    infused_challenge_chain_ip_proof: h.infused_challenge_chain_ip_proof,
                    foliage: h.foliage,
                    foliage_transaction_block: h.foliage_transaction_block,
                    transactions_info: h.transactions_info,
                    transactions_generator: h.transactions_generator.map(Program::from),
                    transactions_generator_ref_list: h.transactions_generator_ref_list,
                })
                .collect()
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableSubEpochSummary {
    #[serde(with = "hex_string_to_bytes32")]
    prev_subepoch_summary_hash: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    reward_chain_hash: Bytes32,
    num_blocks_overflow: u8,
    new_difficulty: Option<u64>,
    new_sub_slot_iters: Option<u64>,
}

pub mod deserialize_sub_epoch_summary_maybe {
    use chia::protocol::SubEpochSummary;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableSubEpochSummary;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<SubEpochSummary>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableSubEpochSummary>::deserialize(deserializer)?;
        Ok(helper.map(|h| SubEpochSummary {
            prev_subepoch_summary_hash: h.prev_subepoch_summary_hash,
            reward_chain_hash: h.reward_chain_hash,
            num_blocks_overflow: h.num_blocks_overflow,
            new_difficulty: h.new_difficulty,
            new_sub_slot_iters: h.new_sub_slot_iters,
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableBlockRecord {
    #[serde(with = "hex_string_to_bytes32")]
    header_hash: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    prev_hash: Bytes32,
    height: u32,
    weight: u128,
    total_iters: u128,
    signage_point_index: u8,
    #[serde(with = "deserialize_classgroup_element")]
    challenge_vdf_output: ClassgroupElement,
    #[serde(with = "deserialize_classgroup_element_maybe")]
    infused_challenge_vdf_output: Option<ClassgroupElement>,
    #[serde(with = "hex_string_to_bytes32")]
    reward_infusion_new_challenge: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    challenge_block_info_hash: Bytes32,
    sub_slot_iters: u64,
    #[serde(with = "hex_string_to_bytes32")]
    pool_puzzle_hash: Bytes32,
    #[serde(with = "hex_string_to_bytes32")]
    farmer_puzzle_hash: Bytes32,
    required_iters: u64,
    deficit: u8,
    overflow: bool,
    prev_transaction_block_height: u32,
    timestamp: Option<u64>,
    #[serde(with = "hex_string_to_bytes32_maybe")]
    prev_transaction_block_hash: Option<Bytes32>,
    fees: Option<u64>,
    #[serde(with = "deserialize_coins_maybe")]
    reward_claims_incorporated: Option<Vec<Coin>>,
    #[serde(with = "hex_string_to_bytes32_list_maybe")]
    finished_challenge_slot_hashes: Option<Vec<Bytes32>>,
    #[serde(with = "hex_string_to_bytes32_list_maybe")]
    finished_infused_challenge_slot_hashes: Option<Vec<Bytes32>>,
    #[serde(with = "hex_string_to_bytes32_list_maybe")]
    finished_reward_slot_hashes: Option<Vec<Bytes32>>,
    #[serde(with = "deserialize_sub_epoch_summary_maybe")]
    sub_epoch_summary_included: Option<SubEpochSummary>,
}

pub mod deserialize_block_record {
    use chia::protocol::BlockRecord;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableBlockRecord;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BlockRecord, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableBlockRecord::deserialize(deserializer)?;
        Ok(BlockRecord {
            header_hash: helper.header_hash,
            prev_hash: helper.prev_hash,
            height: helper.height,
            weight: helper.weight,
            total_iters: helper.total_iters,
            signage_point_index: helper.signage_point_index,
            challenge_vdf_output: helper.challenge_vdf_output,
            infused_challenge_vdf_output: helper.infused_challenge_vdf_output,
            reward_infusion_new_challenge: helper.reward_infusion_new_challenge,
            challenge_block_info_hash: helper.challenge_block_info_hash,
            sub_slot_iters: helper.sub_slot_iters,
            pool_puzzle_hash: helper.pool_puzzle_hash,
            farmer_puzzle_hash: helper.farmer_puzzle_hash,
            required_iters: helper.required_iters,
            deficit: helper.deficit,
            overflow: helper.overflow,
            prev_transaction_block_height: helper.prev_transaction_block_height,
            timestamp: helper.timestamp,
            prev_transaction_block_hash: helper.prev_transaction_block_hash,
            fees: helper.fees,
            reward_claims_incorporated: helper.reward_claims_incorporated,
            finished_challenge_slot_hashes: helper.finished_challenge_slot_hashes,
            finished_infused_challenge_slot_hashes: helper.finished_infused_challenge_slot_hashes,
            finished_reward_slot_hashes: helper.finished_reward_slot_hashes,
            sub_epoch_summary_included: helper.sub_epoch_summary_included,
        })
    }
}

pub mod deserialize_block_record_maybe {
    use chia::protocol::BlockRecord;
    use serde::{Deserialize, Deserializer};

    use super::DeserializableBlockRecord;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<BlockRecord>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableBlockRecord>::deserialize(deserializer)?;
        Ok(helper.map(|h| BlockRecord {
            header_hash: h.header_hash,
            prev_hash: h.prev_hash,
            height: h.height,
            weight: h.weight,
            total_iters: h.total_iters,
            signage_point_index: h.signage_point_index,
            challenge_vdf_output: h.challenge_vdf_output,
            infused_challenge_vdf_output: h.infused_challenge_vdf_output,
            reward_infusion_new_challenge: h.reward_infusion_new_challenge,
            challenge_block_info_hash: h.challenge_block_info_hash,
            sub_slot_iters: h.sub_slot_iters,
            pool_puzzle_hash: h.pool_puzzle_hash,
            farmer_puzzle_hash: h.farmer_puzzle_hash,
            required_iters: h.required_iters,
            deficit: h.deficit,
            overflow: h.overflow,
            prev_transaction_block_height: h.prev_transaction_block_height,
            timestamp: h.timestamp,
            prev_transaction_block_hash: h.prev_transaction_block_hash,
            fees: h.fees,
            reward_claims_incorporated: h.reward_claims_incorporated,
            finished_challenge_slot_hashes: h.finished_challenge_slot_hashes,
            finished_infused_challenge_slot_hashes: h.finished_infused_challenge_slot_hashes,
            finished_reward_slot_hashes: h.finished_reward_slot_hashes,
            sub_epoch_summary_included: h.sub_epoch_summary_included,
        }))
    }
}

pub mod deserialize_block_records_maybe {
    use chia::protocol::BlockRecord;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableBlockRecord;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<BlockRecord>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<Vec<DeserializableBlockRecord>>::deserialize(deserializer)?;
        Ok(helper.map(|h| {
            h.into_iter()
                .map(|h| BlockRecord {
                    header_hash: h.header_hash,
                    prev_hash: h.prev_hash,
                    height: h.height,
                    weight: h.weight,
                    total_iters: h.total_iters,
                    signage_point_index: h.signage_point_index,
                    challenge_vdf_output: h.challenge_vdf_output,
                    infused_challenge_vdf_output: h.infused_challenge_vdf_output,
                    reward_infusion_new_challenge: h.reward_infusion_new_challenge,
                    challenge_block_info_hash: h.challenge_block_info_hash,
                    sub_slot_iters: h.sub_slot_iters,
                    pool_puzzle_hash: h.pool_puzzle_hash,
                    farmer_puzzle_hash: h.farmer_puzzle_hash,
                    required_iters: h.required_iters,
                    deficit: h.deficit,
                    overflow: h.overflow,
                    prev_transaction_block_height: h.prev_transaction_block_height,
                    timestamp: h.timestamp,
                    prev_transaction_block_hash: h.prev_transaction_block_hash,
                    fees: h.fees,
                    reward_claims_incorporated: h.reward_claims_incorporated,
                    finished_challenge_slot_hashes: h.finished_challenge_slot_hashes,
                    finished_infused_challenge_slot_hashes: h
                        .finished_infused_challenge_slot_hashes,
                    finished_reward_slot_hashes: h.finished_reward_slot_hashes,
                    sub_epoch_summary_included: h.sub_epoch_summary_included,
                })
                .collect()
        }))
    }
}

#[derive(Deserialize)]
pub struct DeserializableCoinSpend {
    #[serde(with = "deserialize_coin")]
    coin: Coin,
    #[serde(with = "hex_string_to_bytes")]
    puzzle_reveal: Bytes,
    #[serde(with = "hex_string_to_bytes")]
    solution: Bytes,
}

pub mod deserialize_coin_spend_maybe {
    use chia::protocol::{CoinSpend, Program};
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableCoinSpend;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<CoinSpend>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<DeserializableCoinSpend>::deserialize(deserializer)?;
        Ok(helper.map(|h| CoinSpend {
            coin: h.coin,
            puzzle_reveal: Program::from(h.puzzle_reveal),
            solution: Program::from(h.solution),
        }))
    }
}

pub mod deserialize_coin_spends_maybe {
    use chia::protocol::{CoinSpend, Program};
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableCoinSpend;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<CoinSpend>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Option::<Vec<DeserializableCoinSpend>>::deserialize(deserializer)?;
        Ok(helper.map(|h| {
            h.into_iter()
                .map(|h| CoinSpend {
                    coin: h.coin,
                    puzzle_reveal: Program::from(h.puzzle_reveal),
                    solution: Program::from(h.solution),
                })
                .collect()
        }))
    }
}

pub mod deserialize_coin_spends {
    use chia::protocol::{CoinSpend, Program};
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableCoinSpend;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<CoinSpend>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = Vec::<DeserializableCoinSpend>::deserialize(deserializer)?;
        Ok(helper
            .into_iter()
            .map(|h| CoinSpend {
                coin: h.coin,
                puzzle_reveal: Program::from(h.puzzle_reveal),
                solution: Program::from(h.solution),
            })
            .collect())
    }
}

#[derive(Deserialize, Debug)]
pub struct CoinRecord {
    #[serde(with = "deserialize_coin")]
    pub coin: Coin,
    pub coinbase: bool,
    pub confirmed_block_index: u32,
    pub spent: bool,
    pub spent_block_index: u32,
    pub timestamp: u64,
}

#[derive(Deserialize)]
pub struct DeserializableSpendBundle {
    #[serde(with = "deserialize_g2element")]
    pub aggregated_signature: G2Element,
    #[serde(with = "deserialize_coin_spends")]
    pub coin_spends: Vec<CoinSpend>,
}

pub mod deserialize_spend_bundle {
    use chia::{bls::Signature, protocol::SpendBundle};
    use serde::de::Error;
    use serde::{Deserialize, Deserializer};

    use crate::DeserializableSpendBundle;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SpendBundle, D::Error>
    where
        D: Deserializer<'de>,
    {
        let helper = DeserializableSpendBundle::deserialize(deserializer)?;
        Ok(SpendBundle {
            aggregated_signature: Signature::from_bytes(&helper.aggregated_signature.to_bytes())
                .map_err(|e| D::Error::custom(e.to_string()))?,
            coin_spends: helper.coin_spends,
        })
    }
}

#[derive(Deserialize)]
pub struct DeserializableMempoolItem {
    #[serde(with = "deserialize_spend_bundle")]
    pub spend_bundle: SpendBundle,
    pub fee: u64,
}
