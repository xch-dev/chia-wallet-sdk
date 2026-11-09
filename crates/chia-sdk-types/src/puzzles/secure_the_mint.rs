use std::borrow::Cow;

use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_puzzles::{
    NFT_OWNERSHIP_LAYER_HASH, NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH,
    NFT_STATE_LAYER_HASH, SINGLETON_LAUNCHER_HASH, SINGLETON_TOP_LAYER_V1_1_HASH,
};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// The pre-launcher puzzle from secure the mint (`secure_the_mint_launcher.clsp` in
/// <https://github.com/mintgarden-io/secure-the-mint>).
///
/// When minting, it creates a singleton launcher and asserts that the launcher creates an eve NFT with the curried
/// metadata, royalty and p2 puzzle hash. When melting, it requires a signature from the melt public key instead.
pub const SECURE_THE_MINT_LAUNCHER_PUZZLE: [u8; 842] = hex!(
    "
    ff02ffff01ff02ffff03ff822fffffff01ff04ffff04ff38ffff04ff825fffff
    808080ffff04ffff04ff34ffff04ff0bffff01ff01808080ffff04ffff04ff28
    ffff04ffff0bffff02ff36ffff04ff02ffff04ff825fffffff04ff0bffff01ff
    018080808080ffff02ff3effff04ff02ffff04ffff04ffff02ff2effff04ff02
    ffff04ff05ffff04ffff02ff2effff04ff02ffff04ff17ffff04ffff02ff2eff
    ff04ff02ffff04ff81bfffff04ff820bffffff04ffff02ff2effff04ff02ffff
    04ff82017fffff04ffff0bff3cff8205ff80ffff04ffff0bff3cff8202ff80ff
    ff04ffff02ff3effff04ff02ffff04ffff04ff05ffff04ffff02ff36ffff04ff
    02ffff04ff825fffffff04ff0bffff01ff018080808080ff0b8080ff80808080
    ff80808080808080ffff04ffff0bff3cff8080ffff04ffff0bff3cff81bf80ff
    8080808080808080ffff04ffff0bff3cff5f80ffff04ff2fffff04ffff0bff3c
    ff1780ff8080808080808080ffff04ffff02ff3effff04ff02ffff04ffff04ff
    05ffff04ffff02ff36ffff04ff02ffff04ff825fffffff04ff0bffff01ff0180
    80808080ff0b8080ff80808080ff808080808080ffff01ff01ff808080ff8080
    808080ff808080ff80808080ffff01ff04ffff04ff10ffff04ff8217ffffff01
    ff01808080ff808080ff0180ffff04ffff01ffffff32ff3d46ffff0233ff0401
    ffff01ff0220ffffff02ffff03ff05ffff01ff02ff26ffff04ff02ffff04ff0d
    ffff04ffff0bff2affff0bff3cff2c80ffff0bff2affff0bff2affff0bff3cff
    1280ff0980ffff0bff2aff0bffff0bff3cff8080808080ff8080808080ffff01
    0b80ff0180ff02ffff03ffff22ffff09ffff0dff0580ff3a80ffff09ffff0dff
    0b80ff3a80ffff15ff17ffff0181ff8080ffff01ff0bff05ff0bff1780ffff01
    ff088080ff0180ffff0bff2affff0bff3cff2480ffff0bff2affff0bff2affff
    0bff3cff1280ff0580ffff0bff2affff02ff26ffff04ff02ffff04ff07ffff04
    ffff0bff3cff3c80ff8080808080ffff0bff3cff8080808080ff02ffff03ffff
    07ff0580ffff01ff0bffff0102ffff02ff3effff04ff02ffff04ff09ff808080
    80ffff02ff3effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff
    058080ff0180ff018080
    "
);

pub const SECURE_THE_MINT_LAUNCHER_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    0fcdee23d7f33cf08460f1be9c72e0ca2ea42e62ac7fcc4e017f94f7c01adc7c
    "
));

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(curry)]
pub struct SecureTheMintLauncherArgs {
    pub singleton_mod_hash: Bytes32,
    pub singleton_launcher_puzzle_hash: Bytes32,
    pub nft_state_layer_mod_hash: Bytes32,
    pub metadata_hash: Bytes32,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub nft_ownership_layer_mod_hash: Bytes32,
    pub nft_ownership_transfer_program_mod_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub p2_puzzle_hash: Bytes32,
    pub melt_public_key: Option<PublicKey>,
}

impl SecureTheMintLauncherArgs {
    pub fn new(
        metadata_hash: Bytes32,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        p2_puzzle_hash: Bytes32,
        melt_public_key: Option<PublicKey>,
    ) -> Self {
        Self {
            singleton_mod_hash: SINGLETON_TOP_LAYER_V1_1_HASH.into(),
            singleton_launcher_puzzle_hash: SINGLETON_LAUNCHER_HASH.into(),
            nft_state_layer_mod_hash: NFT_STATE_LAYER_HASH.into(),
            metadata_hash,
            metadata_updater_puzzle_hash,
            nft_ownership_layer_mod_hash: NFT_OWNERSHIP_LAYER_HASH.into(),
            nft_ownership_transfer_program_mod_hash:
                NFT_OWNERSHIP_TRANSFER_PROGRAM_ONE_WAY_CLAIM_WITH_ROYALTIES_HASH.into(),
            royalty_puzzle_hash,
            royalty_basis_points,
            p2_puzzle_hash,
            melt_public_key,
        }
    }
}

impl Mod for SecureTheMintLauncherArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&SECURE_THE_MINT_LAUNCHER_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        SECURE_THE_MINT_LAUNCHER_PUZZLE_HASH
    }
}

#[derive(ToClvm, FromClvm, Debug, Clone, Copy, PartialEq, Eq)]
#[clvm(list)]
pub struct SecureTheMintLauncherSolution {
    /// True to mint the NFT, or false to melt the pre-launcher coin.
    pub mint: bool,
    pub my_id: Bytes32,
}

impl SecureTheMintLauncherSolution {
    pub fn new(mint: bool, my_id: Bytes32) -> Self {
        Self { mint, my_id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_secure_the_mint_launcher_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(SECURE_THE_MINT_LAUNCHER_PUZZLE => SECURE_THE_MINT_LAUNCHER_PUZZLE_HASH);
        Ok(())
    }
}
