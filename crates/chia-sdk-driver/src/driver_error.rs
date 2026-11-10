use std::{array::TryFromSliceError, num::TryFromIntError};

use chia_protocol::Bytes32;
use chia_sdk_signer::SignerError;
use clvm_traits::{FromClvmError, ToClvmError};
use clvmr::error::EvalErr;
use thiserror::Error;

/// The error type returned by the drivers in this crate.
///
/// Most variants describe why a puzzle or spend couldn't be parsed or built. Errors from
/// dependencies, such as CLVM serialization, are wrapped with [`From`] conversions so that they
/// can be propagated with `?`.
#[derive(Debug, Error)]
pub enum DriverError {
    /// An I/O error, for example from zlib while compressing or decompressing an offer.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// An integer didn't fit in the type it was converted to.
    #[error("try from int error")]
    TryFromInt(#[from] TryFromIntError),

    /// A byte slice had the wrong length to be converted to a fixed-size array.
    #[error("try from slice error: {0}")]
    TryFromSlice(#[from] TryFromSliceError),

    /// A value couldn't be converted to CLVM.
    #[error("failed to serialize clvm value: {0}")]
    ToClvm(#[from] ToClvmError),

    /// A CLVM value didn't have the shape of the type it was parsed as. This is returned, for
    /// example, when a puzzle has a known mod hash but its curried arguments or solution are
    /// malformed.
    #[error("failed to deserialize clvm value: {0}")]
    FromClvm(#[from] FromClvmError),

    /// Running a puzzle failed, for example because it raised an error or exceeded the cost limit.
    #[error("clvm eval error: {0}")]
    Eval(#[from] EvalErr),

    /// The curried mod hash argument of a CAT, NFT state, NFT ownership, or revocation layer
    /// doesn't match the hash of the puzzle itself, so it wouldn't recreate the same layer.
    #[error("invalid mod hash")]
    InvalidModHash,

    /// The metadata updater puzzle revealed to update an NFT's metadata doesn't hash to the NFT's
    /// metadata updater puzzle hash.
    #[error("metadata updater puzzle hash mismatch")]
    MetadataUpdaterPuzzleHashMismatch,

    /// A puzzle has a known outer layer, but a part that is required to be standard didn't parse.
    /// For example, an NFT ownership layer whose transfer program isn't the expected transfer
    /// layer, or an action layer or state scheduler with unexpected curried arguments.
    #[error("non-standard inner puzzle layer")]
    NonStandardLayer,

    /// A state scheduler's schedule is empty, or its timestamps aren't strictly increasing.
    #[error("invalid state schedule: must be nonempty and strictly increasing by timestamp")]
    InvalidStateSchedule,

    /// A splitter's payout scheme is empty, has a zero share or a duplicate puzzle hash, or its
    /// shares add up to more than [`u64::MAX`]. This is returned by `SplitterLayer::new`, and when
    /// parsing a splitter puzzle whose curried scheme doesn't reserialize to the same puzzle hash.
    #[error(
        "invalid splitter: must have at least one recipient, unique puzzle hashes, nonzero shares, and a total that fits in a u64"
    )]
    InvalidSplitter,

    /// A fusion has a side without any NFTs, or lists the same NFT more than once. This is
    /// returned by `FusionLayer::new`, and when parsing a fusion puzzle whose other curried
    /// arguments aren't the standard values.
    #[error(
        "invalid fusion: each side must have at least one NFT, and an NFT can only be listed once"
    )]
    InvalidFusion,

    /// A fusion is launched or parsed with an even singleton amount, which the singleton puzzle
    /// would never allow to be spent.
    #[error("the fusion singleton has an amount of {0}, but it must be odd")]
    InvalidFusionSingletonAmount(u64),

    /// When parsing the child of a fusion, the solution is too short to contain the direction of
    /// the swap.
    #[error("the fusion solution is missing the direction of the swap")]
    InvalidFusionSolution,

    /// A fusion swap would send an unlocked NFT to the fusion's p2 puzzle hash (where it would stay
    /// locked) or to the settlement puzzle hash (where anyone could take it).
    #[error("an NFT can't be unlocked to the fusion's p2 puzzle or the settlement puzzle")]
    InvalidFusionTarget,

    /// The NFTs given to a fusion don't match its sides. For example, an NFT that isn't on either
    /// side is being locked, an NFT is missing from a swap, an offered NFT isn't in the settlement
    /// puzzle, or an NFT being unlocked isn't locked by the fusion.
    #[error("the NFTs don't match the side of the fusion that they're being swapped for")]
    FusionNftMismatch,

    /// An NFT being locked or swapped by a fusion doesn't have an amount of 1 and the default
    /// metadata updater, which the fusion puzzle requires.
    #[error(
        "the NFT can't be used in a fusion: it must have an amount of 1 and the default metadata updater"
    )]
    UnsupportedFusionNft,

    /// An offer can't be taken by `Fusion::take_offer`. For example, it doesn't offer exactly one
    /// side and request exactly the other, requests or offers anything else, offers an NFT with an
    /// assigned owner, or requests an NFT with a payment that the fusion puzzle doesn't make.
    #[error("the offer can't be taken by this fusion")]
    FusionOfferMismatch,

    /// A pool state isn't one that the reference wallet would create. For example, the version
    /// isn't supported, the owner key is the infinity point, a self pooling state has a pool URL or
    /// relative lock height, a pool's relative lock height is out of range, or a plot NFT is
    /// launched in the leaving pool state or with an even amount.
    #[error("invalid pool state: {0}")]
    InvalidPoolState(&'static str),

    /// A plot NFT can't move to the requested state from its current one. For example, leaving a
    /// pool that it isn't farming to, joining a pool without leaving the current one first, or
    /// moving to the state it's already in.
    #[error("invalid plot NFT transition: {0}")]
    InvalidPlotNftTransition(&'static str),

    /// A coin can't be claimed by a plot NFT. For example, no rewards were given, a reward is
    /// listed twice, the coin isn't locked to the plot NFT's p2 puzzle hash, it isn't a pool
    /// reward, its amount is odd, or it was farmed at height 0. This is also returned when
    /// parsing a solution whose reward height isn't an atom.
    #[error("invalid pool reward: {0}")]
    InvalidPoolReward(&'static str),

    /// A spend that must recreate an asset didn't create a child. For example, an NFT or
    /// datastore spend without an odd `CREATE_COIN`, or a CAT spend whose children can't be parsed
    /// when clear signing.
    #[error("missing child")]
    MissingChild,

    /// The child `CREATE_COIN` of a DID spend has no memos, so its p2 puzzle hash isn't known.
    #[error("missing hint")]
    MissingHint,

    /// A datastore coin is missing a memo that is needed to parse its state.
    #[error("missing memo")]
    MissingMemo,

    /// A memo is present but malformed. For example, a datastore memo with the wrong length, or a
    /// clawback memo that can't be parsed.
    #[error("invalid memo")]
    InvalidMemo,

    /// A curried singleton struct doesn't use the singleton top layer v1.1 mod hash and the
    /// singleton launcher puzzle hash.
    #[error("invalid singleton struct")]
    InvalidSingletonStruct,

    /// A data layer oracle fee is odd. Only even fees are allowed, since an odd `CREATE_COIN` would
    /// be treated as the child of the singleton.
    #[error("expected even oracle fee, but it was odd")]
    OddOracleFee,

    /// The total of the `RESERVE_FEE` conditions in an offer doesn't fit in a [`u64`].
    #[error("fee overflow")]
    FeeOverflow,

    /// An error with a custom message, used for less common failures. For example, constructing
    /// the puzzle of an `OracleLayer` with an odd fee, or an unsupported reward distributor
    /// operation.
    #[error("custom driver error: {0}")]
    Custom(String),

    /// A merkle proof couldn't be created or verified, because the leaf isn't in the merkle tree.
    #[error("invalid merkle proof")]
    InvalidMerkleProof,

    /// An action layer solution is malformed. Either a selector doesn't pick one of the revealed
    /// action puzzles, a selector has no solution, or the number of merkle proofs doesn't match the
    /// number of action spends.
    #[error("invalid action layer solution")]
    InvalidActionLayerSolution,

    /// An m-of-n vault spend doesn't provide spends for enough distinct members to meet the
    /// required number.
    #[error("invalid spend count for vault subpath")]
    InvalidSubpathSpendCount,

    /// A vault spend is missing the spend of a member puzzle that is required. For 1-of-n and
    /// n-of-n, this is also returned if there aren't enough distinct member spends.
    #[error("missing spend for vault subpath")]
    MissingSubpathSpend,

    /// Two members of a vault require different delegated puzzle wrappers at the same position,
    /// so no delegated puzzle can satisfy both of them.
    #[error("delegated puzzle wrapper conflict")]
    DelegatedPuzzleWrapperConflict,

    /// An m-of-n requires fewer than one, or more than all, of its members.
    #[error("m of n requires between 1 and {items} members, but {required} were required")]
    InvalidMofNRequired {
        /// The number of members that are required to spend.
        required: usize,
        /// The total number of members.
        items: usize,
    },

    /// An m-of-n memo lists the same member more than once.
    #[error("m of n contains duplicate members")]
    DuplicateMofNMembers,

    /// A vault memo is nested more deeply than is allowed when parsing it.
    #[error("mips memo is nested too deeply")]
    MipsMemoTooDeep,

    /// A vault memo contains more members and restrictions in total than is allowed when parsing
    /// it.
    #[error("mips memo is too large")]
    MipsMemoTooLarge,

    /// An action needs to output conditions, but none of the spends can emit them. For example,
    /// when only settlement coins are being spent.
    #[error("cannot emit conditions from spend")]
    CannotEmitConditions,

    /// A settle action applies to a coin that isn't spent with the settlement payments puzzle.
    #[error("cannot settle from spend")]
    CannotSettleFromSpend,

    /// An action needs a coin to create its output from, but there isn't one that can be used.
    /// For example, an earlier action already recreated or melted the singleton.
    #[error("there is no spendable source coin that can create the output without a conflict")]
    NoSourceForOutput,

    /// An action refers to an [`Id`](crate::Id) that doesn't match an asset of the right kind in
    /// the spends.
    #[error("invalid asset id")]
    InvalidAssetId,

    /// The selected coins don't cover the outputs of the transaction.
    #[error("the selected coins are insufficient to cover the outputs of the transaction")]
    InsufficientFunds,

    /// Two settlement coins with the same puzzle hash would make the same notarized payment, so a
    /// single payment assertion would be satisfied by either of them.
    #[error("two settlement coins with the same puzzle hash make the same notarized payment")]
    DuplicateSettlementPayment,

    /// A send or melt action for a singleton has an amount other than the singleton's amount.
    #[error("the amount does not match the amount of the singleton")]
    SingletonAmountMismatch,

    /// A CAT was added for revocation, but it doesn't have a revocation layer.
    #[error("the CAT does not have a revocation layer")]
    NotRevocable,

    /// An amount doesn't fit in a [`u64`], so it can't be used in a coin or condition. For
    /// example, a trade price in an offer, or the total of the fees.
    #[error("the amount is too large to fit in a coin")]
    AmountOverflow,

    /// [`Spends::finish_with_keys`](crate::Spends::finish_with_keys) wasn't given a public key for
    /// one of the p2 puzzle hashes being spent.
    #[error("missing key")]
    MissingKey,

    /// A required spend wasn't provided. For example, [`Spends::spend`](crate::Spends::spend)
    /// wasn't given the p2 spend of a coin, or a coin spend is missing when clear signing.
    #[error("missing spend")]
    MissingSpend,

    /// Compressed offer bytes are shorter than the 2-byte version prefix.
    #[cfg(feature = "offer-compression")]
    #[error("missing compression version prefix")]
    MissingVersionPrefix,

    /// Compressed offer bytes have a newer version prefix than is supported.
    #[cfg(feature = "offer-compression")]
    #[error("unsupported compression version")]
    UnsupportedVersion,

    /// A value couldn't be serialized or deserialized in the streamable format.
    #[error("streamable error: {0}")]
    Streamable(#[from] chia_traits::Error),

    /// The input to decompression isn't a zlib stream that uses a preset dictionary.
    #[cfg(feature = "offer-compression")]
    #[error("cannot decompress uncompressed input")]
    NotCompressed,

    /// The decompressed offer would be larger than the 6 MB limit used by chia-blockchain.
    #[cfg(feature = "offer-compression")]
    #[error("decompressed output exceeds maximum allowed size")]
    DecompressionTooLarge,

    /// zlib couldn't set the compression dictionary.
    #[cfg(feature = "offer-compression")]
    #[error("flate2 error: {0}")]
    Flate2(#[from] flate2::DecompressError),

    /// Bech32m encoding or decoding failed, for example because an encoded offer or address is
    /// invalid or has the wrong prefix.
    #[error("error when decoding address: {0}")]
    Decode(#[from] chia_sdk_utils::Bech32Error),

    /// A requested payment in an offer isn't XCH, a CAT, an NFT, or an option contract with the
    /// settlement payments puzzle as its p2 puzzle.
    #[error("requested payment is for an unsupported asset")]
    UnsupportedRequestedAsset,

    /// The same asset was given different [`AssetInfo`](crate::AssetInfo), or revocable and
    /// non-revocable CATs with the same asset id were combined.
    #[error("incompatible asset info")]
    IncompatibleAssetInfo,

    /// A requested CAT, NFT, or option contract has no entry in the
    /// [`AssetInfo`](crate::AssetInfo), so its settlement puzzle hash or royalty can't be
    /// calculated.
    #[error("missing required singleton asset info")]
    MissingAssetInfo,

    /// Offers being combined both offer the same coin, NFT, or option contract.
    #[error("conflicting inputs in offers")]
    ConflictingOfferInputs,

    /// An offer uses the retired `settlement_payments_old.clsp` puzzle, which is insecure.
    #[error("offer uses the old settlement payments puzzle")]
    OldSettlementPayment,

    /// A partial offer's minimum fill (stored as one less than the minimum) doesn't fit in a
    /// [`u64`] when parsing its hint or a fill.
    #[error("invalid partial offer terms")]
    InvalidPartialOffer,

    /// A partial offer fill is below the minimum, would release nothing, or would release more
    /// than the offer coin has left.
    #[error(
        "partial offer fill is below the minimum, pays out nothing, or exceeds the remaining amount"
    )]
    InvalidPartialOfferFill,

    /// A partial offer's persistent conditions are anything other than the optional expiration and
    /// required fee that `PartialOfferInfo::persistent_conditions` creates.
    #[error("partial offer has unsupported persistent conditions")]
    UnsupportedPartialOfferConditions,

    /// The maker's spend bundle doesn't create the partial offer coin with the expected puzzle hash
    /// and amount, or the lineage proof of a CAT offer coin can't be found in the parent spend.
    #[error("partial offer coin is not created by the maker's spends")]
    PartialOfferNotCreated,

    /// The maker's spend bundle creates more than one partial offer coin with the same terms, or the
    /// parent of the offer coin creates another coin with the same maker and requested asset memos.
    /// Those coins would share the payment nonce, so a single payment could fill all of them.
    #[error("partial offer coins with the same maker and requested asset can't share a parent")]
    PartialOfferSiblings,

    /// The partial offer coin is created by a settlement payments coin, whose solution anyone could
    /// change to take the funds.
    #[error("partial offer coin must not be created by a settlement payments coin")]
    UnsignedPartialOfferCreation,

    /// One of an auction's fees (the buyer's premium, the commission, or the NFT royalty) is more
    /// than `AUCTION_MAX_BPS` (100%).
    #[error("the auction's {name} of {bps} bps exceeds 10,000 bps")]
    AuctionBpsTooHigh {
        /// The name of the fee.
        name: &'static str,
        /// The fee, in basis points.
        bps: u64,
    },

    /// Two of an auction's payments (the buyer's premium, the commission, and the payout) have the
    /// same puzzle hash, so they could be identical coins and the auction couldn't be ended.
    #[error("puzzle hash {0} receives more than one payment when the auction ends")]
    DuplicateAuctionPaymentPuzzleHash(Bytes32),

    /// One of an auction's payments is sent to the settlement payments puzzle hash or the auction's
    /// lock puzzle hash, which are used by the auction itself.
    #[error("puzzle hash {0} can't receive an auction payment, since it's reserved by the auction")]
    ReservedAuctionPaymentPuzzleHash(Bytes32),

    /// When parsing an auction launch, the auction reconstructed from the memo and royalty info
    /// doesn't have the puzzle hash of the singleton that was launched.
    #[error("the launched singleton doesn't match the auction memo")]
    AuctionPuzzleHashMismatch,

    /// An auction is launched or parsed with a singleton amount other than
    /// `AUCTION_SINGLETON_AMOUNT`.
    #[error("the auction singleton has an amount of {0}, but it must be 1")]
    InvalidAuctionSingletonAmount(u64),

    /// The reserve passed to `Launcher::launch_auction` isn't an empty coin locked by the auction.
    #[error("the auction reserve must be an empty coin locked by the auction")]
    InvalidAuctionReserve,

    /// The NFT passed to `Launcher::launch_auction` isn't locked by the auction's lock puzzle hash.
    #[error("the NFT must be locked by the auction before it's launched")]
    AuctionNftNotLocked,

    /// CAT spends were passed to `Auction::spend` for an auction whose reserve is XCH.
    #[error("CAT spends can't be included with an auction that has an XCH reserve")]
    UnexpectedAuctionCatSpends,

    /// An offered NFT has a trade price whose royalty rounds down to zero, so the royalty payment
    /// can't be made. This is returned when calculating an offer's requested royalty payments.
    #[error("nft {0} has a trade price whose royalty rounds to zero, so it can't be settled")]
    ZeroRoyaltyPayment(Bytes32),

    /// A settlement puzzle hash isn't for XCH or any CAT in the [`AssetInfo`](crate::AssetInfo).
    /// For example, an NFT trade price in an offer whose asset isn't known.
    #[error("unknown asset with settlement puzzle hash {0}")]
    UnknownSettlementAsset(Bytes32),

    /// Calculating the signatures required by a spend failed.
    #[error("signer error: {0}")]
    Signer(#[from] SignerError),

    /// A vault's delegated puzzle isn't a quoted list of conditions, so it can't be clear signed.
    #[error("invalid delegated spend format")]
    InvalidDelegatedSpendFormat,

    /// A message sent by a vault isn't a puzzle-to-coin message with exactly one coin id.
    #[error("invalid vault message format")]
    InvalidVaultMessageFormat,

    /// A puzzle reveal doesn't hash to the coin's puzzle hash. For example, a coin spend in an
    /// untrusted offer, or a datastore child whose memos don't match the coin that was created.
    #[error("puzzle hash mismatch for coin spend")]
    WrongPuzzleHash,

    /// A clawback is nested inside another clawback, which clear signing doesn't allow.
    #[error("nested clawbacks are not allowed")]
    NestedClawback,

    /// A coin spend has a custody puzzle that clear signing doesn't recognize, but the vault sent
    /// it a message.
    #[error("linked spend has unknown custody puzzle but was sent a message")]
    InvalidLinkedCustody,

    /// A clawback spend requires the transaction to expire by a certain time, but the transaction
    /// isn't guaranteed to expire by then.
    #[error("the transaction is not guaranteed to expire when its clawed back spends expire")]
    UnguaranteedClawBack,

    /// When clear signing, the vault receives a clawback with the receiver path, but the
    /// transaction isn't guaranteed to only be valid after the clawback expires, so the sender
    /// could still claw it back.
    #[error(
        "the transaction is not guaranteed to only be valid after its received clawbacks expire"
    )]
    UnguaranteedClawbackExpiry,

    /// When clear signing, a CAT child has a different revocation layer than its parent, which
    /// could let someone else revoke it.
    #[error("the revocation layer of the child does not match the parent")]
    RevocableChild,

    /// The conditions of a spend don't match the message that authorizes it.
    #[error("conditions do not match message")]
    WrongConditions,

    /// A vault message doesn't match the custody conditions or a TAIL invocation of the coin it
    /// was sent to.
    #[error("vault message did not match any custody auth or TAIL invocation")]
    UnmatchedVaultMessage,

    /// When clear signing, the transaction asserts a requested payment of an option contract,
    /// which can't be reported as a clear signing asset.
    #[error("asserted requested payment is for an option contract, which can't be clear signed")]
    UnsupportedAssertedOptionPayment,

    /// A coin spend requires a message from the vault, but none was sent to it.
    #[error("missing message for linked custody puzzle")]
    MissingVaultMessage,

    /// More than one vault message matched the custody conditions, or the TAIL invocation, of the
    /// same coin spend.
    #[error("multiple vault messages matched the same custody slot")]
    DuplicateVaultMessage,

    /// An offer pre-split coin is linked to a different vault launcher id than the vault being
    /// clear signed.
    #[error("wrong linked offer launcher id")]
    WrongLinkedOfferLauncherId,

    /// A p2 conditions or singleton spend doesn't assert its own coin id, so the spend path could
    /// be swapped without invalidating the vault's message.
    #[error(
        "p2 conditions or singleton spend is missing AssertMyCoinId condition to prevent swapping the spend path"
    )]
    MissingP2ConditionsOrSingletonAssertion,

    /// An error from silent payment key generation or the address codec.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment error: {0}")]
    SilentPayment(#[from] chia_sdk_utils::silent_payments::SilentPaymentError),

    /// A silent-payment send needs the synthetic secret key for every spent XCH
    /// input. Some input's key was missing, so multi-party aggregation would be
    /// required — multi-party silent-payment flows are not currently supported.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment multi-party flow unsupported")]
    SilentPaymentMultiPartyUnsupported,

    /// The silent-payment send had no eligible spend to derive the output
    /// from: no XCH coin and no CAT coin (or revocable CAT coin on the holder's
    /// path) spent with the standard puzzle. At least one is required.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment requires an xch input or a CAT input held in the standard puzzle")]
    SilentPaymentNoXchInputs,

    /// The secret keys of the silent-payment spend group sum to zero mod r.
    /// ECDH with a zero key yields the identity point and a shared secret that
    /// anyone can compute, so CHIP-0057 requires the sender to fail.
    #[cfg(feature = "chip-0057")]
    #[error("aggregated sender key sum is zero - invalid for ECDH")]
    SilentPaymentZeroKeySum,

    /// The silent-payment input hash reduced to zero mod r, which would make
    /// the shared secret independent of the sender's keys.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment input hash is zero")]
    SilentPaymentZeroInputHash,

    /// A silent-payment output tweak reduced to zero mod r, which would make
    /// the one-time key equal to the address's own spend key.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment output tweak is zero")]
    SilentPaymentZeroTweak,

    /// A silent-payment scalar, such as an output tweak, is not below the
    /// group order `r`. Scalars the SDK derives are always reduced; one built
    /// from raw bytes may not be.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment scalar is not below the group order")]
    SilentPaymentScalarOutOfRange,

    /// More than `K_max` (2,400) silent-payment outputs were requested for a
    /// single scan key in one transaction. Scanners stop iterating a spend
    /// group at that index, so further outputs would never be found.
    #[cfg(feature = "chip-0057")]
    #[error("too many silent payment outputs for one scan key (the limit is 2400)")]
    SilentPaymentTooManyOutputs,

    /// The first memo was exactly 32 bytes, which the standard wallet promotes
    /// to a `puzzle_hash` hint and indexes — exposing the one-time puzzle hash
    /// and defeating silent-payment privacy. Prefix the payload with a sentinel
    /// byte so the first atom is no longer 32 bytes.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment memo hint forbidden")]
    SilentPaymentMemoHintForbidden,

    /// A silent-payment send whose transaction spends two or more XCH coins
    /// (counting intermediate coins that are created and spent inside it) must
    /// pass `Relation::AssertConcurrent` to `Spends::prepare` /
    /// `Spends::finish_with_keys`, so that scanners can reconstruct the spend
    /// group. A transaction that spends a single coin accepts any `Relation`.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment requires input binding")]
    SilentPaymentRequiresInputBinding,

    /// An intermediate coin (one that is created and spent inside the
    /// transaction) is part of the silent-payment spend group, but no secret
    /// key was registered for its puzzle hash. Its key is part of the key sum
    /// a scanner computes, so the payment would be undetectable without it.
    #[cfg(feature = "chip-0057")]
    #[error(
        "silent payment spend group contains an intermediate coin with no registered secret key"
    )]
    SilentPaymentIntermediateKeyMissing,

    /// A silent-payment output would be created by a coin that is not part of
    /// the spend group, such as a settlement coin. CHIP-0057 requires every
    /// silent-payment output to be created by a standard-puzzle coin of the
    /// group.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment output must be created by a standard-puzzle coin of the spend group")]
    SilentPaymentParentNotEligible,

    /// The coins that were bound together when the transaction was completed
    /// differ from the spend group the silent-payment outputs were derived
    /// from, so the payment would be undetectable.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment spend group changed after the outputs were derived")]
    SilentPaymentInputSetChanged,

    /// `Spends::with_silent_payment_keys` was not called before finish, so no
    /// silent-payment secret keys are registered for the spent inputs.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment keys not registered")]
    SilentPaymentKeysNotRegistered,

    /// A registered silent-payment key is not the synthetic key for its coin.
    /// `StandardArgs::curry_tree_hash(registered_pk)` must equal the coin's
    /// `p2_puzzle_hash` and `registered_sk.public_key()` must equal
    /// `registered_pk`; for `derive_settlement_outputs`, the public key of a
    /// group coin's secret key, curried into the standard puzzle and wrapped
    /// in the coin's wrapper, must give the coin's `puzzle_hash`. Pass
    /// synthetic keys (`derive_synthetic`) or construct them via
    /// `SyntheticSecretKey::from_raw`.
    #[cfg(feature = "chip-0057")]
    #[error("silent payment key not synthetic")]
    SilentPaymentKeyNotSynthetic,

    /// A silent-payment send spends a DID, an NFT or an option contract. These
    /// are not eligible spends in version 0 of CHIP-0057 (the closed list is XCH,
    /// CAT and holder-path revocable CAT coins held in the standard puzzle), and
    /// the `ASSERT_CONCURRENT_SPEND` cycle would pass through them, so the
    /// coins on either side of one would not be bound into the spend group.
    #[cfg(feature = "chip-0057")]
    #[error(
        "silent payment sends cannot spend a DID, NFT or option: they are not eligible spends in CHIP-0057 version 0"
    )]
    SilentPaymentIneligibleAssetBundle,

    /// A silent-payment send spends a revocable CAT through the issuer's
    /// hidden puzzle (`Spends::add_for_revocation`). CHIP-0057 counts only the
    /// holder's path of a revocable CAT as an eligible spend.
    #[cfg(feature = "chip-0057")]
    #[error("a revocable CAT spent through the issuer's hidden path is not an eligible spend")]
    SilentPaymentIssuerPathSpend,

    /// A silent-payment send also issues a new CAT (`Id::New`). Its eve coin
    /// would be the first spend of that asset in the cycle, so it would join
    /// the spend group; the SDK does not support that.
    #[cfg(feature = "chip-0057")]
    #[error("a CAT issued in the same transaction cannot be part of a silent payment spend group")]
    SilentPaymentNewCatUnsupported,

    /// A silent-payment output was requested in a CAT that no coin of the
    /// transaction holds. CHIP-0057 ("Outputs in CATs") requires every output
    /// to be created by a coin of its own asset in the spend group.
    #[cfg(feature = "chip-0057")]
    #[error("a silent payment output must be created by a coin of its asset in the spend group")]
    SilentPaymentAssetNotInGroup,

    /// A silent-payment CAT output was given caller memos. Its memo list is
    /// exactly the hint the SDK builds, the bare one-time puzzle hash `(p)`
    /// (CHIP-0057 "Outputs in CATs"), so caller memos are refused.
    #[cfg(feature = "chip-0057")]
    #[error(
        "a silent payment CAT output carries exactly the SDK-built hint; caller memos are not accepted"
    )]
    SilentPaymentCatMemos,

    /// The inputs of a settled silent payment are malformed (CHIP-0057
    /// "Payments through Settlement Coins"): the spend group given to
    /// `derive_settlement_outputs` is empty, or the action system has settled
    /// entries to group but no nonce to group them under.
    #[cfg(feature = "chip-0057")]
    #[error("invalid inputs for a silent payment through a settlement coin")]
    SilentPaymentInvalidSettlementInput,

    /// A silent payment is to be settled by a settlement coin of its asset
    /// that the transaction spends, and the transaction spends none (CHIP-0057
    /// "Payments through Settlement Coins", the taker's case).
    #[cfg(feature = "chip-0057")]
    #[error("no settlement coin of the asset is spent to settle the silent payment")]
    SilentPaymentSettlementCoinMissing,

    /// The settled entries of a party do not agree (CHIP-0057 "Payments
    /// through Settlement Coins"): they carry two different nonces, or the
    /// entries of one asset are paid both by the counterparty's settlement
    /// coin and by one the transaction spends. A party's requested payments
    /// in one asset are one notarized payment under one nonce.
    #[cfg(feature = "chip-0057")]
    #[error("the settled silent payment entries of a party disagree on the nonce or the payer")]
    SilentPaymentSettlementMismatch,

    /// A stream payment time is before the last payment time, or the stream has already ended.
    #[error("stream payment time is before the last payment time, or the stream has already ended")]
    InvalidStreamPaymentTime,

    /// A vault message envelope has no messages. This is returned when building an envelope
    /// without any messages, and when verifying a proof whose delegated spend doesn't send any.
    #[error("a vault message envelope must contain at least one message")]
    EmptyVaultMessageEnvelope,

    /// A delegated spend that sends vault messages does anything other than recreate the vault,
    /// such as creating other coins, sending other messages, or reserving a fee.
    #[error(
        "a delegated spend that signs vault messages can't contain any other conditions or messages, other than recreating the vault"
    )]
    MixedVaultMessageEnvelope,

    /// An encoded vault message proof is empty, so it doesn't have a version byte.
    #[error("missing vault message proof version")]
    MissingVaultMessageProofVersion,

    /// An encoded vault message proof has an unknown version byte, or uses the controlled version
    /// without any controller spends.
    #[error("unsupported vault message proof version {0}")]
    UnsupportedVaultMessageProofVersion(u8),

    /// The coin of a vault spend in a vault message proof doesn't have the singleton puzzle hash
    /// for the expected launcher id and custody hash, or its puzzle reveal doesn't hash to the
    /// coin's puzzle hash.
    #[error("the vault message proof is for a coin that isn't the expected vault")]
    WrongVaultPuzzleHash,

    /// A vault spend in a vault message proof isn't a singleton whose inner puzzle is a top-level
    /// MIPS puzzle.
    #[error("the vault message proof is for a vault whose custody isn't a top-level MIPS puzzle")]
    UnsupportedVaultCustody,

    /// A vault that signs messages, or that authorizes another vault to, isn't recreated with the
    /// same custody hash and amount.
    #[error(
        "a vault that signs messages, or authorizes another vault to, must be recreated with the same custody hash and amount"
    )]
    VaultMessageEnvelopeChangesVault,

    /// A vault message proof's spends assert a height or time, which can't be checked without the
    /// chain.
    #[error(
        "the vault message proof asserts a height or time, which can't be checked without the chain"
    )]
    VaultMessageTimelocked,

    /// A message in a vault message envelope is longer than [`VAULT_MESSAGE_MAX_LENGTH`](crate::VAULT_MESSAGE_MAX_LENGTH) bytes.
    #[error("a vault message can be at most 1024 bytes")]
    VaultMessageTooLong,

    /// A vault message proof is larger than can be decoded, or the cost of the bytes of its spends
    /// alone exceeds the maximum cost of verifying it.
    #[error("the vault message proof is too large for the maximum cost")]
    VaultMessageProofTooLarge,

    /// A vault message proof has more than
    /// [`VAULT_MESSAGE_PROOF_MAX_WRAPPERS`](crate::VAULT_MESSAGE_PROOF_MAX_WRAPPERS) delegated
    /// puzzle wrappers around the delegated spend.
    #[error("the vault message proof has too many delegated puzzle wrappers")]
    TooManyDelegatedPuzzleWrappers,

    /// A vault message proof has a delegated puzzle wrapper that forces a coin message, which the
    /// messages in the envelope would satisfy.
    #[error(
        "the vault message proof has a delegated puzzle wrapper that can't be used to sign messages"
    )]
    UnsupportedDelegatedPuzzleWrapper,

    /// The aggregated signature of a vault message proof isn't valid for its `AGG_SIG_*`
    /// conditions.
    #[error("the vault message proof's aggregated signature is invalid")]
    InvalidVaultMessageSignature,

    /// When clear signing, a revealed controlled vault's coin doesn't have the singleton puzzle
    /// hash for its launcher id and custody hash.
    #[error(
        "a controlled vault's coin doesn't have the singleton puzzle hash for its launcher id and custody hash"
    )]
    WrongControlledVaultPuzzleHash,

    /// When clear signing, the message that a controller sends to a controlled vault isn't the
    /// delegated puzzle hash that the controlled vault was revealed with.
    #[error(
        "the message sent to a controlled vault doesn't match the delegated puzzle hash it was revealed with"
    )]
    WrongControlledVaultMessage,

    /// When clear signing, the same vault is reached more than once in an authorization path, or
    /// more than one revealed controlled vault has the same puzzle hash.
    #[error("a controlled vault is authorized more than once in the same transaction")]
    DuplicateControlledVault,

    /// When clear signing, the chain of vaults that authorize each other's spends is longer than
    /// [`MAX_CONTROLLED_VAULT_DEPTH`](crate::MAX_CONTROLLED_VAULT_DEPTH).
    #[error("controlled vaults are nested too deeply")]
    ControlledVaultTooDeep,

    /// When clear signing, the revealed coin of the signing vault doesn't have the singleton puzzle
    /// hash for its launcher id and custody hash.
    #[error(
        "the coin of the vault being signed for doesn't have the singleton puzzle hash for its launcher id and custody hash"
    )]
    WrongSigningVaultPuzzleHash,

    /// When clear signing, a vault's coin or recreated child doesn't have an amount of 1, which
    /// every vault must have.
    #[error("a vault must be recreated with an amount of 1")]
    InvalidVaultAmount,

    /// When clear signing, the coin spend of a controlled vault isn't a singleton whose inner
    /// puzzle is a top-level MIPS puzzle.
    #[error(
        "the puzzle of a controlled vault must be a singleton whose inner puzzle is a top-level MIPS puzzle"
    )]
    InvalidControlledVaultPuzzle,

    /// A vault controller envelope isn't exactly the recreation of the controller, one message to
    /// a vault it controls, and the same message to the controller message coin. This is also
    /// returned if the controlled vault isn't part of the proof, or when clear signing a vault
    /// that signs messages with an authorization path that isn't made of controller envelopes.
    #[error(
        "a vault controller's delegated spend can only recreate the controller, send one message to the vault it controls, and send the same message to the controller message coin"
    )]
    InvalidVaultControllerEnvelope,

    /// When clear signing, a vault that authorizes another vault's spend does anything other than
    /// recreate itself and send that vault one message.
    #[error(
        "a vault that authorizes another vault's spend can only recreate itself and send that vault one message"
    )]
    InvalidVaultControllerSpend,

    /// When clear signing, a controlled vault that authorizes another vault's spend isn't recreated
    /// with the same custody hash and amount.
    #[error(
        "a controlled vault that authorizes another vault's spend must be recreated with the same custody hash and amount"
    )]
    VaultControllerChangesVault,

    /// A vault message proof has a spend of a vault controller whose launcher id isn't one of the
    /// given controllers, so its custody hash isn't known.
    #[error(
        "the vault message proof has a spend of vault controller {0}, whose custody hash wasn't given"
    )]
    UnknownVaultController(Bytes32),

    /// A vault message proof has more than
    /// [`VAULT_MESSAGE_PROOF_MAX_CONTROLLERS`](crate::VAULT_MESSAGE_PROOF_MAX_CONTROLLERS)
    /// controller spends.
    #[error("the vault message proof has too many vault controller spends")]
    TooManyVaultControllers,

    /// A vault message proof spends the same vault more than once.
    #[error("the vault message proof spends the same vault more than once")]
    DuplicateVaultControllerSpend,

    /// Running the spends of a vault message proof with the consensus rules failed.
    #[error("consensus validation failed: {0}")]
    Validation(#[from] chia_consensus::validation_error::ValidationErr),

    /// A Secure the Bag bag was built without any payments.
    #[error("a bag must contain at least one payment")]
    EmptyBag,

    /// Two payments in a bag would create a coin with the same puzzle hash.
    #[error("the bag already has a payment that creates a coin with puzzle hash {0}")]
    DuplicateBagPayment(Bytes32),

    /// A bag payment's clawback has a different receiver puzzle hash or amount than the payment.
    #[error("a bag payment's clawback must have the same receiver and amount as the payment")]
    BagClawbackMismatch,

    /// A bag's width is zero, or it's one and there's more than one payment, so the bag would
    /// never reach a single root.
    #[error("invalid bag width {width} for {payments} payments")]
    InvalidBagWidth {
        /// The maximum number of children of each node.
        width: usize,
        /// The number of payments in the bag.
        payments: usize,
    },

    /// A coin with the given p2 puzzle hash isn't a node or payment of the bag.
    #[error("puzzle hash {0} is not part of this bag")]
    UnknownBagNode(Bytes32),

    /// The coin that a path through a bag starts from isn't on the path to the coin with the given
    /// p2 puzzle hash.
    #[error("the coin isn't on the path to {0} in this bag")]
    NotBagAncestor(Bytes32),

    /// A coin in a bag doesn't have the amount that the bag expects for it.
    #[error("the coin's amount doesn't match the bag")]
    BagAmountMismatch,

    /// A payment in a bag has the same puzzle hash as one of the bag's nodes, which would make the
    /// bag ambiguous.
    #[error("a payment has the same puzzle hash as node {0} of the bag")]
    BagNodeCollision(Bytes32),

    /// The coin passed to a secured mint doesn't have the mint's pre-launcher puzzle hash.
    #[error("the coin isn't the pre-launcher of this secured mint")]
    NotPreLauncher,

    /// A secured mint's pre-launcher coin doesn't have an amount of 1.
    #[error("a pre-launcher must have an amount of 1, but it has {0}")]
    InvalidPreLauncherAmount(u64),

    /// A secured mint without a melt public key can't melt its pre-launcher coin.
    #[error("the secured mint has no melt public key, so its pre-launcher can't be melted")]
    NoMeltPublicKey,
}
