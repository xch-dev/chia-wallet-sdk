use std::borrow::Cow;

use chia_protocol::Bytes32;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::{
    puzzles::{
        PuzzleHashPuzzleAndSolution, SlotNeigborsInfo, XchandlesDataValue,
        XchandlesNewDataPuzzleHashes, XchandlesOtherPrecommitData,
    },
    Mod,
};

pub const XCHANDLES_REGISTER_PUZZLE: [u8; 1567] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff09ff81bfffff0bffff0101ff825dff8080
    ffff20ff822dff80ffff0aff81bfff82027f80ffff0aff82037fff81bf80ffff
    09ff82015fffff02ff2effff04ff02ffff04ff8204ffff8080808080ffff09ff
    8202dfffff02ff2effff04ff02ffff04ff8209ffff808080808080ffff01ff04
    ff5fffff04ffff04ff10ffff04ff8215ffff808080ffff04ffff02ff3effff04
    ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff8202
    7fffff04ff8213ffff82037f8080ffff04ff822bffff823bff8080ff80808080
    ff8080808080ffff04ffff02ff3effff04ff02ffff04ff2fffff04ffff02ff2e
    ffff04ff02ffff04ffff04ffff04ff82037fffff04ff82027fff8227ff8080ff
    ff04ff8257ffff8277ff8080ff80808080ff8080808080ffff04ffff02ff3aff
    ff04ff02ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff
    81bfff82017f80ffff04ffff10ff8215ffffff06ffff02ff8209ffff820dff80
    8080ff825fff8080ff80808080ff8080808080ffff04ffff02ff3affff04ff02
    ffff04ff2fffff04ffff02ff2effff04ff02ffff04ffff04ffff04ff82027fff
    ff04ff8213ffff81bf8080ffff04ff822bffff823bff8080ff80808080ff8080
    808080ffff04ffff02ff3affff04ff02ffff04ff2fffff04ffff02ff2effff04
    ff02ffff04ffff04ffff04ff82037fffff04ff81bfff8227ff8080ffff04ff82
    57ffff8277ff8080ff80808080ff8080808080ffff02ff2affff04ff02ffff04
    ffff02ff8204ffffff04ffff02ff26ffff04ff02ffff04ff17ffff04ff82bfff
    ffff04ffff0bffff0101ffff02ff2effff04ff02ffff04ffff04ffff04ffff04
    ff82015fff8206ff80ffff04ff8202dfff820dff8080ffff04ffff04ff825dff
    ff82ffff80ffff04ff829fffff82dfff808080ff8080808080ff808080808080
    ff8206ff8080ffff04ffff05ffff02ff8209ffff820dff8080ffff04ffff02ff
    26ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04ffff04ff05
    ffff04ff829fffff0b8080ff80808080ffff04ff824fffff808080808080ffff
    04ffff02ff26ffff04ff02ffff04ff05ffff04ffff02ff2effff04ff02ffff04
    ffff04ff05ffff04ff82dfffff0b8080ff80808080ffff04ff826fffff808080
    808080ff8080808080808080808080808080ffff01ff088080ff0180ffff04ff
    ff01ffffff51ff333effff4342ff02ff02ffff03ff05ffff01ff0bff72ffff02
    ff36ffff04ff02ffff04ff09ffff04ffff02ff3cffff04ff02ffff04ff0dff80
    808080ff808080808080ffff016280ff0180ffffffffa04bf5122f344554c53b
    de2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459aa09dcf97a184f32623
    d11a73124ceb99a5709b083721e878a16d78f596718ba7b2ffa102a12871fee2
    10fb8619291eaea194581cbd2531e4b23759d225f6806923f63222a102a8d5dd
    63fba471ebcb1f3e8f7c1e1879b7152a6e7298a91ce119a63400ade7c5ffff04
    ffff04ff34ffff04ffff0113ffff04ffff0101ffff04ff05ffff04ff0bff8080
    80808080ffff04ffff04ff38ffff04ffff0effff0172ff0580ff808080ffff04
    ffff04ff24ffff04ffff0112ffff04ffff0effff0161ff0580ffff04ff17ff80
    80808080ffff02ffff03ffff09ff17ff2f80ff80ffff01ff04ffff04ff24ffff
    04ffff0112ffff04ffff0effff0162ff0580ffff04ff2fff8080808080ff8080
    80ff0180808080ff04ff28ffff04ffff02ff26ffff04ff02ffff04ff05ffff04
    ffff0bffff0101ff0b80ff8080808080ffff04ff80ffff04ffff04ff05ff8080
    ff8080808080ffffff0bff52ffff02ff36ffff04ff02ffff04ff05ffff04ffff
    02ff3cffff04ff02ffff04ff07ff80808080ff808080808080ff0bff2cffff0b
    ff2cff62ff0580ffff0bff2cff0bff428080ffff02ffff03ffff07ff0580ffff
    01ff0bffff0102ffff02ff2effff04ff02ffff04ff09ff80808080ffff02ff2e
    ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180
    ff04ff34ffff04ffff0112ffff04ff80ffff04ffff02ff26ffff04ff02ffff04
    ff05ffff04ffff0bffff0101ff0b80ff8080808080ff8080808080ff018080
    "
);

pub const XCHANDLES_REGISTER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    faba8d7baabdf69289369a7d55331bf2a0b594b6aeb70f3d872e52b123d187e6
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct XchandlesRegisterActionArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_puzzle_hash: Bytes32,
    pub precommit_1st_curry_hash: Bytes32,
    pub handle_slot_1st_curry_hash: Bytes32,
}

#[derive(FromClvm, ToClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRestOfSlot {
    pub this_this_value: Bytes32, // left_left_value or right_right_value
    pub this_expiration: u64,     // left_expiration or right_expiration
    #[clvm(rest)]
    pub this_data: XchandlesDataValue, // left_data or right_data
}

impl XchandlesRestOfSlot {
    pub fn new(
        this_this_value: Bytes32,
        this_expiration: u64,
        this_data: XchandlesDataValue,
    ) -> Self {
        Self {
            this_this_value,
            this_expiration,
            this_data,
        }
    }
}

#[derive(FromClvm, ToClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct XchandlesRegisterActionSolution<PP, PS, CMP, CMS, S> {
    pub handle_hash: Bytes32,
    pub neighbors: SlotNeigborsInfo,
    pub cat_maker_puzzle_and_solution: PuzzleHashPuzzleAndSolution<CMP, CMS>,
    pub pricing_puzzle_and_solution: PuzzleHashPuzzleAndSolution<PP, PS>,
    pub left_rest_of_slot: XchandlesRestOfSlot,
    pub right_rest_of_slot: XchandlesRestOfSlot,
    pub data_puzzle_hashes: XchandlesNewDataPuzzleHashes,
    #[clvm(rest)]
    pub other_precommit_data: XchandlesOtherPrecommitData<S>,
}

impl Mod for XchandlesRegisterActionArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&XCHANDLES_REGISTER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        XCHANDLES_REGISTER_PUZZLE_HASH
    }
}
