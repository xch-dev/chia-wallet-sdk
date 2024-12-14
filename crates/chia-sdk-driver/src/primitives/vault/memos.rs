#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct RestrictionMemo<T> {
    pub is_morpher: bool,
    pub curried_puzzle_hash: Bytes32,
    pub restriction: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct TimelockMemo {
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MemberMemo<T> {
    pub curried_puzzle_hash: Bytes32,
    pub member: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BlsMemberMemo {
    pub public_key: PublicKey,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MofNMemo<T> {
    pub required: usize,
    pub members: Vec<T>,
}

pub fn from_memo(allocator: &Allocator, memo: NodePtr) -> Result<Self, DriverError> {
    let memo = MofNMemo::from_clvm(allocator, memo)?;

    if memo.members.len() < memo.required {
        return Err(DriverError::InvalidMemo);
    }

    let mut members = Vec::with_capacity(memo.members.len());

    for member_memo in memo.members {
        members.push(Member::from_memo(allocator, member_memo)?);
    }

    Ok(Self {
        required: memo.required,
        members,
    })
}
