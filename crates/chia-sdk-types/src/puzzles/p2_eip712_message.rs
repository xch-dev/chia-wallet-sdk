use std::borrow::Cow;

use chia_protocol::{Bytes32, BytesImpl};
use chia_secp::{K1PublicKey, K1Signature};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use hex_literal::hex;

use crate::Mod;

/// The 2-byte EIP-712 envelope prefix (`\x19\x01`) followed by the 32-byte
/// EIP-712 domain separator computed for the target Chia network.
///
/// Currying these two values together (instead of as separate atoms) saves a
/// `concat` inside the softfork guard and matches the on-chain layout used by
/// the puzzle when reconstructing the EIP-712 digest.
pub type Eip712PrefixAndDomainSeparator = BytesImpl<34>;

/// Curried arguments for the EIP-712 message puzzle (CHIP-0037).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2Eip712MessageArgs {
    pub prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
    pub type_hash: Bytes32,
    pub public_key: K1PublicKey,
}

impl P2Eip712MessageArgs {
    pub fn new(
        prefix_and_domain_separator: Eip712PrefixAndDomainSeparator,
        type_hash: Bytes32,
        public_key: K1PublicKey,
    ) -> Self {
        Self {
            prefix_and_domain_separator,
            type_hash,
            public_key,
        }
    }
}

impl Mod for P2Eip712MessageArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_EIP712_MESSAGE_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_EIP712_MESSAGE_PUZZLE_HASH
    }
}

/// Solution for the EIP-712 message puzzle.
///
/// `signed_hash` is the EIP-712 digest the off-chain wallet committed to; the
/// puzzle independently reconstructs the digest from `my_id` and the tree hash
/// of `delegated_puzzle` to ensure the signature is bound to this exact spend.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct P2Eip712MessageSolution<P, S> {
    pub my_id: Bytes32,
    pub signed_hash: Bytes32,
    pub signature: K1Signature,
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}

impl<P, S> P2Eip712MessageSolution<P, S> {
    pub fn new(
        my_id: Bytes32,
        signed_hash: Bytes32,
        signature: K1Signature,
        delegated_puzzle: P,
        delegated_solution: S,
    ) -> Self {
        Self {
            my_id,
            signed_hash,
            signature,
            delegated_puzzle,
            delegated_solution,
        }
    }
}

/// CLVM bytecode for the CHIP-0037 `p2_eip712_message` puzzle.
///
/// Source: <https://github.com/Chia-Network/chips/blob/main/assets/chip-0037/clsp/p2_eip712_message.clsp>
pub const P2_EIP712_MESSAGE_PUZZLE: [u8; 276] = hex!(
    "
    ff02ffff01ff02ffff03ffff22ffff20ffff8413d61f00ff17ff5fff81bf8080
    ffff20ffff24ffff01820ab9ffff0101ffff01ff02ffff03ffff09ffff3eff02
    ffff3eff05ff0bff178080ff2f80ff80ffff01ff088080ff0180ffff04ff05ff
    ff04ff0bffff04ff2fffff04ffff02ff06ffff04ff02ffff04ff82017fff8080
    8080ffff04ff5fff808080808080808080ffff01ff04ffff04ff04ffff04ff2f
    ff808080ffff02ff82017fff8202ff8080ffff01ff088080ff0180ffff04ffff
    01ff46ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff
    02ffff04ff09ff80808080ffff02ff06ffff04ff02ffff04ff0dff8080808080
    ffff01ff0bffff0101ff058080ff0180ff018080
    "
);

pub const P2_EIP712_MESSAGE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "aacce7b99db5b1e9eb16d676fa5f1a2e469ef589f29c4ab0010bac338a4df085"
));

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_puzzle_hash;

    #[test]
    fn test_puzzle_hash() -> anyhow::Result<()> {
        assert_puzzle_hash!(P2_EIP712_MESSAGE_PUZZLE => P2_EIP712_MESSAGE_PUZZLE_HASH);

        Ok(())
    }
}
