use std::borrow::Cow;

use chia_protocol::Bytes32;
use chia_puzzle_types::{singleton::SingletonStruct, LineageProof};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE: [u8; 1170] = hex!(
    "
    ff02ffff01ff04ffff04ffff10ff8209ffffff010180ffff04ff8215ffffff04
    ffff10ff822dffffff010180ffff04ff825dffffff04ff82bdffff8080808080
    80ffff02ff3cffff04ff02ffff04ffff0bffff02ff3affff04ff02ffff04ff09
    ffff04ffff02ff3effff04ff02ffff04ffff04ff09ffff04ffff02ff36ffff04
    ff02ffff04ffff30ff83047bffffff02ff3affff04ff02ffff04ff09ffff04ff
    ff02ff3effff04ff02ffff04ff05ff80808080ffff04ff830a7bffff80808080
    8080ff83167bff80ffff04ff83037bffff8080808080ff1d8080ff80808080ff
    ff04ffff02ff3affff04ff02ffff04ff0bffff04ffff0bffff0101ff0b80ffff
    04ff822bffffff04ff825bffffff04ffff02ff3affff04ff02ffff04ff17ffff
    04ffff0bffff0101ff1780ffff04ff8192ffff04ff82bbffffff04ff2fff8080
    808080808080ff8080808080808080ff808080808080ffff02ff3effff04ff02
    ffff04ffff04ffff02ff3effff04ff02ffff04ffff04ff8209ffff8213ff80ff
    80808080ffff04ffff02ff2effff04ff02ffff04ffff02ff3affff04ff02ffff
    04ff5fffff04ffff0bffff0101ff8301fbff80ffff04ff81bfff808080808080
    ff80808080ff808080ff8080808080ffff04ffff04ffff04ff28ffff04ff8213
    ffff808080ffff04ffff02ff2affff04ff02ffff04ff82017fffff04ffff02ff
    3effff04ff02ffff04ffff04ff8301fbffffff04ff829dffffff01018080ff80
    808080ffff04ff8301fbffff808080808080ffff04ffff04ff10ffff04ffff10
    ff83013dffff8202ff80ff808080ff80808080ff808080808080ffff04ffff01
    ffffff55ff463fffff333eff02ff04ffff04ff38ffff04ff05ff808080ffff04
    ffff04ff34ffff04ff05ff808080ff0b8080ffffffff02ffff03ff05ffff01ff
    0bff81f2ffff02ff26ffff04ff02ffff04ff09ffff04ffff02ff22ffff04ff02
    ffff04ff0dff80808080ff808080808080ffff0181d280ff0180ffffa04bf512
    2f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf
    97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa1
    02a12871fee210fb8619291eaea194581cbd2531e4b23759d225f6806923f632
    22a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400
    ade7c5ffff04ff24ffff04ffff02ff3affff04ff02ffff04ff05ffff04ffff0b
    ffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff17ff8080ff8080
    808080ff0bff81b2ffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff22ff
    ff04ff02ffff04ff07ff80808080ff808080808080ffffff0bff2cffff0bff2c
    ff81d2ff0580ffff0bff2cff0bff81928080ff02ffff03ff0bffff01ff30ffff
    02ff36ffff04ff02ffff04ff05ffff04ff1bff8080808080ff23ff3380ffff01
    0580ff0180ffff04ff05ffff04ffff0101ffff04ffff04ff05ff8080ff808080
    80ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ff
    ff04ff09ff80808080ffff02ff3effff04ff02ffff04ff0dff8080808080ffff
    01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    b092c8a9a97f69a906230663bffaf52a6d435ee57fd93a5e84862a1f935ea101
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct RewardDistributorStakeActionArgs {
    pub did_singleton_struct: SingletonStruct,
    pub nft_state_layer_mod_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub offer_mod_hash: Bytes32,
    pub nonce_mod_hash: Bytes32,
    pub my_p2_puzzle_hash: Bytes32,
    pub entry_slot_1st_curry_hash: Bytes32,
    pub max_second_offset: u64,
}

#[derive(FromClvm, ToClvm, Copy, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct IntermediaryCoinProof {
    pub full_puzzle_hash: Bytes32,
    #[clvm(rest)]
    pub amount: u64,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct NftLauncherProof {
    pub did_proof: LineageProof,
    #[clvm(rest)]
    pub intermediary_coin_proofs: Vec<IntermediaryCoinProof>,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct RewardDistributorStakeActionSolution {
    pub my_id: Bytes32,
    pub nft_metadata_hash: Bytes32,
    pub nft_metadata_updater_hash_hash: Bytes32,
    pub nft_transfer_porgram_hash: Bytes32,
    pub nft_launcher_proof: NftLauncherProof,
    #[clvm(rest)]
    pub entry_custody_puzzle_hash: Bytes32,
}

impl Mod for RewardDistributorStakeActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&REWARD_DISTRIBUTOR_STAKE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        REWARD_DISTRIBUTOR_STAKE_PUZZLE_HASH
    }
}

// run '(mod (NONCE INNER_PUZZLE . inner_solution) (a INNER_PUZZLE inner_solution))' -d
pub const NONCE_WRAPPER_PUZZLE: [u8; 7] = hex!("ff02ff05ff0780");
pub const NONCE_WRAPPER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "847d971ef523417d555ea9854b1612837155d34d453298defcd310774305f657"
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct NonceWrapperArgs<N, I> {
    pub nonce: N,
    pub inner_puzzle: I,
}

impl<N, I> Mod for NonceWrapperArgs<N, I> {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&NONCE_WRAPPER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        NONCE_WRAPPER_PUZZLE_HASH
    }
}
