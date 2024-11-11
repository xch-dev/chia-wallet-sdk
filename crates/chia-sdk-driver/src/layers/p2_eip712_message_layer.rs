use chia_protocol::{Bytes, Bytes32};
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{CurriedProgram, TreeHash};
use clvmr::{Allocator, NodePtr};
use ethers::utils::keccak256;
use hex_literal::hex;

use crate::{DriverError, Layer, Puzzle, Spend, SpendContext};

pub const P2_EIP712_MESSAGE_PUZZLE: [u8; 276] = hex!("ff02ffff01ff02ffff03ffff22ffff20ffff8413d61f00ff17ff5fff81bf8080ffff20ffff24ffff01820ab9ffff0101ffff01ff02ffff03ffff09ffff3eff02ffff3eff05ff0bff178080ff2f80ff80ffff01ff088080ff0180ffff04ff05ffff04ff0bffff04ff2fffff04ffff02ff06ffff04ff02ffff04ff82017fff80808080ffff04ff5fff808080808080808080ffff01ff04ffff04ff04ffff04ff2fff808080ffff02ff82017fff8202ff8080ffff01ff088080ff0180ffff04ffff01ff46ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff06ffff04ff02ffff04ff09ff80808080ffff02ff06ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080");
pub const P2_EIP712_MESSAGE_PUZZLE_HASH: TreeHash = TreeHash::new(hex!(
    "
    0770e1551037c7f37138d3eb0166079f1efb096d58de846dc8844ca9f52f9ada
    "
));

type EthPubkeyBytes = [u8; 33];
type EthSignatureBytes = [u8; 64];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct P2Eip712MessageLayer {
    pub genesis_challenge: Bytes32,
    pub pubkey: EthPubkeyBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct P2Eip712MessageArgs {
    pub prefix_and_domain_separator: Bytes,
    pub type_hash: Bytes32,
    pub pubkey: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(solution)]
pub struct P2Eip712MessageSolution<P, S> {
    pub my_id: Bytes32,
    pub signed_hash: Bytes32,
    pub signature: Bytes,
    pub delegated_puzzle: P,
    pub delegated_solution: S,
}

impl P2Eip712MessageLayer {
    pub fn new(genesis_challenge: Bytes32, pubkey: EthPubkeyBytes) -> Self {
        Self {
            genesis_challenge,
            pubkey,
        }
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        my_id: Bytes32,
        signature: EthSignatureBytes,
        delegated_spend: Spend,
    ) -> Result<Spend, DriverError> {
        self.construct_spend(
            ctx,
            P2Eip712MessageSolution {
                my_id,
                signed_hash: self.hash_to_sign(my_id, ctx.tree_hash(delegated_spend.puzzle).into()),
                signature: Bytes::new(signature.to_vec()),
                delegated_puzzle: delegated_spend.puzzle,
                delegated_solution: delegated_spend.solution,
            },
        )
    }

    pub fn domain_separator(&self) -> Bytes32 {
        let type_hash = keccak256(b"EIP712Domain(string name,string version,bytes32 salt)");

        keccak256(ethers::abi::encode(&[
            ethers::abi::Token::FixedBytes(type_hash.to_vec()),
            ethers::abi::Token::FixedBytes(keccak256("Chia Coin Spend").to_vec()),
            ethers::abi::Token::FixedBytes(keccak256("1").to_vec()),
            ethers::abi::Token::FixedBytes(self.genesis_challenge.to_vec()),
        ]))
        .into()
    }

    pub fn prefix_and_domain_separator(&self) -> [u8; 34] {
        let mut pads = [0u8; 34];
        pads[0] = 0x19;
        pads[1] = 0x01;
        pads[2..].copy_from_slice(&self.domain_separator());
        pads
    }

    pub fn type_hash() -> Bytes32 {
        keccak256(b"ChiaCoinSpend(bytes32 coin_id,bytes32 delegated_puzzle_hash)").into()
    }

    pub fn hash_to_sign(&self, coin_id: Bytes32, delegated_puzzle_hash: Bytes32) -> Bytes32 {
        /*
        bytes32 messageHash = keccak256(abi.encode(
            typeHash,
            coin_id,
            delegated_puzzle_hash
        ));
        */
        let mut to_hash = Vec::new();
        to_hash.extend_from_slice(&P2Eip712MessageLayer::type_hash());
        to_hash.extend_from_slice(&coin_id);
        to_hash.extend_from_slice(&delegated_puzzle_hash);

        let message_hash = keccak256(&to_hash);

        let mut to_hash = Vec::new();
        to_hash.extend_from_slice(&self.prefix_and_domain_separator());
        to_hash.extend_from_slice(&message_hash);

        keccak256(&to_hash).into()
    }
}

impl Layer for P2Eip712MessageLayer {
    type Solution = P2Eip712MessageSolution<NodePtr, NodePtr>;

    fn construct_puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        let curried = CurriedProgram {
            program: ctx.p2_eip712_message_puzzle()?,
            args: P2Eip712MessageArgs {
                prefix_and_domain_separator: self.prefix_and_domain_separator().to_vec().into(),
                type_hash: P2Eip712MessageLayer::type_hash(),
                pubkey: self.pubkey.to_vec().into(),
            },
        };
        ctx.alloc(&curried)
    }

    fn construct_solution(
        &self,
        ctx: &mut SpendContext,
        solution: Self::Solution,
    ) -> Result<NodePtr, DriverError> {
        ctx.alloc(&solution)
    }

    fn parse_puzzle(_allocator: &Allocator, _puzzle: Puzzle) -> Result<Option<Self>, DriverError> {
        Ok(None)
    }

    fn parse_solution(
        allocator: &Allocator,
        solution: NodePtr,
    ) -> Result<Self::Solution, DriverError> {
        Ok(P2Eip712MessageSolution::from_clvm(allocator, solution)?)
    }
}
