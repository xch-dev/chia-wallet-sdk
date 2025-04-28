use chia_consensus::consensus_constants::ConsensusConstants;
use chia_protocol::Bytes32;
use chia_sha2::Sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AggSigConstants {
    me: Bytes32,
    parent: Bytes32,
    puzzle: Bytes32,
    amount: Bytes32,
    parent_amount: Bytes32,
    puzzle_amount: Bytes32,
    parent_puzzle: Bytes32,
}

impl AggSigConstants {
    pub fn new(agg_sig_me: Bytes32) -> Self {
        Self {
            me: agg_sig_me,
            parent: hash(agg_sig_me, 43),
            puzzle: hash(agg_sig_me, 44),
            amount: hash(agg_sig_me, 45),
            puzzle_amount: hash(agg_sig_me, 46),
            parent_amount: hash(agg_sig_me, 47),
            parent_puzzle: hash(agg_sig_me, 48),
        }
    }

    pub fn me(&self) -> Bytes32 {
        self.me
    }

    pub fn parent(&self) -> Bytes32 {
        self.parent
    }

    pub fn puzzle(&self) -> Bytes32 {
        self.puzzle
    }

    pub fn amount(&self) -> Bytes32 {
        self.amount
    }

    pub fn parent_amount(&self) -> Bytes32 {
        self.parent_amount
    }

    pub fn puzzle_amount(&self) -> Bytes32 {
        self.puzzle_amount
    }

    pub fn parent_puzzle(&self) -> Bytes32 {
        self.parent_puzzle
    }
}

impl From<&ConsensusConstants> for AggSigConstants {
    fn from(constants: &ConsensusConstants) -> Self {
        Self {
            me: constants.agg_sig_me_additional_data,
            parent: constants.agg_sig_parent_additional_data,
            puzzle: constants.agg_sig_puzzle_additional_data,
            amount: constants.agg_sig_amount_additional_data,
            puzzle_amount: constants.agg_sig_puzzle_amount_additional_data,
            parent_amount: constants.agg_sig_parent_amount_additional_data,
            parent_puzzle: constants.agg_sig_parent_puzzle_additional_data,
        }
    }
}

impl From<ConsensusConstants> for AggSigConstants {
    fn from(constants: ConsensusConstants) -> Self {
        Self::from(&constants)
    }
}

fn hash(agg_sig_data: Bytes32, byte: u8) -> Bytes32 {
    let mut hasher = Sha256::new();
    hasher.update(agg_sig_data);
    hasher.update([byte]);
    hasher.finalize().into()
}
