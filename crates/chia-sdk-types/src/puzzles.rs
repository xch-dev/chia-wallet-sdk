mod augmented_condition;
mod mips;
mod mods;
mod option_contract;
mod p2_curried;
mod p2_delegated_conditions;
mod p2_one_of_many;
mod p2_singleton;
mod revocation;

pub use augmented_condition::*;
pub use mips::*;
pub use mods::*;
pub use option_contract::*;
pub use p2_curried::*;
pub use p2_delegated_conditions::*;
pub use p2_one_of_many::*;
pub use p2_singleton::*;
pub use revocation::*;

#[cfg(feature = "chip-0035")]
mod datalayer;

#[cfg(feature = "chip-0035")]
pub use datalayer::*;

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
