//! Silent payments (CHIP-0057) — wallet-side cryptographic primitives.
//!
//! All scalar-field reduction in this module flows through [`ScalarField`], which
//! enforces the unsigned-vs-signed byte-interpretation choice at the type level.
//! See [`ScalarField::from_bytes_unsigned`] for why unsigned reduction is mandatory
//! for protocol scalars and must not be swapped for the standard-puzzle reducer.

mod paths;
pub use paths::*;
mod scalar;
pub use scalar::*;
mod tagged_hash;
pub use tagged_hash::*;
