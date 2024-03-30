use chia_bls::{sign, DerivableKey, SecretKey, Signature};
use chia_wallet::DeriveSynthetic;

use crate::{KeyStore, RequiredSignature};

/// Responsible for signing messages.
pub trait Signer {
    /// Signs a message with its corresponding public key, if possible.
    fn sign_message(&self, index: u32, message: &[u8]) -> Signature;
}

/// An in-memory implementation of an hardened key signer.
pub struct HardenedMemorySigner {
    intermediate_sk: SecretKey,
    hidden_puzzle_hash: [u8; 32],
}

impl HardenedMemorySigner {
    /// Creates a new `HardenedMemorySigner` with the given intermediate secret key and hidden puzzle hash.
    pub fn new(intermediate_sk: SecretKey, hidden_puzzle_hash: [u8; 32]) -> Self {
        Self {
            intermediate_sk,
            hidden_puzzle_hash,
        }
    }
}

impl Signer for HardenedMemorySigner {
    fn sign_message(&self, index: u32, message: &[u8]) -> Signature {
        let sk = self
            .intermediate_sk
            .derive_hardened(index)
            .derive_synthetic(&self.hidden_puzzle_hash);
        sign(&sk, message)
    }
}

/// An in-memory implementation of an unhardened key signer.
pub struct UnhardenedMemorySigner {
    intermediate_sk: SecretKey,
    hidden_puzzle_hash: [u8; 32],
}

impl UnhardenedMemorySigner {
    /// Creates a new `UnhardenedMemorySigner` with the given intermediate secret key and hidden puzzle hash.
    pub fn new(intermediate_sk: SecretKey, hidden_puzzle_hash: [u8; 32]) -> Self {
        Self {
            intermediate_sk,
            hidden_puzzle_hash,
        }
    }
}

impl Signer for UnhardenedMemorySigner {
    fn sign_message(&self, index: u32, message: &[u8]) -> Signature {
        let sk = self
            .intermediate_sk
            .derive_unhardened(index)
            .derive_synthetic(&self.hidden_puzzle_hash);
        sign(&sk, message)
    }
}

/// Tries to sign each of the messages, replacing the `Vec` with the unsuccessful items.
pub async fn sign_all_with<E>(
    key_store: &mut impl KeyStore<Error = E>,
    signer: &impl Signer,
    required_signatures: &mut Vec<RequiredSignature>,
) -> Result<Signature, E> {
    let mut new_required_signatures = Vec::new();
    let mut aggregate_signature = Signature::default();

    for required_signature in required_signatures.drain(..) {
        let Some(index) = key_store.pk_index(required_signature.public_key()).await? else {
            new_required_signatures.push(required_signature);
            continue;
        };

        aggregate_signature += &signer.sign_message(index, &required_signature.final_message());
    }

    *required_signatures = new_required_signatures;
    Ok(aggregate_signature)
}

