use chia_consensus::opcodes::{
    AGG_SIG_AMOUNT, AGG_SIG_ME, AGG_SIG_PARENT, AGG_SIG_PARENT_AMOUNT, AGG_SIG_PARENT_PUZZLE,
    AGG_SIG_PUZZLE, AGG_SIG_PUZZLE_AMOUNT, AGG_SIG_UNSAFE, ASSERT_BEFORE_HEIGHT_ABSOLUTE,
    ASSERT_BEFORE_HEIGHT_RELATIVE, ASSERT_BEFORE_SECONDS_ABSOLUTE, ASSERT_BEFORE_SECONDS_RELATIVE,
    ASSERT_COIN_ANNOUNCEMENT, ASSERT_CONCURRENT_PUZZLE, ASSERT_CONCURRENT_SPEND, ASSERT_EPHEMERAL,
    ASSERT_HEIGHT_ABSOLUTE, ASSERT_HEIGHT_RELATIVE, ASSERT_MY_AMOUNT, ASSERT_MY_BIRTH_HEIGHT,
    ASSERT_MY_BIRTH_SECONDS, ASSERT_MY_COIN_ID, ASSERT_MY_PARENT_ID, ASSERT_MY_PUZZLEHASH,
    ASSERT_PUZZLE_ANNOUNCEMENT, ASSERT_SECONDS_ABSOLUTE, ASSERT_SECONDS_RELATIVE, CREATE_COIN,
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, REMARK, RESERVE_FEE,
    SEND_MESSAGE, SOFTFORK,
};
use clvm_traits::{ClvmDecoder, FromClvmError};

/// The magic amount for a `CREATE_COIN` that's intercepted by an outer puzzle (such as the CAT or
/// singleton layer) rather than passed to consensus.
const MAGIC_AMOUNT: &[u8] = &[0x8f];

/// The result of checking a condition against the consensus rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusCheck {
    /// Consensus accepts the condition.
    Valid,
    /// Consensus accepts the condition, but treats it as always true.
    TriviallyTrue,
    /// The condition isn't known to consensus, so it's ignored.
    Ignored,
}

