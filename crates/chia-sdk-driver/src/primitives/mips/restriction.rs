use clvm_utils::TreeHash;

#[derive(Debug, Clone, Copy)]
pub struct Restriction {
    pub is_member_condition_validator: bool,
    pub puzzle_hash: TreeHash,
}
