use bindy::Result;
use chia_protocol::Bytes32;
use chia_sdk_types::{MAINNET_CONSTANTS, TESTNET11_CONSTANTS, default_constants};

use crate::AggSigConstants;

/// The consensus constants of a Chia network, for validating spend bundles and calculating their
/// cost.
///
/// Use [`ConsensusConstants::mainnet`] or [`ConsensusConstants::testnet11`] for the public
/// networks. Any other network (such as a new testnet or a simulator) can be described with
/// [`ConsensusConstants::new`], which uses mainnet's consensus rules with the network's genesis
/// challenge, and the `with_*` methods for the fork heights that differ from mainnet.
#[derive(Clone)]
pub struct ConsensusConstants(pub(crate) chia_consensus::consensus_constants::ConsensusConstants);

impl ConsensusConstants {
    /// Creates mainnet's consensus constants with a different genesis challenge and `AGG_SIG_ME`
    /// additional data. The additional data for the other `AGG_SIG_*` conditions is derived from
    /// `agg_sig_me`. On mainnet and testnet11, `agg_sig_me` is the genesis challenge.
    pub fn new(genesis_challenge: Bytes32, agg_sig_me: Bytes32) -> Result<Self> {
        Ok(Self(default_constants(genesis_challenge, agg_sig_me)))
    }

    /// The consensus constants of mainnet.
    pub fn mainnet() -> Result<Self> {
        Ok(Self(MAINNET_CONSTANTS.clone()))
    }

    /// The consensus constants of testnet11, which the simulator uses by default.
    pub fn testnet11() -> Result<Self> {
        Ok(Self(TESTNET11_CONSTANTS.clone()))
    }

    /// The genesis challenge, which identifies the network.
    pub fn genesis_challenge(&self) -> Result<Bytes32> {
        Ok(self.0.genesis_challenge)
    }

    /// The `AGG_SIG_*` additional data that BLS signatures on this network are made with.
    pub fn agg_sig_constants(&self) -> Result<AggSigConstants> {
        Ok(AggSigConstants((&self.0).into()))
    }

    /// The maximum CLVM cost of a block. The mempool allows half of it in a single transaction.
    pub fn max_block_cost_clvm(&self) -> Result<u64> {
        Ok(self.0.max_block_cost_clvm)
    }

    /// The CLVM cost per byte of a spend bundle's serialized puzzles and solutions.
    pub fn cost_per_byte(&self) -> Result<u64> {
        Ok(self.0.cost_per_byte)
    }

    /// The height of the 2.0 hard fork.
    pub fn hard_fork_height(&self) -> Result<u32> {
        Ok(self.0.hard_fork_height)
    }

    /// The height of the 3.0 hard fork, which changes the cost model.
    pub fn hard_fork2_height(&self) -> Result<u32> {
        Ok(self.0.hard_fork2_height)
    }

    /// The height at which soft fork 8 activates.
    pub fn soft_fork8_height(&self) -> Result<u32> {
        Ok(self.0.soft_fork8_height)
    }

    /// The height at which soft fork 9 activates.
    pub fn soft_fork9_height(&self) -> Result<u32> {
        Ok(self.0.soft_fork9_height)
    }

    /// Whether this is a test network rather than mainnet.
    pub fn testnet(&self) -> Result<bool> {
        Ok(self.0.testnet)
    }

    /// Returns a copy of the constants with a different 2.0 hard fork height.
    pub fn with_hard_fork_height(&self, height: u32) -> Result<Self> {
        let mut constants = self.0.clone();
        constants.hard_fork_height = height;
        Ok(Self(constants))
    }

    /// Returns a copy of the constants with a different 3.0 hard fork height.
    pub fn with_hard_fork2_height(&self, height: u32) -> Result<Self> {
        let mut constants = self.0.clone();
        constants.hard_fork2_height = height;
        Ok(Self(constants))
    }

    /// Returns a copy of the constants with a different soft fork 8 height.
    pub fn with_soft_fork8_height(&self, height: u32) -> Result<Self> {
        let mut constants = self.0.clone();
        constants.soft_fork8_height = height;
        Ok(Self(constants))
    }

    /// Returns a copy of the constants with a different soft fork 9 height.
    pub fn with_soft_fork9_height(&self, height: u32) -> Result<Self> {
        let mut constants = self.0.clone();
        constants.soft_fork9_height = height;
        Ok(Self(constants))
    }

    /// Returns a copy of the constants marked as a test network or not.
    pub fn with_testnet(&self, testnet: bool) -> Result<Self> {
        let mut constants = self.0.clone();
        constants.testnet = testnet;
        Ok(Self(constants))
    }
}
