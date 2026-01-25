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

pub const XCHANDLES_EXPIRE_PUZZLE: [u8; 1320] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ffff02ff2effff04ff02ffff04ff8204
    bfff80808080ff82015f80ffff09ffff02ff2effff04ff02ffff04ff82023fff
    80808080ff8203df8080ffff01ff04ff5fffff04ffff04ff10ffff04ff82053f
    ff808080ffff04ffff04ff10ffff04ff820b3fff808080ffff04ffff02ff3eff
    ff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff
    ff0bffff0101ff82173f80ff820bbf80ffff04ff820b3fff8217bf8080ff8080
    8080ff8080808080ffff04ffff02ff3affff04ff02ffff04ff2fffff04ffff02
    ff2effff04ff02ffff04ffff04ffff04ffff0bffff0101ff82173f80ff820bbf
    80ffff04ffff10ffff06ffff02ff82023fff82033f8080ff82053f80ff8209bf
    8080ff80808080ff8080808080ffff02ff2affff04ff02ffff04ffff02ff8204
    bfffff04ffff02ff26ffff04ff02ffff04ff17ffff04ff8215bfffff04ffff0b
    ffff0101ffff02ff2effff04ff02ffff04ffff04ffff04ffff04ff82015fff82
    06bf80ffff04ff8203dfff82033f8080ffff04ffff04ff82173fff821dbf80ff
    ff04ff8211bfffff01916e65775f7265736f6c7665645f64617461808080ff80
    80808080ff808080808080ff8206bf8080ffff04ffff05ffff02ff82023fff82
    033f8080ffff04ffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff2effff
    04ff02ffff04ffff04ff05ffff04ff8211bfff0b8080ff80808080ffff04ff82
    2fbfff808080808080ffff04ffff02ff26ffff04ff02ffff04ff05ffff04ffff
    02ff2effff04ff02ffff04ffff04ff05ffff04ff8219bfff0b8080ff80808080
    ffff04ff823fbfff808080808080ff808080808080808080808080ffff01ff08
    8080ff0180ffff04ffff01ffffff51ff333effff4342ff02ff02ffff03ff05ff
    ff01ff0bff72ffff02ff36ffff04ff02ffff04ff09ffff04ffff02ff3cffff04
    ff02ffff04ff0dff80808080ff808080808080ffff016280ff0180ffffffffa0
    4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a
    a09dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7
    b2ffa102a12871fee210fb8619291eaea194581cbd2531e4b23759d225f68069
    23f63222a102a8d5dd63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119
    a63400ade7c5ffff04ffff04ff34ffff04ffff0113ffff04ffff0101ffff04ff
    05ffff04ff0bff808080808080ffff04ffff04ff38ffff04ffff0effff0178ff
    0580ff808080ffff04ffff04ff24ffff04ffff0112ffff04ffff0effff0161ff
    0580ffff04ff17ff8080808080ffff02ffff03ffff09ff17ff2f80ff80ffff01
    ff04ffff04ff24ffff04ffff0112ffff04ffff0effff0162ff0580ffff04ff2f
    ff8080808080ff808080ff0180808080ff04ff28ffff04ffff02ff26ffff04ff
    02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff
    04ffff04ff05ff8080ff8080808080ffffff0bff52ffff02ff36ffff04ff02ff
    ff04ff05ffff04ffff02ff3cffff04ff02ffff04ff07ff80808080ff80808080
    8080ff0bff2cffff0bff2cff62ff0580ffff0bff2cff0bff428080ffff02ffff
    03ffff07ff0580ffff01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff
    80808080ffff02ff2effff04ff02ffff04ff0dff8080808080ffff01ff0bffff
    0101ff058080ff0180ff04ff34ffff04ffff0112ffff04ff80ffff04ffff02ff
    26ffff04ff02ffff04ff05ffff04ffff0bffff0101ff0b80ff8080808080ff80
    80808080ff018080
    "
);

pub const XCHANDLES_EXPIRE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    e5b570b6f2e426b17cf6b73c9dfa350d2d56eb7ff761a894f57a6ca2dc9b8954
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
    pub cat_maker_data: PuzzleAndSolution<CMP, CMS>,
    pub expired_handle_pricing_puzzle_data: PuzzleAndSolution<EP, ES>,
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
