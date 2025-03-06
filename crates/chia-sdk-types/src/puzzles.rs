mod augmented_condition;
mod mods;
mod p2_curried;
mod p2_delegated_conditions;
mod p2_one_of_many;
mod p2_singleton;
mod revocation;

pub use augmented_condition::*;
pub use mods::*;
pub use p2_curried::*;
pub use p2_delegated_conditions::*;
pub use p2_one_of_many::*;
pub use p2_singleton::*;
pub use revocation::*;

#[cfg(feature = "chip-0035")]
mod datalayer;

#[cfg(feature = "chip-0035")]
pub use datalayer::*;

#[cfg(feature = "experimental-vaults")]
mod mips;

#[cfg(feature = "experimental-vaults")]
pub use mips::*;

#[cfg(test)]
mod tests {
    #[macro_export]
    macro_rules! assert_puzzle_hash {
        ($puzzle:ident => $puzzle_hash:ident) => {
            let mut a = clvmr::Allocator::new();
            let ptr = clvmr::serde::node_from_bytes(&mut a, &$puzzle)?;
            let hash = clvm_utils::tree_hash(&mut a, ptr);
            assert_eq!($puzzle_hash, hash);
        };
    }
}
