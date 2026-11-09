use std::borrow::Cow;

use chia_bls::PublicKey;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, ToTreeHash, TreeHash};
use hex_literal::hex;

use crate::Mod;

/// Serialized `p2_m_of_n_delegate_direct` puzzle: an M-of-N multisig.
///
/// The spender picks exactly `m` of the curried public keys with selectors, and the
/// puzzle outputs an `AGG_SIG_UNSAFE` condition for each selected key on the tree hash of
/// the delegated puzzle, followed by the delegated puzzle's conditions. Since the
/// signatures are not bound to the coin, the delegated puzzle should include conditions
/// that tie it to a specific spend. Medieval vaults use it as their inner puzzle.
pub const P2_M_OF_N_DELEGATE_DIRECT_PUZZLE: [u8; 453] = hex!(
    "
    ff02ffff01ff02ffff03ffff09ff05ffff02ff16ffff04ff02ffff04ff17ff80
    80808080ffff01ff02ff0cffff04ff02ffff04ffff02ff0affff04ff02ffff04
    ff17ffff04ff0bff8080808080ffff04ffff02ff1effff04ff02ffff04ff2fff
    80808080ffff04ff2fffff04ff5fff80808080808080ffff01ff088080ff0180
    ffff04ffff01ffff31ff02ffff03ff05ffff01ff04ffff04ff08ffff04ff09ff
    ff04ff0bff80808080ffff02ff0cffff04ff02ffff04ff0dffff04ff0bffff04
    ff17ffff04ff2fff8080808080808080ffff01ff02ff17ff2f8080ff0180ffff
    02ffff03ff05ffff01ff02ffff03ff09ffff01ff04ff13ffff02ff0affff04ff
    02ffff04ff0dffff04ff1bff808080808080ffff01ff02ff0affff04ff02ffff
    04ff0dffff04ff1bff808080808080ff0180ff8080ff0180ffff02ffff03ff05
    ffff01ff10ffff02ff16ffff04ff02ffff04ff0dff80808080ffff02ffff03ff
    09ffff01ff0101ff8080ff018080ff8080ff0180ff02ffff03ffff07ff0580ff
    ff01ff0bffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ffff02ff
    1effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff01
    80ff018080
    "
);

/// Tree hash of [`P2_M_OF_N_DELEGATE_DIRECT_PUZZLE`].
pub const P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    0f199d5263ac1a62b077c159404a71abd3f9691cc57520bf1d4c5cb501504457
    "
));

/// Curried arguments of [`P2_M_OF_N_DELEGATE_DIRECT_PUZZLE`].
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct P2MOfNDelegateDirectArgs {
    /// Number of signatures required.
    pub m: usize,
    /// The N public keys that may sign.
    pub public_key_list: Vec<PublicKey>,
}

impl P2MOfNDelegateDirectArgs {
    /// Creates the arguments for an `m`-of-`public_key_list.len()` multisig.
    pub fn new(m: usize, public_key_list: Vec<PublicKey>) -> Self {
        Self { m, public_key_list }
    }

    /// Computes the curried puzzle hash.
    pub fn curry_tree_hash(m: usize, public_key_list: Vec<PublicKey>) -> TreeHash {
        CurriedProgram {
            program: P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH,
            args: Self::new(m, public_key_list),
        }
        .tree_hash()
    }

    /// Builds the [`selectors`](P2MOfNDelegateDirectSolution::selectors) for a solution:
    /// `true` for each key in `public_key_list` that is also in `used_pubkeys`.
    ///
    /// The puzzle requires exactly `m` selectors to be `true`, so `used_pubkeys` should
    /// contain exactly `m` of the keys.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chia_bls::SecretKey;
    /// use chia_sdk_types::puzzles::P2MOfNDelegateDirectArgs;
    ///
    /// let keys: Vec<_> = (0..3_u8)
    ///     .map(|i| SecretKey::from_seed(&[i; 32]).public_key())
    ///     .collect();
    ///
    /// let selectors =
    ///     P2MOfNDelegateDirectArgs::selectors_for_used_pubkeys(&keys, &[keys[0], keys[2]]);
    /// assert_eq!(selectors, [true, false, true]);
    /// ```
    pub fn selectors_for_used_pubkeys(
        public_key_list: &[PublicKey],
        used_pubkeys: &[PublicKey],
    ) -> Vec<bool> {
        public_key_list
            .iter()
            .map(|pubkey| used_pubkeys.contains(pubkey))
            .collect()
    }
}

/// Solution to [`P2_M_OF_N_DELEGATE_DIRECT_PUZZLE`].
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(list)]
pub struct P2MOfNDelegateDirectSolution<P, S> {
    /// One flag per curried public key, `true` for the keys that sign. Exactly `m` must be
    /// `true`.
    pub selectors: Vec<bool>,
    /// Puzzle whose tree hash the selected keys sign and whose output is returned.
    pub delegated_puzzle: P,
    /// Solution passed to the delegated puzzle.
    pub delegated_solution: S,
}

impl Mod for P2MOfNDelegateDirectArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Borrowed(&P2_M_OF_N_DELEGATE_DIRECT_PUZZLE)
    }

    fn mod_hash() -> TreeHash {
        P2_M_OF_N_DELEGATE_DIRECT_PUZZLE_HASH
    }
}
