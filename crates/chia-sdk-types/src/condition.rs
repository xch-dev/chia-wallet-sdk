use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::Memos;
use chia_sdk_derive::conditions;
use nfts::TradePrice;

mod agg_sig;
mod announcements;
mod list;
mod messages;
mod nfts;

pub use announcements::*;
pub use list::*;
pub use messages::*;

conditions! {
    /// A condition that can be output by a puzzle, parsed into its typed representation.
    ///
    /// Puzzles return a list of conditions when they run. Each condition is a list that starts with
    /// an opcode, and the rest of the list contains its arguments. The consensus rules of the
    /// blockchain interpret the conditions to decide what the spend does, such as creating coins or
    /// requiring signatures. Some puzzles (such as the singleton, CAT, and NFT layers) also look
    /// for special conditions that are removed before the conditions reach consensus.
    ///
    /// When parsing, the variants are tried in order, and anything that doesn't match is kept as
    /// [`Condition::Other`]. The generic `T` is the type of unparsed values (such as memos,
    /// remarks, and other conditions), which is usually [`NodePtr`](clvmr::NodePtr).
    ///
    /// Each variant wraps a struct of the same name from the [`conditions`](mod@conditions) module.
    /// There are constructor and accessor methods for every variant, such as
    /// [`Condition::create_coin`], [`Condition::into_create_coin`], [`Condition::as_create_coin`],
    /// and [`Condition::is_create_coin`].
    ///
    /// ## Example
    ///
    /// ```rust
    /// # use chia_protocol::Bytes32;
    /// # use chia_puzzle_types::Memos;
    /// # use chia_sdk_types::{Condition, conditions::CreateCoin};
    /// # use clvm_traits::{FromClvm, ToClvm};
    /// # use clvmr::{Allocator, NodePtr};
    /// let mut allocator = Allocator::new();
    ///
    /// let condition: Condition = Condition::create_coin(Bytes32::new([1; 32]), 1000, Memos::None);
    /// assert!(condition.is_create_coin());
    ///
    /// // Conditions serialize to the same CLVM a puzzle would output.
    /// let ptr = condition.to_clvm(&mut allocator)?;
    /// let parsed = Condition::<NodePtr>::from_clvm(&allocator, ptr)?;
    ///
    /// let create_coin = parsed.into_create_coin().expect("this is a create coin condition");
    /// assert_eq!(create_coin, CreateCoin::new(Bytes32::new([1; 32]), 1000, Memos::None));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub enum Condition<T> {
        /// Does nothing. Puzzles can use it to attach arbitrary data to a spend.
        ///
        /// The consensus rules ignore everything after the opcode.
        Remark<T> as Copy {
            opcode: i8 if 1,
            /// The arbitrary data after the opcode.
            ...rest: T,
        },
        /// Requires a BLS signature by `public_key` on `message`, combined with the coin's parent
        /// coin id and the network's `AGG_SIG_PARENT` additional data.
        AggSigParent {
            opcode: i8 if 43,
            /// The public key that must sign the message.
            public_key: PublicKey,
            /// The message to sign, before any coin or network data required by the condition is
            /// appended.
            message: Bytes,
        },
        /// Requires a BLS signature by `public_key` on `message`, combined with the coin's puzzle
        /// hash and the network's `AGG_SIG_PUZZLE` additional data.
        AggSigPuzzle {
            opcode: i8 if 44,
            /// The public key that must sign the message.
            public_key: PublicKey,
            /// The message to sign, before any coin or network data required by the condition is
            /// appended.
            message: Bytes,
        },
        /// Requires a BLS signature by `public_key` on `message`, combined with the coin's amount
        /// and the network's `AGG_SIG_AMOUNT` additional data.
        AggSigAmount {
            opcode: i8 if 45,
            /// The public key that must sign the message.
            public_key: PublicKey,
            /// The message to sign, before any coin or network data required by the condition is
            /// appended.
            message: Bytes,
        },
        /// Requires a BLS signature by `public_key` on `message`, combined with the coin's puzzle
        /// hash and amount, and the network's `AGG_SIG_PUZZLE_AMOUNT` additional data.
        AggSigPuzzleAmount {
            opcode: i8 if 46,
            /// The public key that must sign the message.
            public_key: PublicKey,
            /// The message to sign, before any coin or network data required by the condition is
            /// appended.
            message: Bytes,
        },
        /// Requires a BLS signature by `public_key` on `message`, combined with the coin's parent
        /// coin id and amount, and the network's `AGG_SIG_PARENT_AMOUNT` additional data.
        AggSigParentAmount {
            opcode: i8 if 47,
            /// The public key that must sign the message.
            public_key: PublicKey,
            /// The message to sign, before any coin or network data required by the condition is
            /// appended.
            message: Bytes,
        },
        /// Requires a BLS signature by `public_key` on `message`, combined with the coin's parent
        /// coin id and puzzle hash, and the network's `AGG_SIG_PARENT_PUZZLE` additional data.
        AggSigParentPuzzle {
            opcode: i8 if 48,
            /// The public key that must sign the message.
            public_key: PublicKey,
            /// The message to sign, before any coin or network data required by the condition is
            /// appended.
            message: Bytes,
        },
        /// Requires a BLS signature by `public_key` on `message` alone.
        ///
        /// Nothing about the coin or network is added to the message, so the signature can be
        /// replayed on any other spend that requires the same message. Consensus rejects messages
        /// that end with any of the network's additional data values.
        AggSigUnsafe {
            opcode: i8 if 49,
            /// The public key that must sign the message.
            public_key: PublicKey,
            /// The message to sign, before any coin or network data required by the condition is
            /// appended.
            message: Bytes,
        },
        /// Requires a BLS signature by `public_key` on `message`, combined with the coin id and the
        /// network's `AGG_SIG_ME` additional data (the genesis challenge on mainnet).
        ///
        /// This is the most common signature condition, since it binds the signature to one coin on
        /// one network and prevents replays.
        AggSigMe {
            opcode: i8 if 50,
            /// The public key that must sign the message.
            public_key: PublicKey,
            /// The message to sign, before any coin or network data required by the condition is
            /// appended.
            message: Bytes,
        },
        /// Creates a new coin with the given puzzle hash and amount, whose parent is the coin being
        /// spent.
        ///
        /// The memos are optional. By convention, the first memo is a hint (usually the inner
        /// puzzle hash of the receiver) that lets wallets find coins sent to them.
        CreateCoin<T> as Copy {
            opcode: i8 if 51,
            /// The puzzle hash of the new coin.
            puzzle_hash: Bytes32,
            /// The amount of the new coin, in mojos.
            amount: u64,
            /// Optional memos, which are not part of the coin id. Use [`Memos::None`] to omit them.
            ...memos: Memos<T>,
        },
        /// Asserts that the spend bundle pays at least `amount` mojos in fees.
        ///
        /// This doesn't create the fee by itself. The fee is whatever is left over after
        /// subtracting the amounts of the created coins from the amounts of the spent coins.
        ReserveFee as Copy {
            opcode: i8 if 52,
            /// The minimum fee, in mojos.
            amount: u64,
        },
        /// Creates an announcement that other spends in the same block can assert with
        /// [`AssertCoinAnnouncement`].
        ///
        /// The announcement id is the hash of the coin id and the message. See
        /// [`announcement_id`](crate::announcement_id).
        CreateCoinAnnouncement {
            opcode: i8 if 60,
            /// The announced message.
            message: Bytes,
        },
        /// Asserts that a [`CreateCoinAnnouncement`] with the given announcement id was made in the
        /// same block.
        ///
        /// Use [`announcement_id`](crate::announcement_id) with the announcing coin's id to compute
        /// the id.
        AssertCoinAnnouncement as Copy {
            opcode: i8 if 61,
            /// The hash of the announcing coin id and the message.
            announcement_id: Bytes32,
        },
        /// Creates an announcement that other spends in the same block can assert with
        /// [`AssertPuzzleAnnouncement`].
        ///
        /// The announcement id is the hash of the coin's puzzle hash and the message. See
        /// [`announcement_id`](crate::announcement_id).
        CreatePuzzleAnnouncement {
            opcode: i8 if 62,
            /// The announced message.
            message: Bytes,
        },
        /// Asserts that a [`CreatePuzzleAnnouncement`] with the given announcement id was made in
        /// the same block.
        ///
        /// Use [`announcement_id`](crate::announcement_id) with the announcing coin's puzzle hash
        /// to compute the id.
        AssertPuzzleAnnouncement as Copy {
            opcode: i8 if 63,
            /// The hash of the announcing puzzle hash and the message.
            announcement_id: Bytes32,
        },
        /// Asserts that the coin with the given id is spent in the same block.
        AssertConcurrentSpend as Copy {
            opcode: i8 if 64,
            /// The id of the coin that must also be spent.
            coin_id: Bytes32,
        },
        /// Asserts that a coin with the given puzzle hash is spent in the same block.
        AssertConcurrentPuzzle as Copy {
            opcode: i8 if 65,
            /// The puzzle hash of a coin that must also be spent.
            puzzle_hash: Bytes32,
        },
        /// Sends a message to another coin spend, which must receive it with a matching
        /// [`ReceiveMessage`] condition in the same block.
        ///
        /// The `mode` is a 6-bit mask. The upper 3 bits say which parts of the sender (parent coin
        /// id, puzzle hash, amount) the receiver commits to, and the lower 3 bits say the same for
        /// the receiver. Use [`MessageFlags`](crate::MessageFlags) to build it. The `data`
        /// identifies the receiver, with one value for each receiver bit that is set (or a single
        /// coin id if all three are set).
        SendMessage<T> {
            opcode: i8 if 66,
            /// The sender and receiver mode bits. See [`MessageFlags`](crate::MessageFlags).
            mode: u8,
            /// The message being sent.
            message: Bytes,
            /// The receiver's committed information, as selected by the lower 3 bits of the mode.
            ...data: Vec<T>,
        },
        /// Receives a message sent by another coin spend with a matching [`SendMessage`] condition
        /// in the same block.
        ///
        /// The `mode` must match the sender's. The `data` identifies the sender, with one value for
        /// each sender bit that is set (or a single coin id if all three are set).
        ReceiveMessage<T> {
            opcode: i8 if 67,
            /// The sender and receiver mode bits. See [`MessageFlags`](crate::MessageFlags).
            mode: u8,
            /// The message being received.
            message: Bytes,
            /// The sender's committed information, as selected by the upper 3 bits of the mode.
            ...data: Vec<T>,
        },
        /// Asserts that the coin being spent has the given coin id.
        AssertMyCoinId as Copy {
            opcode: i8 if 70,
            /// The expected coin id.
            coin_id: Bytes32,
        },
        /// Asserts that the coin being spent has the given parent coin id.
        AssertMyParentId as Copy {
            opcode: i8 if 71,
            /// The expected parent coin id.
            parent_id: Bytes32,
        },
        /// Asserts that the coin being spent has the given puzzle hash.
        AssertMyPuzzleHash as Copy {
            opcode: i8 if 72,
            /// The expected puzzle hash.
            puzzle_hash: Bytes32,
        },
        /// Asserts that the coin being spent has the given amount.
        AssertMyAmount as Copy {
            opcode: i8 if 73,
            /// The expected amount, in mojos.
            amount: u64,
        },
        /// Asserts that the coin being spent was created in a block with the given timestamp.
        AssertMyBirthSeconds as Copy {
            opcode: i8 if 74,
            /// The expected creation timestamp, in seconds since the Unix epoch.
            seconds: u64,
        },
        /// Asserts that the coin being spent was created at the given block height.
        AssertMyBirthHeight as Copy {
            opcode: i8 if 75,
            /// The expected creation height.
            height: u32,
        },
        /// Asserts that the coin being spent was created in the same block, so it's ephemeral.
        AssertEphemeral as Default + Copy {
            opcode: i8 if 76,
        },
        /// Asserts that at least the given number of seconds have passed since the coin was
        /// created.
        AssertSecondsRelative as Copy {
            opcode: i8 if 80,
            /// The minimum number of seconds since the coin was created.
            seconds: u64,
        },
        /// Asserts that the previous transaction block's timestamp is at least the given value.
        AssertSecondsAbsolute as Copy {
            opcode: i8 if 81,
            /// The minimum timestamp, in seconds since the Unix epoch.
            seconds: u64,
        },
        /// Asserts that at least the given number of blocks have passed since the coin was created.
        AssertHeightRelative as Copy {
            opcode: i8 if 82,
            /// The minimum number of blocks since the coin was created.
            height: u32,
        },
        /// Asserts that the block height is at least the given value.
        AssertHeightAbsolute as Copy {
            opcode: i8 if 83,
            /// The minimum block height.
            height: u32,
        },
        /// Asserts that fewer than the given number of seconds have passed since the coin was
        /// created.
        AssertBeforeSecondsRelative as Copy {
            opcode: i8 if 84,
            /// The number of seconds after the coin's creation when this spend stops being valid.
            seconds: u64,
        },
        /// Asserts that the previous transaction block's timestamp is less than the given value.
        AssertBeforeSecondsAbsolute as Copy {
            opcode: i8 if 85,
            /// The timestamp, in seconds since the Unix epoch, when this spend stops being valid.
            seconds: u64,
        },
        /// Asserts that fewer than the given number of blocks have passed since the coin was
        /// created.
        AssertBeforeHeightRelative as Copy {
            opcode: i8 if 86,
            /// The number of blocks after the coin's creation when this spend stops being valid.
            height: u32,
        },
        /// Asserts that the block height is less than the given value.
        AssertBeforeHeightAbsolute as Copy {
            opcode: i8 if 87,
            /// The block height when this spend stops being valid.
            height: u32,
        },
        /// Reserves room for conditions that may be added in a future soft fork.
        ///
        /// Consensus charges the given cost and otherwise ignores the rest of the condition.
        Softfork<T> as Copy {
            opcode: i8 if 90,
            /// The cost to charge, in units of 10,000 CLVM cost.
            cost: u64,
            /// The arguments after the cost, which are currently ignored.
            ...rest: T,
        },
        /// Changes the owner DID of an NFT.
        ///
        /// This isn't a consensus condition. It's output by the inner puzzle of an NFT, and the NFT
        /// ownership layer removes it and passes it to the transfer program. The transfer program
        /// asserts that the new DID approves the transfer (unless the owner is being removed or
        /// stays the same), and asserts royalty payments for each trade price.
        TransferNft as Default {
            opcode: i8 if -10,
            /// The launcher id of the new owner DID, or [`None`] to remove the owner.
            launcher_id: Option<Bytes32>,
            /// The trade prices that royalties need to be paid on, if the NFT is being traded.
            trade_prices: Vec<TradePrice>,
            /// The inner puzzle hash of the new owner DID, used to check that it approves the
            /// transfer.
            singleton_inner_puzzle_hash: Option<Bytes32>,
        },
        /// Runs a CAT's TAIL program, which is needed to issue or melt the CAT.
        ///
        /// This isn't a consensus condition. It's a `CREATE_COIN` with an empty puzzle hash and the
        /// magic amount `-113`, which the CAT puzzle removes from the inner puzzle's output before
        /// running `program` with `solution`.
        RunCatTail<P, S> as Copy {
            opcode: i8 if 51,
            puzzle_hash: () if (),
            magic_amount: i8 if -113,
            /// The TAIL program, whose tree hash must be the CAT's asset id.
            program: P,
            /// The solution to pass to the TAIL program.
            solution: S,
        },
        /// Melts a singleton, so that it isn't recreated.
        ///
        /// This isn't a consensus condition. It's a `CREATE_COIN` with an empty puzzle hash and the
        /// magic amount `-113`, which the singleton top layer removes from the output instead of
        /// recreating the singleton.
        MeltSingleton as Default + Copy {
            opcode: i8 if 51,
            puzzle_hash: () if (),
            magic_amount: i8 if -113,
        },
        /// Updates the metadata of an NFT by running its metadata updater puzzle.
        ///
        /// This isn't a consensus condition. It's output by the inner puzzle of an NFT, and the NFT
        /// state layer removes it and runs the updater. The updater's tree hash must match the
        /// current metadata updater puzzle hash, and it can only be used once per spend.
        UpdateNftMetadata<P, S> as Copy {
            opcode: i8 if -24,
            /// The metadata updater puzzle, such as
            /// [`NftMetadataUpdater`](crate::puzzles::NftMetadataUpdater).
            updater_puzzle_reveal: P,
            /// The solution to pass to the metadata updater.
            updater_solution: S,
        },
        /// Updates the merkle root of delegated puzzles for a `DataLayer` store that uses the
        /// delegation layer puzzle from CHIP-0035 (`DelegationLayerArgs`, which requires the
        /// `chip-0035` feature).
        ///
        /// This isn't a consensus condition. It can be output by a delegated puzzle (such as an
        /// admin), and the delegation layer removes it and recreates itself with the new merkle
        /// root and memos. The writer layer puzzle (`WriterLayerArgs`) prevents writers from
        /// outputting it.
        UpdateDatastoreMerkleRoot {
            opcode: i8 if -13,
            /// The new merkle root of the delegated puzzle hashes.
            new_merkle_root: Bytes32,
            /// The memos to use when recreating the delegation layer, which replace the default
            /// hint.
            ...memos: Vec<Bytes>,
        },
    }
}
