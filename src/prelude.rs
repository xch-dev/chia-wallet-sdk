//! The most commonly used types, traits, and functions from across the SDK.
//!
//! This includes core types from `chia-protocol`, BLS and secp keys, the CLVM [`Allocator`]
//! and conversion traits, the main driver types like [`SpendContext`], [`StandardLayer`], and
//! [`Cat`], the signer types, the [`Simulator`], [`Conditions`] and each condition type, and the
//! utilities in `chia-sdk-utils`. Everything else can be found in the modules at the crate root.
//!
//! The coinset RPC types are glob imported from `chia-sdk-coinset`, so they include
//! [`ChiaRpcClient`] and its response types.
//!
//! # Example
//!
//! ```
//! use chia_wallet_sdk::prelude::*;
//!
//! let mut sim = Simulator::new();
//! let mut ctx = SpendContext::new();
//!
//! let alice = sim.bls(1_000);
//! let hint = ctx.hint(alice.puzzle_hash)?;
//!
//! // Spend Alice's coin to a new coin with the same puzzle hash, minus a fee.
//! StandardLayer::new(alice.pk).spend(
//!     &mut ctx,
//!     alice.coin,
//!     Conditions::new()
//!         .create_coin(alice.puzzle_hash, 900, hint)
//!         .reserve_fee(100),
//! )?;
//!
//! sim.spend_coins(ctx.take(), &[alice.sk])?;
//!
//! let coins = sim.unspent_coins(alice.puzzle_hash, false);
//! assert_eq!(coins.iter().map(|coin| coin.amount).sum::<u64>(), 900);
//! # Ok::<(), anyhow::Error>(())
//! ```

pub use chia_bls::{PublicKey, SecretKey, Signature};
pub use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, CoinState, Program, SpendBundle};
pub use chia_secp::{K1PublicKey, K1SecretKey, K1Signature, R1PublicKey, R1SecretKey, R1Signature};
pub use clvm_traits;
pub use clvm_traits::{FromClvm, ToClvm};
pub use clvm_utils::{
    CurriedProgram, ToTreeHash, TreeHash, tree_hash, tree_hash_atom, tree_hash_pair,
};
pub use clvmr::{Allocator, Atom, NodePtr, SExp};

pub use chia_sdk_client::{Peer, PeerOptions};
pub use chia_sdk_coinset::*;
pub use chia_sdk_driver::{
    Action, Arbitrage, ArbitrageSide, AssetInfo, Cat, CatAssetInfo, CatInfo, CatSpend, ClawbackV2,
    CurriedPuzzle, Delta, Deltas, Did, DidInfo, DriverError, HashedPtr, Id, Launcher, Layer, Nft,
    NftAssetInfo, NftInfo, NftMint, Offer, OfferAmounts, OfferCoins, OptionAssetInfo,
    OptionContract, OptionInfo, OptionLauncher, OptionLauncherInfo, OptionMetadata, OptionType,
    OptionUnderlying, Outputs, Puzzle, RawPuzzle, Relation, RequestedPayments, RoyaltyInfo,
    SettlementLayer, Singleton, SingletonInfo, Spend, SpendAction, SpendContext,
    SpendWithConditions, Spends, StandardLayer, Vault, VaultInfo,
};
pub use chia_sdk_signer::{
    AggSigConstants, RequiredBlsSignature, RequiredSecpSignature, RequiredSignature,
};
pub use chia_sdk_test::{
    BlsPair, BlsPairWithCoin, K1Pair, R1Pair, Simulator, SimulatorConfig, SimulatorError,
};
pub use chia_sdk_types::{
    Compilation, Condition, Conditions, MAINNET_CONSTANTS, MerkleProof, MerkleTree, Mod,
    TESTNET11_CONSTANTS, compile_chialisp, compile_rue, conditions::*, run_puzzle,
};
pub use chia_sdk_utils::{Address, Bech32, parse_hex, select_coins};
