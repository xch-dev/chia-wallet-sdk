use chia_bls::PublicKey;
use chia_sdk_types::{BlsMember, Mod};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{DriverError, SpendContext};

use super::{KnownPuzzles, MofN, VaultLayer};

#[derive(Debug, Clone)]
pub struct Member {
    puzzle_hash: TreeHash,
    kind: MemberKind,
}

#[derive(Debug, Clone)]
pub enum MemberKind {
    Bls(BlsMember),
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

    pub fn m_of_n(m_of_n: MofN) -> Self {
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
            MemberKind::Bls(..) | MemberKind::Unknown => kind,
            MemberKind::MofN(m_of_n) => MemberKind::MofN(m_of_n.replace(known_puzzles)),
        };

        Self {
            puzzle_hash: self.puzzle_hash,
            kind,
        }
    }
}
