use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{
        PuzzleAndSolution, SlotNeigborsInfo, XchandlesDataValue, XchandlesNewDataPuzzleHashes,
    },
    Mod,
};

pub const XCHANDLES_EXPIRE_PUZZLE: [u8; 1303] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff02ff2effff04ff02ffff04ff8202
    7fff80808080ff82015f80ffff09ffff02ff2effff04ff02ffff04ff82013fff
    80808080ff8203df8080ffff01ff04ff5fffff04ffff04ff10ffff04ff8202bf
    ff808080ffff04ffff04ff10ffff04ff8205bfff808080ffff04ffff02ff3eff
    ff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff
    ff0bffff0101ff820bbf80ff8205ff80ffff04ff8205bfff820bff8080ff8080
    8080ff8080808080ffff04ffff02ff3affff04ff02ffff04ff2fffff04ffff02
    ff2effff04ff02ffff04ffff04ffff04ffff0bffff0101ff820bbf80ff8205ff
    80ffff04ffff10ffff06ffff02ff82013fff8201bf8080ff8202bf80ff8204ff
    8080ff80808080ff8080808080ffff02ff2affff04ff02ffff04ffff02ff8202
    7fffff04ffff02ff26ffff04ff02ffff04ff17ffff04ff820affffff04ffff0b
    ffff0101ffff02ff2effff04ff02ffff04ffff04ffff04ffff04ff82015fff82
    037f80ffff04ff8203dfff8201bf8080ffff04ffff04ff820bbfff820eff80ff
    ff04ff8208ffff820cff808080ff8080808080ff808080808080ff82037f8080
    ffff04ffff05ffff02ff82013fff8201bf8080ffff04ffff02ff26ffff04ff02
    ffff04ff05ffff04ffff02ff2effff04ff02ffff04ffff04ff05ffff04ff8208
    ffff0b8080ff80808080ffff04ff8217ffff808080808080ffff04ffff02ff26
    ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ffff04ff05ff
    ff04ff820cffff0b8080ff80808080ffff04ff821fffff808080808080ff8080
    80808080808080808080ffff01ff088080ff0180ffff04ffff01ffffff51ff33
    3effff4342ff02ff02ffff03ff05ffff01ff0bff72ffff02ff36ffff04ff02ff
    ff04ff09ffff04ffff02ff3cffff04ff02ffff04ff0dff80808080ff80808080
    8080ffff016280ff0180ffffffffa04bf5122f344554c53bde2ebb8cd2b7e3d1
    600ad631c385a5d7cce23c7785459aa09dcf97a184f32623d11a73124ceb99a5
    709b083721e878a16d78f596718ba7b2ffa102a12871fee210fb8619291eaea1
    94581cbd2531e4b23759d225f6806923f63222a102a8d5dd63fba471ebcb1f3e
    8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04ffff04ff34ffff04
    ffff0113ffff04ffff0101ffff04ff05ffff04ff0bff808080808080ffff04ff
    ff04ff38ffff04ffff0effff0178ff0580ff808080ffff04ffff04ff24ffff04
    ffff0112ffff04ffff0effff0165ff0580ffff04ff17ff8080808080ffff02ff
    ff03ffff09ff17ff2f80ff80ffff01ff04ffff04ff24ffff04ffff0112ffff04
    ffff0effff0166ff0580ffff04ff2fff8080808080ff808080ff0180808080ff
    04ff28ffff04ffff02ff26ffff04ff02ffff04ff05ffff04ffff0bffff0101ff
    0b80ff8080808080ffff04ff80ffff04ffff04ff05ff8080ff8080808080ffff
    ff0bff52ffff02ff36ffff04ff02ffff04ff05ffff04ffff02ff3cffff04ff02
    ffff04ff07ff80808080ff808080808080ff0bff2cffff0bff2cff62ff0580ff
    ff0bff2cff0bff428080ffff02ffff03ffff07ff0580ffff01ff0bffff0102ff
    ff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2effff04ff02ffff04
    ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff04ff34ffff04ff
    ff0112ffff04ff80ffff04ffff02ff26ffff04ff02ffff04ff05ffff04ffff0b
    ffff0101ff0b80ff8080808080ff8080808080ff018080
    "
);

pub const XCHANDLES_EXPIRE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    d97f49ef4bd91aa3ebd5501240f625f8ad5f499cdf4b252e2c8e3d2c2ad99d23
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesExpireActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_mod_hash: Bytes32,
    pub precommit_1st_curry_hash: Bytes32,
    pub handle_slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRefundAndSecret<S> {
    pub refund_puzzle_hash_hash: Bytes32,
    #[clvm(rest)]
    pub secret: S,
}

impl<S> XchandlesRefundAndSecret<S> {
    pub fn new(refund_puzzle_hash_hash: Bytes32, secret: S) -> Self {
        Self {
            refund_puzzle_hash_hash,
            secret,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesOtherPrecommitData<S> {
    pub launcher_ids: XchandlesDataValue,
    #[clvm(rest)]
    pub refund_and_secret: XchandlesRefundAndSecret<S>,
}

impl<S> XchandlesOtherPrecommitData<S> {
    pub fn new(
        owner_launcher_id: Bytes32,
        resolved_launcher_id: Bytes32,
        refund_puzzle_hash_hash: Bytes32,
        secret: S,
    ) -> Self {
        Self {
            launcher_ids: XchandlesDataValue {
                owner_launcher_id,
                resolved_launcher_id,
            },
            refund_and_secret: XchandlesRefundAndSecret::new(refund_puzzle_hash_hash, secret),
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesExpireActionSolution<CMP, CMS, EP, ES, S> {
    pub expired_handle_pricing_puzzle_and_solution: PuzzleAndSolution<EP, ES>,
    pub cat_maker_and_solution: PuzzleAndSolution<CMP, CMS>,
    pub other_precommit_data: XchandlesOtherPrecommitData<S>,
    pub neighbors: SlotNeigborsInfo,
    pub old_rest: XchandlesDataValue,
    #[clvm(rest)]
    pub new_inner_puzzle_hashes: XchandlesNewDataPuzzleHashes,
}

impl Mod for XchandlesExpireActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_EXPIRE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_EXPIRE_PUZZLE_HASH
    }
}
