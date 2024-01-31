use chia_bls::PublicKey;
use chia_protocol::{Coin, CoinSpend, Program};
use chia_wallet::standard::{StandardArgs, StandardSolution};
use clvm_traits::{clvm_quote, ToClvmError};
use clvm_utils::CurriedProgram;
use clvmr::{allocator::NodePtr, Allocator, FromNodePtr, ToNodePtr};

use crate::{Condition, DerivationStore};

/// Creates a new coin spend for a given standard transaction coin.
pub fn spend_standard_coin(
    a: &mut Allocator,
    standard_puzzle_ptr: NodePtr,
    coin: Coin,
    synthetic_key: PublicKey,
    conditions: &[Condition<NodePtr>],
) -> Result<CoinSpend, ToClvmError> {
    let puzzle = CurriedProgram {
        program: standard_puzzle_ptr,
        args: StandardArgs { synthetic_key },
    }
    .to_node_ptr(a)?;

    let solution = StandardSolution {
        original_public_key: None,
        delegated_puzzle: clvm_quote!(conditions),
        solution: (),
    }
    .to_node_ptr(a)?;

    let puzzle = Program::from_node_ptr(a, puzzle).unwrap();
    let solution = Program::from_node_ptr(a, solution).unwrap();

    Ok(CoinSpend::new(coin, puzzle, solution))
}

/// Spends a list of standard transaction coins.
pub async fn spend_standard_coins(
    a: &mut Allocator,
    standard_puzzle_ptr: NodePtr,
    derivation_store: &impl DerivationStore,
    coins: Vec<Coin>,
    conditions: &[Condition<NodePtr>],
) -> Vec<CoinSpend> {
    let mut coin_spends = Vec::new();
    for (i, coin) in coins.into_iter().enumerate() {
        let puzzle_hash = &coin.puzzle_hash;
        let index = derivation_store
            .index_of_ph(puzzle_hash.into())
            .await
            .expect("cannot spend coin with unknown puzzle hash");

        let synthetic_key = derivation_store
            .public_key(index)
            .await
            .expect("cannot spend coin with unknown public key");

        coin_spends.push(
            spend_standard_coin(
                a,
                standard_puzzle_ptr,
                coin,
                synthetic_key,
                if i == 0 { conditions } else { &[] },
            )
            .unwrap(),
        );
    }
    coin_spends
}

#[cfg(test)]
mod tests {
    use chia_bls::{derive_keys::master_to_wallet_unhardened, SecretKey};
    use chia_protocol::Bytes32;
    use chia_wallet::{
        standard::{DEFAULT_HIDDEN_PUZZLE_HASH, STANDARD_PUZZLE},
        DeriveSynthetic,
    };
    use clvmr::serde::{node_from_bytes, node_to_bytes};
    use hex_literal::hex;

    use crate::{testing::SEED, CreateCoin};

    use super::*;

    // Calculates a synthetic key at the given derivation index, using the test seed.
    fn synthetic_key(index: u32) -> PublicKey {
        let sk = SecretKey::from_seed(SEED.as_ref());
        let pk = sk.public_key();
        let child_key = master_to_wallet_unhardened(&pk, index);
        child_key.derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)
    }

    #[test]
    fn test_standard_spend() {
        let a = &mut Allocator::new();
        let standard_puzzle_ptr = node_from_bytes(a, &STANDARD_PUZZLE).unwrap();
        let coin = Coin::new(Bytes32::from([0; 32]), Bytes32::from([1; 32]), 42);
        let synthetic_key = synthetic_key(0);

        let conditions = vec![Condition::CreateCoin(CreateCoin::Normal {
            puzzle_hash: coin.puzzle_hash,
            amount: coin.amount,
        })];

        let coin_spend =
            spend_standard_coin(a, standard_puzzle_ptr, coin, synthetic_key, &conditions).unwrap();
        let output_ptr = coin_spend
            .puzzle_reveal
            .run(a, 0, u64::MAX, &coin_spend.solution)
            .unwrap()
            .1;
        let actual = node_to_bytes(a, output_ptr).unwrap();

        let expected = hex!(
            "
            ffff32ffb08584adae5630842a1766bc444d2b872dd3080f4e5daaecf6f762a4
            be7dc148f37868149d4217f3dcc9183fe61e48d8bfffa09744e53c76d9ce3c6b
            eb75a3d414ebbec42e31e96621c66b7a832ca1feccceea80ffff33ffa0010101
            0101010101010101010101010101010101010101010101010101010101ff2a80
            80
            "
        );
        assert_eq!(hex::encode(actual), hex::encode(expected));
    }
}
