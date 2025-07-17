use chia_protocol::{Bytes32, Coin};
use chia_puzzle_types::{singleton::SingletonArgs, LineageProof, Proof};
use clvm_utils::TreeHash;

/// A generic singleton primitive, which can be extended with the [`SingletonInfo`] trait.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Singleton<I> {
    /// The coin that this [`Singleton`] represents. Its puzzle hash should match the singleton outer puzzle hash.
    pub coin: Coin,

    /// The proof is needed by the singleton puzzle to prove that this coin is a legitimate singleton.
    /// It's typically obtained by looking up and parsing the parent coin.
    ///
    /// Note that while the proof will be a [`LineageProof`] for most coins,
    /// for the first singleton in the lineage it will be an [`EveProof`](chia_puzzle_types::EveProof) instead.
    /// However, the eve coin is typically unhinted and spent in the same transaction as it was created,
    /// so this is not relevant for database storage or syncing unspent coins.
    pub proof: Proof,

    /// The information needed to construct the outer puzzle.
    pub info: I,
}

impl<I> Singleton<I>
where
    I: SingletonInfo,
{
    pub fn new(coin: Coin, proof: Proof, info: I) -> Self {
        Self { coin, proof, info }
    }

    /// Creates a [`LineageProof`] for which would be valid for any children created by this [`Singleton`].
    pub fn child_lineage_proof(&self) -> LineageProof {
        LineageProof {
            parent_parent_coin_info: self.coin.parent_coin_info,
            parent_inner_puzzle_hash: self.info.inner_puzzle_hash().into(),
            parent_amount: self.coin.amount,
        }
    }

    /// Creates a new [`Singleton`] that represents a child of this one.
    ///
    /// You can specify the new [`SingletonInfo`] to use for the child.
    ///
    /// It's important to use the right [`SingletonInfo`] instead of modifying it afterward,
    /// otherwise the puzzle hash of the child will not match the one expected by the coin.
    pub fn child_with<N>(&self, info: N, amount: u64) -> Singleton<N>
    where
        N: SingletonInfo,
    {
        Singleton::new(
            Coin::new(
                self.coin.coin_id(),
                SingletonArgs::curry_tree_hash(info.launcher_id(), info.inner_puzzle_hash()).into(),
                amount,
            ),
            Proof::Lineage(self.child_lineage_proof()),
            info,
        )
    }
}

pub trait SingletonInfo {
    fn launcher_id(&self) -> Bytes32;

    /// Calculates the inner puzzle hash of the singleton.
    ///
    /// This does not include the [`SingletonLayer`](crate::SingletonLayer).
    fn inner_puzzle_hash(&self) -> TreeHash;

    /// Calculates the full puzzle hash of the the outer [`SingletonLayer`](crate::SingletonLayer).
    fn puzzle_hash(&self) -> TreeHash {
        SingletonArgs::curry_tree_hash(self.launcher_id(), self.inner_puzzle_hash())
    }
}
