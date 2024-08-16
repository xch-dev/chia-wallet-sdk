use chia_protocol::Bytes32;
use chia_puzzles::singleton::SingletonStruct;

#[derive(Debug, Clone, Copy)]
pub struct NftInfo<M> {
    pub singleton_struct: SingletonStruct,
    pub metadata: M,
    pub current_owner: Option<Bytes32>,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_ten_thousandths: u16,
    pub p2_puzzle_hash: Bytes32,
}