/// Checks a condition's arguments the same way that consensus does in block validation (rather than
/// the stricter mempool rules). If consensus would reject the condition, this returns an error.
///
/// This mirrors `parse_args` in `chia-consensus`, but works with any decoder. The only rule that isn't
/// checked is that an `AGG_SIG_UNSAFE` message can't end with the additional data for another kind of
/// signature, since that depends on the network.
pub fn check_consensus<D>(
    decoder: &D,
    opcode: &[u8],
    args: D::Node,
) -> Result<ConsensusCheck, FromClvmError>
where
    D: ClvmDecoder,
{
    // Consensus only knows about single byte opcodes. Two byte opcodes are reserved for future soft forks
    // and are always accepted, and any other opcode is ignored.
    let [opcode] = opcode else {
        return Ok(ConsensusCheck::Ignored);
    };

    let mut args = Args {
        decoder,
        node: args,
    };

    match u16::from(*opcode) {
        AGG_SIG_UNSAFE
        | AGG_SIG_ME
        | AGG_SIG_PARENT
        | AGG_SIG_PUZZLE
        | AGG_SIG_AMOUNT
        | AGG_SIG_PUZZLE_AMOUNT
        | AGG_SIG_PARENT_AMOUNT
        | AGG_SIG_PARENT_PUZZLE => {
            args.hash(48)?;
            args.message()?;
        }
        CREATE_COIN => {
            let puzzle_hash = args.next()?;
            let amount = args.next()?;

            if args.is_atom(&amount, MAGIC_AMOUNT) {
                return Ok(ConsensusCheck::Ignored);
            }

            args.check_hash(&puzzle_hash, 32)?;
            args.amount(&amount)?;
        }
        RESERVE_FEE | ASSERT_MY_AMOUNT => {
            let amount = args.next()?;
            args.amount(&amount)?;
        }
        CREATE_COIN_ANNOUNCEMENT | CREATE_PUZZLE_ANNOUNCEMENT => {
            args.message()?;
        }
        ASSERT_COIN_ANNOUNCEMENT
        | ASSERT_PUZZLE_ANNOUNCEMENT
        | ASSERT_CONCURRENT_SPEND
        | ASSERT_CONCURRENT_PUZZLE
        | ASSERT_MY_COIN_ID
        | ASSERT_MY_PARENT_ID
        | ASSERT_MY_PUZZLEHASH => {
            args.hash(32)?;
        }
        ASSERT_MY_BIRTH_SECONDS => {
            args.uint(8)?.value()?;
        }
        ASSERT_MY_BIRTH_HEIGHT | SOFTFORK => {
            args.uint(4)?.value()?;
        }
        ASSERT_EPHEMERAL | REMARK => {}
        ASSERT_SECONDS_RELATIVE | ASSERT_SECONDS_ABSOLUTE => {
            return args.uint(8)?.lower_bound();
        }
        ASSERT_HEIGHT_RELATIVE | ASSERT_HEIGHT_ABSOLUTE => {
            return args.uint(4)?.lower_bound();
        }
        ASSERT_BEFORE_SECONDS_RELATIVE | ASSERT_BEFORE_SECONDS_ABSOLUTE => {
            return args.uint(8)?.upper_bound();
        }
        ASSERT_BEFORE_HEIGHT_RELATIVE | ASSERT_BEFORE_HEIGHT_ABSOLUTE => {
            return args.uint(4)?.upper_bound();
        }
        SEND_MESSAGE | RECEIVE_MESSAGE => {
            let mode = args.message_mode()?;
            args.message()?;

            // The sender commits to the receiver's coin, and vice versa.
            let other_side = if u16::from(*opcode) == SEND_MESSAGE {
                mode & 0b111
            } else {
                (mode >> 3) & 0b111
            };

            args.spend_id(other_side)?;
        }
        _ => return Ok(ConsensusCheck::Ignored),
    }

    Ok(ConsensusCheck::Valid)
}

struct Args<'a, D: ClvmDecoder> {
    decoder: &'a D,
    node: D::Node,
}