#[cfg(test)]
mod tests {
    use chia_bls::derive_keys::master_to_wallet_unhardened_intermediate;
    use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend, Program};
    use chia_wallet::standard::DEFAULT_HIDDEN_PUZZLE_HASH;
    use clvm_traits::{clvm_list, FromNodePtr, ToClvm};
    use clvmr::{Allocator, NodePtr};
    use hex_literal::hex;
    use sqlx::SqlitePool;

    use crate::{sqlite::SqliteKeyStore, testing::SECRET_KEY, Condition};

    use super::*;

    const AGG_SIG_ME: [u8; 32] =
        hex!("ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb");

    fn coin1() -> Coin {
        Coin::new(Bytes32::from([0; 32]), Bytes32::from([1; 32]), 2)
    }

    fn serialize<T>(value: T) -> Program
    where
        T: ToClvm<NodePtr>,
    {
        let mut a = Allocator::new();
        let ptr = value.to_clvm(&mut a).unwrap();
        Program::from_node_ptr(&a, ptr).unwrap()
    }

    async fn sign_coin_spend<E>(
        key_store: &mut impl KeyStore<Error = E>,
        signer: &impl Signer,
        coin_spend: &CoinSpend,
    ) -> Signature
    where
        E: std::fmt::Debug,
    {
        let mut a = Allocator::new();
        let mut required_signatures =
            RequiredSignature::from_coin_spend(&mut a, coin_spend, AGG_SIG_ME).unwrap();

        let signature = sign_all_with(key_store, signer, &mut required_signatures)
            .await
            .unwrap();
        assert!(required_signatures.is_empty());

        signature
    }

    macro_rules! condition {
        ($name:ident, $pk:expr, $msg:expr) => {
            Condition::<NodePtr>::$name {
                public_key: $pk,
                message: $msg,
            }
        };
    }

    #[sqlx::test]
    async fn test_sign_spend(pool: SqlitePool) {
        let root_sk: &SecretKey = &SECRET_KEY;
        let intermediate_sk = master_to_wallet_unhardened_intermediate(root_sk);
        let intermediate_pk = intermediate_sk.public_key();

        let mut conn = pool.acquire().await.unwrap();
        let mut key_store = SqliteKeyStore::new(&mut conn, false);
        let signer = UnhardenedMemorySigner::new(intermediate_sk, DEFAULT_HIDDEN_PUZZLE_HASH);

        key_store
            .extend_keys(
                0,
                &[intermediate_pk
                    .derive_unhardened(0)
                    .derive_synthetic(&DEFAULT_HIDDEN_PUZZLE_HASH)],
            )
            .await
            .unwrap();

        let pk = key_store
            .public_key(0)
            .await
            .unwrap()
            .expect("no public key");
        let msg = Bytes::new(vec![42; 42]);

        macro_rules! test_conditions {
            ( $( $name:ident: $hex:expr ),* ) => { $(
                let coin_spend = CoinSpend::new(
                    coin1(),
                    serialize(1),
                    serialize(clvm_list!(condition!($name, pk.clone(), msg.clone()))),
                );

                let signature = sign_coin_spend(&mut key_store, &signer, &coin_spend).await;

                assert_eq!(hex::encode(signature.to_bytes()), hex::encode($hex));
            )* };
        }

        test_conditions!(
            AggSigParent: hex!(
                "
                8b9a388289f71fe24b5ed21a7169e2046df4e2291e509e6b89b57d03fb51b137
                6c41211a9289050e066419908242365912a909094977dbd9ac209d44db1f1a2b
                340cee3ce572727308911fcfc81d3a7a94711c31ad618bb7f7b4c08367887334
                "
            ),
            AggSigPuzzle: hex!(
                "
                97bc452a7a6439ea281f0ae33cf418173b98fbe782ab2928dbd6097ef5dc58ab
                dd5ef34fe4b4e13d4f5ce4397e5cad6f0072283b53ff03ea4a2d37c3a16c861e
                a1f289e29d7d6fd4552e762347e5bb4590eda4b486974c3e208e0e27138846b5
                "
            ),
            AggSigAmount: hex!(
                "
                b2b7bb2dbe8617b22001d57b5e934378bd98ede324ae464376108aa00c703938
                1d97e39dcc2c96680f8b28c159eb09b50c54a65a7ad981168c98c7b790557e4e
                1d6bfe2881f695a150838d72608586ae93a0311739f361c6bf811518405504d7
                "
            ),
            AggSigPuzzleAmount: hex!(
                "
                b828185c2572a6fc408934c426c11d6452096532d19483bb4c299eaa04bd45a9
                a231ee7b0293c211b0056d7e9862478b0f4e451229ce1432141642e1a2d04708
                2873f77699406ab353d9fa04d11fac2e22420ebd8fd3917793bbb9642f29ef27
                "
            ),
            AggSigParentAmount: hex!(
                "
                a8942f9b1bd2b5ce9a624b3652734a4a8318f774f212bff5f46b4967edf1e3b4
                ec1a759a060f33ec62a15d5d7b162c71172a41f675f19574f28f65cfd6bd73de
                3f6aa0cd73b3fc7889e188f258d554c690b33c9099a39e14c72c293e04118afb
                "
            ),
            AggSigUnsafe: hex!(
                "
                b34d5f4c969d7b0290b1af7de8f71903bd71d6875744b5263bf00b60ab4da6dd
                3c671a0a3b765cb6e5a7b8b9305a59e20274bf6c53b6891e27543b77948edcaa
                5270a4db63e70b8e13d8b1624b3ef5149466c9e99d21959254687e5f7a36de42
                "
            ),
            AggSigMe: hex!(
                "
                89266426a248adee3a699196f00e89a509a177cffb0d2df7fa405bb6a42ffe90
                0450f4788ac34f505991054a0ee0b036013e594a5738199bfe33a37b61496acd
                28d63ac7b3de741b57e822f75ebb3c69df03aa5ef241094386967d6c7805cb63
                "
            )
        );
    }
}
