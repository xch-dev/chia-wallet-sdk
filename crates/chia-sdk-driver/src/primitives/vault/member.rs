use chia_bls::PublicKey;
use chia_protocol::Bytes32;
use chia_sdk_types::{
    BlsMember, BlsTaprootMember, FixedPuzzleMember, Mod, PasskeyMember, PasskeyMemberPuzzleAssert,
    Secp256k1Member, Secp256k1MemberPuzzleAssert, Secp256r1Member, Secp256r1MemberPuzzleAssert,
    SingletonMember,
};
use chia_secp::{K1PublicKey, R1PublicKey};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

use super::{KnownPuzzles, MofN, PuzzleWithRestrictions, VaultLayer};

#[derive(Debug, Clone)]
pub struct Member {
    puzzle_hash: TreeHash,
    kind: MemberKind,
}

#[derive(Debug, Clone)]
pub enum MemberKind {
    Bls(BlsMember),
    BlsTaproot(BlsTaprootMember),
    FixedPuzzle(FixedPuzzleMember),
    Passkey(PasskeyMember),
    PasskeyPuzzleAssert(PasskeyMemberPuzzleAssert),
    Secp256k1(Secp256k1Member),
    Secp256k1PuzzleAssert(Secp256k1MemberPuzzleAssert),
    Secp256r1(Secp256r1Member),
    Secp256r1PuzzleAssert(Secp256r1MemberPuzzleAssert),
    Singleton(SingletonMember),
    MofN(MofN),
    Unknown,
}

impl Member {
    pub fn bls(public_key: PublicKey) -> Self {
        let member = BlsMember::new(public_key);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::Bls(member),
        }
    }

    pub fn bls_taproot(synthetic_key: PublicKey) -> Self {
        let member = BlsTaprootMember::new(synthetic_key);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::BlsTaproot(member),
        }
    }

    pub fn fixed_puzzle(fixed_puzzle_hash: Bytes32) -> Self {
        let member = FixedPuzzleMember::new(fixed_puzzle_hash);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::FixedPuzzle(member),
        }
    }

    pub fn passkey(genesis_challenge: Bytes32, public_key: R1PublicKey) -> Self {
        let member = PasskeyMember::new(genesis_challenge, public_key);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::Passkey(member),
        }
    }

    pub fn passkey_puzzle_assert(genesis_challenge: Bytes32, public_key: R1PublicKey) -> Self {
        let member = PasskeyMemberPuzzleAssert::new(genesis_challenge, public_key);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::PasskeyPuzzleAssert(member),
        }
    }

    pub fn secp256k1(public_key: K1PublicKey) -> Self {
        let member = Secp256k1Member::new(public_key);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::Secp256k1(member),
        }
    }

    pub fn secp256k1_puzzle_assert(public_key: K1PublicKey) -> Self {
        let member = Secp256k1MemberPuzzleAssert::new(public_key);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::Secp256k1PuzzleAssert(member),
        }
    }

    pub fn secp256r1(public_key: R1PublicKey) -> Self {
        let member = Secp256r1Member::new(public_key);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::Secp256r1(member),
        }
    }

    pub fn secp256r1_puzzle_assert(public_key: R1PublicKey) -> Self {
        let member = Secp256r1MemberPuzzleAssert::new(public_key);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::Secp256r1PuzzleAssert(member),
        }
    }

    pub fn singleton(launcher_id: Bytes32) -> Self {
        let member = SingletonMember::new(launcher_id);
        Self {
            puzzle_hash: member.curry_tree_hash(),
            kind: MemberKind::Singleton(member),
        }
    }

    pub fn m_of_n(required: usize, members: Vec<PuzzleWithRestrictions<Member>>) -> Self {
        let m_of_n = MofN::new(required, members).expect("invalid m_of_n");
        Self {
            puzzle_hash: m_of_n.puzzle_hash(),
            kind: MemberKind::MofN(m_of_n),
        }
    }

    pub fn unknown(puzzle_hash: TreeHash) -> Self {
        Self {
            puzzle_hash,
            kind: MemberKind::Unknown,
        }
    }

    pub fn kind(&self) -> &MemberKind {
        &self.kind
    }
}

impl VaultLayer for Member {
    fn puzzle_hash(&self) -> TreeHash {
        self.puzzle_hash
    }

    fn puzzle(&self, ctx: &mut SpendContext) -> Result<NodePtr, DriverError> {
        match &self.kind {
            MemberKind::Bls(bls) => ctx.curry(bls),
            MemberKind::BlsTaproot(bls_taproot) => ctx.curry(bls_taproot),
            MemberKind::FixedPuzzle(fixed_puzzle) => ctx.curry(fixed_puzzle),
            MemberKind::Passkey(passkey) => ctx.curry(passkey),
            MemberKind::PasskeyPuzzleAssert(passkey_puzzle_assert) => {
                ctx.curry(passkey_puzzle_assert)
            }
            MemberKind::Secp256k1(secp256k1) => ctx.curry(secp256k1),
            MemberKind::Secp256k1PuzzleAssert(secp256k1_puzzle_assert) => {
                ctx.curry(secp256k1_puzzle_assert)
            }
            MemberKind::Secp256r1(secp256r1) => ctx.curry(secp256r1),
            MemberKind::Secp256r1PuzzleAssert(secp256r1_puzzle_assert) => {
                ctx.curry(secp256r1_puzzle_assert)
            }
            MemberKind::Singleton(singleton) => ctx.curry(singleton),
            MemberKind::MofN(m_of_n) => m_of_n.puzzle(ctx),
            MemberKind::Unknown => Err(DriverError::UnknownPuzzle),
        }
    }

    fn replace(self, known_puzzles: &KnownPuzzles) -> Self {
        let kind = known_puzzles
            .members
            .get(&self.puzzle_hash)
            .cloned()
            .unwrap_or(self.kind);

        let kind = match kind {
            MemberKind::Bls(..)
            | MemberKind::BlsTaproot(..)
            | MemberKind::FixedPuzzle(..)
            | MemberKind::Passkey(..)
            | MemberKind::PasskeyPuzzleAssert(..)
            | MemberKind::Secp256k1(..)
            | MemberKind::Secp256k1PuzzleAssert(..)
            | MemberKind::Secp256r1(..)
            | MemberKind::Secp256r1PuzzleAssert(..)
            | MemberKind::Singleton(..)
            | MemberKind::Unknown => kind,
            MemberKind::MofN(m_of_n) => MemberKind::MofN(m_of_n.replace(known_puzzles)),
        };

        Self {
            puzzle_hash: self.puzzle_hash,
            kind,
        }
    }
}