impl<D: ClvmDecoder> Args<'_, D> {
    fn next(&mut self) -> Result<D::Node, FromClvmError> {
        let (first, rest) = self
            .decoder
            .decode_pair(&self.node)
            .map_err(|_| invalid("missing condition argument"))?;
        self.node = rest;
        Ok(first)
    }

    fn atom(&self, node: &D::Node) -> Result<Vec<u8>, FromClvmError> {
        self.decoder
            .decode_atom(node)
            .map(|atom| atom.as_ref().to_vec())
            .map_err(|_| invalid("condition argument must be an atom"))
    }

    fn is_atom(&self, node: &D::Node, expected: &[u8]) -> bool {
        self.decoder
            .decode_atom(node)
            .is_ok_and(|atom| atom.as_ref() == expected)
    }

    fn check_hash(&self, node: &D::Node, size: usize) -> Result<(), FromClvmError> {
        if self.atom(node)?.len() != size {
            return Err(invalid("condition argument has the wrong length"));
        }
        Ok(())
    }

    fn hash(&mut self, size: usize) -> Result<(), FromClvmError> {
        let node = self.next()?;
        self.check_hash(&node, size)
    }

    fn message(&mut self) -> Result<(), FromClvmError> {
        let node = self.next()?;
        if self.atom(&node)?.len() > 1024 {
            return Err(invalid("condition message is longer than 1024 bytes"));
        }
        Ok(())
    }

    fn amount(&self, node: &D::Node) -> Result<u64, FromClvmError> {
        sanitize_uint(&self.atom(node)?, 8)?.value()
    }

    fn uint(&mut self, max_size: usize) -> Result<Uint, FromClvmError> {
        let node = self.next()?;
        sanitize_uint(&self.atom(&node)?, max_size)
    }

    fn message_mode(&mut self) -> Result<u8, FromClvmError> {
        let node = self.next()?;
        let mode = small_number(&self.atom(&node)?).ok_or(invalid("invalid message mode"))?;

        // Only 6 bits are allowed to be set.
        u8::try_from(mode)
            .ok()
            .filter(|mode| mode & !0b11_1111 == 0)
            .ok_or(invalid("invalid message mode"))
    }

    fn spend_id(&mut self, mode: u8) -> Result<(), FromClvmError> {
        // If the parent, puzzle hash, and amount are all committed to, the coin id is used instead.
        if mode == 0b111 {
            return self.hash(32);
        }

        if mode & 0b100 != 0 {
            self.hash(32)?;
        }

        if mode & 0b010 != 0 {
            self.hash(32)?;
        }

        if mode & 0b001 != 0 {
            let node = self.next()?;
            self.amount(&node)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Uint {
    Value(u64),
    PositiveOverflow,
    NegativeOverflow,
}

impl Uint {
    fn value(self) -> Result<u64, FromClvmError> {
        match self {
            Self::Value(value) => Ok(value),
            Self::PositiveOverflow | Self::NegativeOverflow => {
                Err(invalid("condition argument is out of range"))
            }
        }
    }

    /// A lower bound that's negative is always true, but one that's too large can never be true.
    fn lower_bound(self) -> Result<ConsensusCheck, FromClvmError> {
        match self {
            Self::Value(_) => Ok(ConsensusCheck::Valid),
            Self::NegativeOverflow => Ok(ConsensusCheck::TriviallyTrue),
            Self::PositiveOverflow => Err(invalid("timelock can never be satisfied")),
        }
    }

    /// An upper bound that's too large is always true, but one that's negative can never be true.
    fn upper_bound(self) -> Result<ConsensusCheck, FromClvmError> {
        match self {
            Self::Value(_) => Ok(ConsensusCheck::Valid),
            Self::PositiveOverflow => Ok(ConsensusCheck::TriviallyTrue),
            Self::NegativeOverflow => Err(invalid("timelock can never be satisfied")),
        }
    }
}

/// Mirrors `sanitize_uint` in `chia-consensus`. Note that negative values aren't required to be canonical.
fn sanitize_uint(bytes: &[u8], max_size: usize) -> Result<Uint, FromClvmError> {
    if bytes.is_empty() {
        return Ok(Uint::Value(0));
    }

    if bytes[0] & 0x80 != 0 {
        return Ok(Uint::NegativeOverflow);
    }

    // A leading zero is only allowed if it's needed to keep the value positive.
    if bytes == [0] || (bytes.len() > 1 && bytes[0] == 0 && bytes[1] & 0x80 == 0) {
        return Err(invalid("integer is not canonically encoded"));
    }

    let size_limit = if bytes[0] == 0 {
        max_size + 1
    } else {
        max_size
    };

    if bytes.len() > size_limit {
        return Ok(Uint::PositiveOverflow);
    }

    Ok(Uint::Value(
        bytes
            .iter()
            .fold(0, |value, byte| (value << 8) | u64::from(*byte)),
    ))
}

/// Mirrors `fits_in_small_atom` in `clvmr`, which only accepts canonical values that fit in 26 bits.
fn small_number(bytes: &[u8]) -> Option<u32> {
    if !bytes.is_empty()
        && (bytes.len() > 4
            || (bytes.len() == 1 && bytes[0] == 0)
            || bytes[0] & 0x80 != 0
            || (bytes[0] == 0 && bytes[1] & 0x80 == 0)
            || (bytes.len() == 4 && bytes[0] > 0x03))
    {
        return None;
    }

    Some(
        bytes
            .iter()
            .fold(0, |value, byte| (value << 8) | u32::from(*byte)),
    )
}

fn invalid(message: &str) -> FromClvmError {
    FromClvmError::Custom(message.to_string())
}
