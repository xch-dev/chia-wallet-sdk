use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use clvm_traits::{
    clvm_list, destructure_list, match_list, ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError,
    MatchByte, ToClvm, ToClvmError,
};
use clvmr::NodePtr;

macro_rules! condition {
    ( $name:ident, $code:expr ) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name;
        condition!(impl $name, $code);
    };

    ( $name:ident, $code:expr, { $( $field:ident: $ty:ty ),* $(,)? } ) => {
        #[allow(missing_docs)]
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            $( pub $field: $ty ),*
        }
        condition!(impl $name, $code, { $( $field: $ty ),* });
    };

    ( impl $name:ident, $code:expr ) => {
        impl<Node> ToClvm<Node> for $name {
            fn to_clvm(&self, encoder: &mut impl ClvmEncoder<Node = Node>) -> Result<Node, ToClvmError> {
                clvm_list!($code).to_clvm(encoder)
            }
        }

        impl<Node> FromClvm<Node> for $name {
            fn from_clvm(
                decoder: &impl ClvmDecoder<Node = Node>,
                node: Node,
            ) -> Result<Self, FromClvmError> {
                let destructure_list!(code) = <match_list!(i32)>::from_clvm(decoder, node)?;
                if code != $code {
                    return Err(FromClvmError::Custom(format!("invalid code: {}", code)));
                }
                Ok(Self)
            }
        }
    };

    ( impl $name:ident, $code:expr, { $( $field:ident: $ty:ty ),* } ) => {
        impl<Node> ToClvm<Node> for $name
        where
            $( $ty: ToClvm<Node> ),*
        {
            fn to_clvm(&self, encoder: &mut impl ClvmEncoder<Node = Node>) -> Result<Node, ToClvmError> {
                clvm_list!($code, $( &self.$field ),*).to_clvm(encoder)
            }
        }

        impl<Node> FromClvm<Node> for $name
        where
            $( $ty: FromClvm<Node> ),*
        {
            fn from_clvm(
                decoder: &impl ClvmDecoder<Node = Node>,
                node: Node,
            ) -> Result<Self, FromClvmError> {
                let destructure_list!(code, $( $field ),*) =
                    <match_list!(i32, $( $ty, )* )>::from_clvm(decoder, node)?;
                if code != $code {
                    return Err(FromClvmError::Custom(format!("invalid code: {}", code)));
                }
                Ok(Self { $( $field ),* })
            }
        }
    };
}

#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct AggSig {
    pub kind: AggSigKind,
    pub public_key: PublicKey,
    pub message: Bytes,
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm, Hash)]
#[repr(u8)]
#[clvm(atom)]
pub enum AggSigKind {
    Parent = 43,
    Puzzle = 44,
    Amount = 45,
    PuzzleAmount = 46,
    ParentAmount = 47,
    ParentPuzzle = 48,
    Unsafe = 49,
    Me = 50,
}

#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(transparent)]
pub enum CreateCoin {
    WithoutMemos(CreateCoinWithoutMemos),
    WithMemos(CreateCoinWithMemos),
}

impl CreateCoin {
    pub fn puzzle_hash(&self) -> Bytes32 {
        match self {
            Self::WithoutMemos(inner) => inner.puzzle_hash,
            Self::WithMemos(inner) => inner.puzzle_hash,
        }
    }

    pub fn amount(&self) -> u64 {
        match self {
            Self::WithoutMemos(inner) => inner.amount,
            Self::WithMemos(inner) => inner.amount,
        }
    }
}

