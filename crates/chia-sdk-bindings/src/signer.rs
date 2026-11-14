use bindy::Result;
use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32, CoinSpend};
use chia_sdk_signer::{RequiredSignature, SecpPublicKey};
use chia_sdk_types::{MAINNET_CONSTANTS, TESTNET11_CONSTANTS};

use crate::{Clvm, K1PublicKey, Program, R1PublicKey};

/// The network-specific additional data that is appended to `AGG_SIG_*` messages before they
/// are signed, so that signatures can't be replayed on another network or for another kind of
/// condition.
#[derive(Clone, Copy)]
pub struct AggSigConstants(chia_sdk_signer::AggSigConstants);

impl AggSigConstants {
    /// Derives the constants from the `AGG_SIG_ME` additional data, which is the network's
    /// genesis challenge on mainnet and testnet11.
    pub fn new(agg_sig_me: Bytes32) -> Result<Self> {
        Ok(Self(chia_sdk_signer::AggSigConstants::new(agg_sig_me)))
    }

    /// The constants for mainnet.
    pub fn mainnet() -> Result<Self> {
        Ok(Self((&*MAINNET_CONSTANTS).into()))
    }

    /// The constants for testnet11, which the simulator also uses.
    pub fn testnet11() -> Result<Self> {
        Ok(Self((&*TESTNET11_CONSTANTS).into()))
    }

    /// The additional data for `AGG_SIG_ME`.
    pub fn me(&self) -> Result<Bytes32> {
        Ok(self.0.me())
    }

    /// The additional data for `AGG_SIG_PARENT`.
    pub fn parent(&self) -> Result<Bytes32> {
        Ok(self.0.parent())
    }

    /// The additional data for `AGG_SIG_PUZZLE`.
    pub fn puzzle(&self) -> Result<Bytes32> {
        Ok(self.0.puzzle())
    }

    /// The additional data for `AGG_SIG_AMOUNT`.
    pub fn amount(&self) -> Result<Bytes32> {
        Ok(self.0.amount())
    }

    /// The additional data for `AGG_SIG_PARENT_AMOUNT`.
    pub fn parent_amount(&self) -> Result<Bytes32> {
        Ok(self.0.parent_amount())
    }

    /// The additional data for `AGG_SIG_PUZZLE_AMOUNT`.
    pub fn puzzle_amount(&self) -> Result<Bytes32> {
        Ok(self.0.puzzle_amount())
    }

    /// The additional data for `AGG_SIG_PARENT_PUZZLE`.
    pub fn parent_puzzle(&self) -> Result<Bytes32> {
        Ok(self.0.parent_puzzle())
    }
}

/// The signatures required by a list of coin spends, as calculated by
/// [`Clvm::required_signatures`].
#[derive(Clone)]
pub struct RequiredSignatures {
    /// The BLS signatures requested by `AGG_SIG_*` conditions, in the order of the coin spends.
    /// Sign each one and aggregate them into the spend bundle's aggregated signature.
    pub bls: Vec<RequiredBlsSignature>,
    /// The secp256k1 and secp256r1 signatures checked by the puzzles, in the order of the coin
    /// spends. Sign each one and put it in the solution in place of the placeholder.
    pub secp: Vec<RequiredSecpSignature>,
}

/// A BLS signature requested by an `AGG_SIG_*` condition.
#[derive(Clone)]
pub struct RequiredBlsSignature {
    /// The id of the coin whose spend requested the signature.
    pub coin_id: Bytes32,
    /// The public key that the signature must be made with.
    pub public_key: PublicKey,
    /// The message from the condition, before anything is appended.
    pub raw_message: Bytes,
    /// Information about the coin that is appended to the message, which depends on the kind of
    /// condition. This is empty for `AGG_SIG_UNSAFE`.
    pub appended_info: Bytes,
    /// The value from [`AggSigConstants`] that is appended last, or `None` for `AGG_SIG_UNSAFE`.
    pub domain_string: Option<Bytes32>,
}

impl RequiredBlsSignature {
    /// The full message to sign, which is the raw message followed by the appended info and
    /// the domain string.
    pub fn message(&self) -> Result<Bytes> {
        Ok(self.to_rust().message().into())
    }

    fn to_rust(&self) -> chia_sdk_signer::RequiredBlsSignature {
        chia_sdk_signer::RequiredBlsSignature {
            public_key: self.public_key,
            raw_message: self.raw_message.clone(),
            appended_info: self.appended_info.to_vec(),
            domain_string: self.domain_string,
        }
    }
}

/// A secp256k1 or secp256r1 signature checked by a `secp256k1_verify` or `secp256r1_verify`
/// operator in a puzzle.
///
/// Exactly one of `k1_public_key` and `r1_public_key` is set, depending on the curve. Sign
/// `message_hash` with `sign_prehashed`, and put the signature in the solution in place of
/// `placeholder`.
#[derive(Clone)]
pub struct RequiredSecpSignature {
    /// The id of the coin whose spend checks the signature.
    pub coin_id: Bytes32,
    /// The public key, if the puzzle uses `secp256k1_verify`.
    pub k1_public_key: Option<K1PublicKey>,
    /// The public key, if the puzzle uses `secp256r1_verify`.
    pub r1_public_key: Option<R1PublicKey>,
    /// The 32 byte message hash that must be signed with `sign_prehashed`.
    pub message_hash: Bytes32,
    /// The signature argument that was passed to the verify operator, which is usually the
    /// placeholder signature from the solution.
    pub placeholder: Program,
}

impl Clvm {
    /// Calculates the signatures required by the coin spends, by running each puzzle with its
    /// solution and collecting its `AGG_SIG_*` conditions and secp verify operators.
    ///
    /// Each puzzle reveal must hash to its coin's puzzle hash. Secp verify operators are treated
    /// as successful while calculating, so the solutions can contain placeholder signatures.
    pub fn required_signatures(
        &self,
        coin_spends: Vec<CoinSpend>,
        constants: AggSigConstants,
    ) -> Result<RequiredSignatures> {
        let mut ctx = self.0.lock().unwrap();

        let mut bls = Vec::new();
        let mut secp = Vec::new();

        for coin_spend in &coin_spends {
            let coin_id = coin_spend.coin.coin_id();

            for item in RequiredSignature::from_coin_spend(&mut ctx, coin_spend, &constants.0)? {
                match item {
                    RequiredSignature::Bls(item) => bls.push(RequiredBlsSignature {
                        coin_id,
                        public_key: item.public_key,
                        raw_message: item.raw_message,
                        appended_info: item.appended_info.into(),
                        domain_string: item.domain_string,
                    }),
                    RequiredSignature::Secp(item) => {
                        let (k1_public_key, r1_public_key) = match item.public_key {
                            SecpPublicKey::K1(public_key) => (Some(K1PublicKey(public_key)), None),
                            SecpPublicKey::R1(public_key) => (None, Some(R1PublicKey(public_key))),
                        };

                        secp.push(RequiredSecpSignature {
                            coin_id,
                            k1_public_key,
                            r1_public_key,
                            message_hash: item.message_hash.into(),
                            placeholder: Program(self.0.clone(), item.placeholder_ptr),
                        });
                    }
                }
            }
        }

        Ok(RequiredSignatures { bls, secp })
    }
}
