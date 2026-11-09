/// Which side of a message the [`MessageFlags`] apply to.
///
/// The mode of a [`SendMessage`](crate::conditions::SendMessage) or
/// [`ReceiveMessage`](crate::conditions::ReceiveMessage) condition stores the sender's flags in
/// the upper 3 bits and the receiver's flags in the lower 3 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSide {
    /// The coin that sends the message, stored in bits 3 to 5 of the mode.
    Sender,
    /// The coin that receives the message, stored in bits 0 to 2 of the mode.
    Receiver,
}

/// Which parts of a coin one side of a message commits to.
///
/// Each side of a message can commit to any combination of the other coin's parent coin id,
/// puzzle hash, and amount. Committing to all three ([`MessageFlags::COIN`]) is the same as
/// committing to the coin id.
///
/// ## Example
///
/// Build the mode for a message from a puzzle hash to a specific coin:
///
/// ```rust
/// # use chia_sdk_types::{MessageFlags, MessageSide};
/// let mode = MessageFlags::PUZZLE.encode(MessageSide::Sender)
///     | MessageFlags::COIN.encode(MessageSide::Receiver);
///
/// assert_eq!(mode, 0b010_111);
/// assert_eq!(MessageFlags::decode(mode, MessageSide::Sender), MessageFlags::PUZZLE);
/// assert_eq!(MessageFlags::decode(mode, MessageSide::Receiver), MessageFlags::COIN);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageFlags {
    /// Whether the parent coin id is committed to.
    pub parent: bool,
    /// Whether the puzzle hash is committed to.
    pub puzzle: bool,
    /// Whether the amount is committed to.
    pub amount: bool,
}

impl MessageFlags {
    /// Commits to nothing, so any coin can be on this side of the message.
    pub const NONE: Self = Self::new(false, false, false);
    /// Commits to the parent coin id.
    pub const PARENT: Self = Self::new(true, false, false);
    /// Commits to the puzzle hash.
    pub const PUZZLE: Self = Self::new(false, true, false);
    /// Commits to the amount.
    pub const AMOUNT: Self = Self::new(false, false, true);
    /// Commits to the parent coin id and puzzle hash.
    pub const PARENT_PUZZLE: Self = Self::new(true, true, false);
    /// Commits to the parent coin id and amount.
    pub const PARENT_AMOUNT: Self = Self::new(true, false, true);
    /// Commits to the puzzle hash and amount.
    pub const PUZZLE_AMOUNT: Self = Self::new(false, true, true);
    /// Commits to the parent coin id, puzzle hash, and amount, which identifies a single coin by
    /// its coin id.
    pub const COIN: Self = Self::new(true, true, true);

    /// Creates a new [`MessageFlags`] from whether each part is committed to.
    pub const fn new(parent: bool, puzzle: bool, amount: bool) -> Self {
        Self {
            parent,
            puzzle,
            amount,
        }
    }

    /// Decodes the flags for one side from a 6-bit message mode. Bits for the other side are
    /// ignored.
    pub const fn decode(value: u8, side: MessageSide) -> Self {
        match side {
            MessageSide::Sender => Self::new(
                value & 0b100_000 != 0,
                value & 0b010_000 != 0,
                value & 0b001_000 != 0,
            ),
            MessageSide::Receiver => Self::new(
                value & 0b000_100 != 0,
                value & 0b000_010 != 0,
                value & 0b000_001 != 0,
            ),
        }
    }

    /// Encodes the flags into the bits of a message mode for one side. Combine the sender and
    /// receiver bits with a bitwise or to get the full mode.
    pub const fn encode(self, side: MessageSide) -> u8 {
        match side {
            MessageSide::Sender => {
                (self.parent as u8) << 5 | (self.puzzle as u8) << 4 | (self.amount as u8) << 3
            }
            MessageSide::Receiver => {
                (self.parent as u8) << 2 | (self.puzzle as u8) << 1 | (self.amount as u8)
            }
        }
    }
}
