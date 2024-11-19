mod cat_layer;
mod did_layer;
mod nft_ownership_layer;
mod nft_state_layer;
mod p2_controller_puzzle_layer;
mod p2_delegated_conditions_layer;
mod p2_delegated_singleton_layer;
mod p2_eip712_message_layer;
mod p2_one_of_many;
mod p2_singleton;
mod royalty_transfer_layer;
mod settlement_layer;
mod singleton_layer;
mod standard_layer;

pub use cat_layer::*;
pub use did_layer::*;
pub use nft_ownership_layer::*;
pub use nft_state_layer::*;
pub use p2_controller_puzzle_layer::*;
pub use p2_delegated_conditions_layer::*;
pub use p2_delegated_singleton_layer::*;
pub use p2_eip712_message_layer::*;
pub use p2_one_of_many::*;
pub use p2_singleton::*;
pub use royalty_transfer_layer::*;
pub use settlement_layer::*;
pub use singleton_layer::*;
pub use standard_layer::*;

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
