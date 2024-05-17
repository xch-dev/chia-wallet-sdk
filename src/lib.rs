#![doc = include_str!("../README.md")]

mod address;
mod condition;
mod parser;
mod spends;
mod ssl;
mod wallet;

pub use address::*;
pub use condition::*;
pub use parser::*;
pub use spends::*;
pub use ssl::*;
pub use wallet::*;

#[cfg(test)]
mod test;

/// The `sqlite` module contains the SQLite storage backend.
#[cfg(any(test, feature = "sqlite"))]
pub mod sqlite;

#[cfg(any(test, feature = "sqlite"))]
pub use sqlite::*;

/// Removes the leading zeros from a CLVM atom.
pub fn trim_leading_zeros(mut slice: &[u8]) -> &[u8] {
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }
    slice
}

/// Converts a `usize` to an atom in CLVM format, with leading zeros trimmed.
pub fn usize_to_bytes(num: usize) -> Vec<u8> {
    let bytes: Vec<u8> = num.to_be_bytes().into();
    trim_leading_zeros(bytes.as_slice()).to_vec()
}

/// Converts a `u64` to an atom in CLVM format, with leading zeros trimmed.
pub fn u64_to_bytes(num: u64) -> Vec<u8> {
    let bytes: Vec<u8> = num.to_be_bytes().into();
    trim_leading_zeros(bytes.as_slice()).to_vec()
}

/// Converts a `u16` to an atom in CLVM format, with leading zeros trimmed.
pub fn u16_to_bytes(num: u16) -> Vec<u8> {
    let bytes: Vec<u8> = num.to_be_bytes().into();
    trim_leading_zeros(bytes.as_slice()).to_vec()
}
