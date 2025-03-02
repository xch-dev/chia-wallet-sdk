use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceAssertCoinAnnouncementMod;

impl Mod for ForceAssertCoinAnnouncementMod {
    const MOD_REVEAL: &[u8] = &FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE;
    const MOD_HASH: TreeHash = FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE_HASH;
}

pub const FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE: [u8; 85] = hex!(
    "
    ff02ffff01ff02ff06ffff04ff02ffff04ff05ff80808080ffff04ffff01ff3d
    ff02ffff03ffff09ff11ff0480ffff0105ffff01ff04ff09ffff02ff06ffff04
    ff02ffff04ff0dff808080808080ff0180ff018080
    "
);

pub const FORCE_ASSERT_COIN_ANNOUNCEMENT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "ca0daca027e5ebd4a61fad7e32cfe1e984ad5b561c2fc08dea30accf3a191fab"
));
