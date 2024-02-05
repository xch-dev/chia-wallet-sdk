use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use clvm_traits::{
    clvm_list, destructure_list, match_list, ClvmDecoder, ClvmEncoder, FromClvm, FromClvmError,
    MatchByte, ToClvm, ToClvmError,
};

/// A condition that must be met in order to spend a coin.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
#[allow(missing_docs)]
#[repr(u64)]
pub enum Condition<T> {
    Remark = 1,

    AggSigParent {
        public_key: PublicKey,
        message: Bytes,
    } = 43,

    AggSigPuzzle {
        public_key: PublicKey,
        message: Bytes,
    } = 44,

    AggSigAmount {
        public_key: PublicKey,
        message: Bytes,
    } = 45,

    AggSigPuzzleAmount {
        public_key: PublicKey,
        message: Bytes,
    } = 46,

    AggSigParentAmount {
        public_key: PublicKey,
        message: Bytes,
    } = 47,

    AggSigParentPuzzle {
        public_key: PublicKey,
        message: Bytes,
    } = 48,

    AggSigUnsafe {
        public_key: PublicKey,
        message: Bytes,
    } = 49,

    AggSigMe {
        public_key: PublicKey,
        message: Bytes,
    } = 50,

    #[clvm(tuple)]
    CreateCoin(CreateCoin) = 51,

    ReserveFee {
        amount: u64,
    } = 52,

    CreateCoinAnnouncement {
        message: Bytes,
    } = 60,

    AssertCoinAnnouncement {
        announcement_id: Bytes,
    } = 61,

    CreatePuzzleAnnouncement {
        message: Bytes,
    } = 62,

    AssertPuzzleAnnouncement {
        announcement_id: Bytes,
    } = 63,

    AssertConcurrentSpend {
        coin_id: Bytes32,
    } = 64,

    AssertConcurrentPuzzle {
        puzzle_hash: Bytes32,
    } = 65,

    AssertMyCoinId {
        coin_id: Bytes32,
    } = 70,

    AssertMyParentId {
        parent_id: Bytes32,
    } = 71,

    AssertMyPuzzleHash {
        puzzle_hash: Bytes32,
    } = 72,

    AssertMyAmount {
        amount: u64,
    } = 73,

    AssertMyBirthSeconds {
        seconds: u64,
    } = 74,

    AssertMyBirthHeight {
        block_height: u32,
    } = 75,

    AssertEphemeral = 76,

    AssertSecondsRelative {
        seconds: u64,
    } = 80,

    AssertSecondsAbsolute {
        seconds: u64,
    } = 81,

    AssertHeightRelative {
        block_height: u32,
    } = 82,

    AssertHeightAbsolute {
        block_height: u32,
    } = 83,

    AssertBeforeSecondsRelative {
        seconds: u64,
    } = 84,

    AssertBeforeSecondsAbsolute {
        seconds: u64,
    } = 85,

    AssertBeforeHeightRelative {
        block_height: u32,
    } = 86,

    AssertBeforeHeightAbsolute {
        block_height: u32,
    } = 87,

    #[clvm(tuple)]
    Softfork {
        cost: u64,
        rest: T,
    } = 90,
}

/// A create coin condition.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(untagged, list)]
#[allow(missing_docs)]
pub enum CreateCoin {
    Normal {
        puzzle_hash: Bytes32,
        amount: u64,
    },
    Memos {
        puzzle_hash: Bytes32,
        amount: u64,
        memos: Vec<Bytes>,
    },
}

/// A condition that must be met in order to spend a CAT coin.
#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(untagged, tuple)]
#[allow(missing_docs)]
pub enum CatCondition<T> {
    Normal(Condition<T>),
    RunTail(RunTail<T>),
}

/// A run TAIL condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTail<T> {
    /// The TAIL program reveal.
    pub program: T,
    /// The solution to the TAIL program.
    pub solution: T,
}

impl<Node, T> ToClvm<Node> for RunTail<T>
where
    T: ToClvm<Node>,
{
    fn to_clvm(&self, encoder: &mut impl ClvmEncoder<Node = Node>) -> Result<Node, ToClvmError> {
        clvm_list!(51, (), -113, &self.program, &self.solution).to_clvm(encoder)
    }
}

impl<Node, T> FromClvm<Node> for RunTail<T>
where
    T: FromClvm<Node>,
{
    fn from_clvm(
        decoder: &impl ClvmDecoder<Node = Node>,
        node: Node,
    ) -> Result<Self, FromClvmError> {
        let destructure_list!(_, _, _, program, solution) =
            <match_list!(MatchByte::<51>, (), MatchByte::<142>, T, T)>::from_clvm(decoder, node)?;
        Ok(Self { program, solution })
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use clvm_traits::{FromNodePtr, ToNodePtr};
    use clvmr::{allocator::NodePtr, serde::node_to_bytes, Allocator};
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
            Condition::<NodePtr>::CreateCoin(CreateCoin::Memos {
                puzzle_hash: Bytes32::from([0; 32]),
                amount: 0,
                memos: vec![Bytes::from([1; 32].to_vec())],
            }),
            &hex!(
                "
                ff33ffa00000000000000000000000000000000000000000000000000000000000000000ff8
                0ffffa001010101010101010101010101010101010101010101010101010101010101018080
                "
            ),
        );

        check(
            Condition::<NodePtr>::CreateCoin(CreateCoin::Normal {
                puzzle_hash: Bytes32::from([0; 32]),
                amount: 0,
            }),
            &hex!(
                "
                ff33ffa00000000000000000000000000000000000000000000000000000000000000000ff8080
                "
            ),
        );
    }
}