condition!(Remark, 1, {});
condition!(AggSigParent, 43, { public_key: PublicKey, message: Bytes });
condition!(AggSigPuzzle, 44, { public_key: PublicKey, message: Bytes });
condition!(AggSigAmount, 45, { public_key: PublicKey, message: Bytes });
condition!(AggSigPuzzleAmount, 46, { public_key: PublicKey, message: Bytes });
condition!(AggSigParentAmount, 47, { public_key: PublicKey, message: Bytes });
condition!(AggSigParentPuzzle, 48, { public_key: PublicKey, message: Bytes });
condition!(AggSigUnsafe, 49, { public_key: PublicKey, message: Bytes });
condition!(AggSigMe, 50, { public_key: PublicKey, message: Bytes });
condition!(CreateCoinWithoutMemos, 51, { puzzle_hash: Bytes32, amount: u64 });
condition!(CreateCoinWithMemos, 51, { puzzle_hash: Bytes32, amount: u64, memos: Vec<Bytes> });
condition!(ReserveFee, 52, { amount: u64 });
condition!(CreateCoinAnnouncement, 60, { message: Bytes });
condition!(AssertCoinAnnouncement, 61, { announcement_id: Bytes32 });
condition!(CreatePuzzleAnnouncement, 62, { message: Bytes });
condition!(AssertPuzzleAnnouncement, 63, { announcement_id: Bytes32 });
condition!(AssertConcurrentSpend, 64, { coin_id: Bytes32 });
condition!(AssertConcurrentPuzzle, 65, { puzzle_hash: Bytes32 });
condition!(AssertMyCoinId, 70, { coin_id: Bytes32 });
condition!(AssertMyParentId, 71, { parent_id: Bytes32 });
condition!(AssertMyPuzzleHash, 72, { puzzle_hash: Bytes32 });
condition!(AssertMyAmount, 73, { amount: u64 });
condition!(AssertMyBirthSeconds, 74, { seconds: u64 });
condition!(AssertMyBirthHeight, 75, { block_height: u32 });
condition!(AssertEphemeral, 76, {});
condition!(AssertSecondsRelative, 80, { seconds: u64 });
condition!(AssertSecondsAbsolute, 81, { seconds: u64 });
condition!(AssertHeightRelative, 82, { block_height: u32 });
condition!(AssertHeightAbsolute, 83, { block_height: u32 });
condition!(AssertBeforeSecondsRelative, 84, { seconds: u64 });
condition!(AssertBeforeSecondsAbsolute, 85, { seconds: u64 });
condition!(AssertBeforeHeightRelative, 86, { block_height: u32 });
condition!(AssertBeforeHeightAbsolute, 87, { block_height: u32 });

condition!(NewNftOwner, -10, {
    new_owner: Option<Bytes32>,
    trade_prices_list: Vec<NftTradePrice>,
    new_did_inner_hash: Option<Bytes32>
});

#[allow(clippy::derivable_impls)]
impl Default for NewNftOwner {
    fn default() -> Self {
        Self {
            new_owner: None,
            trade_prices_list: Vec::new(),
            new_did_inner_hash: None,
        }
    }
}

#[allow(missing_docs)]
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct NftTradePrice {
    pub trade_price: u16,
    pub puzzle_hash: Bytes32,
}

/// A run TAIL condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTail {
    /// The TAIL program reveal.
    pub program: NodePtr,

    /// The solution to the TAIL program.
    pub solution: NodePtr,
}

impl ToClvm<NodePtr> for RunTail {
    fn to_clvm(
        &self,
        encoder: &mut impl ClvmEncoder<Node = NodePtr>,
    ) -> Result<NodePtr, ToClvmError> {
        clvm_list!(51, (), -113, &self.program, &self.solution).to_clvm(encoder)
    }
}

impl FromClvm<NodePtr> for RunTail {
    fn from_clvm(
        decoder: &impl ClvmDecoder<Node = NodePtr>,
        node: NodePtr,
    ) -> Result<Self, FromClvmError> {
        let destructure_list!(_, _, _, program, solution) =
            <match_list!(MatchByte::<51>, (), MatchByte::<142>, NodePtr, NodePtr)>::from_clvm(
                decoder, node,
            )?;
        Ok(Self { program, solution })
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use clvm_traits::{FromNodePtr, ToNodePtr};
    use clvmr::{serde::node_to_bytes, Allocator};
    use hex_literal::hex;

    use super::*;

    fn check<T>(value: T, expected: &[u8])
    where
        T: ToNodePtr + FromNodePtr + PartialEq + Debug,
    {
        let a = &mut Allocator::new();
        let serialized = value.to_node_ptr(a).unwrap();
        let deserialized = T::from_node_ptr(a, serialized).unwrap();
        assert_eq!(value, deserialized);

        let bytes = node_to_bytes(a, serialized).unwrap();
        assert_eq!(hex::encode(bytes), hex::encode(expected));
    }

    #[test]
    fn test() {
        check(
            CreateCoinWithMemos {
                puzzle_hash: Bytes32::from([0; 32]),
                amount: 0,
                memos: vec![Bytes::from([1; 32].to_vec())],
            },
            &hex!(
                "
                ff33ffa00000000000000000000000000000000000000000000000000000000000000000ff8
                0ffffa001010101010101010101010101010101010101010101010101010101010101018080
                "
            ),
        );

        check(
            CreateCoinWithoutMemos {
                puzzle_hash: Bytes32::from([0; 32]),
                amount: 0,
            },
            &hex!(
                "
                ff33ffa00000000000000000000000000000000000000000000000000000000000000000ff8080
                "
            ),
        );
    }
}
