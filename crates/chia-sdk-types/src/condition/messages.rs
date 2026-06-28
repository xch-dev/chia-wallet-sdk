#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSide {
    Sender,
    Receiver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageFlags {
    pub parent: bool,
    pub puzzle: bool,
    pub amount: bool,
}

impl MessageFlags {
    pub const NONE: Self = Self::new(false, false, false);
    pub const PARENT: Self = Self::new(true, false, false);
    pub const PUZZLE: Self = Self::new(false, true, false);
    pub const AMOUNT: Self = Self::new(false, false, true);
    pub const PARENT_PUZZLE: Self = Self::new(true, true, false);
    pub const PARENT_AMOUNT: Self = Self::new(true, false, true);
    pub const PUZZLE_AMOUNT: Self = Self::new(false, true, true);
    pub const COIN: Self = Self::new(true, true, true);

    pub const fn new(parent: bool, puzzle: bool, amount: bool) -> Self {
        Self {
            parent,
            puzzle,
            amount,
        }
    }

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
