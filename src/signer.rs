use chia_bls::{sign, Signature};

use chia_protocol::{CoinSpend, SpendBundle};
use clvm_traits::{FromClvm, FromClvmError};
use clvmr::{reduction::EvalErr, Allocator, NodePtr};
use thiserror::Error;

mod required_signature;

pub use required_signature::*;

use crate::{Condition, SecretKeyStore};

/// An error that occurs while trying to sign a coin spend.
#[derive(Debug, Error)]
pub enum SignSpendError {
    /// An error that occurs while trying to calculate the conditions.
    #[error("{0:?}")]
    Eval(#[from] EvalErr),

    /// An error that occurs while attempting to parse the conditions.
    #[error("{0}")]
    Clvm(#[from] FromClvmError),

    /// An error that indicates that a key is missing.
    #[error("missing key")]
    MissingKey,
}

/// Signs each of the required messages in a coin spend.
pub async fn sign_coin_spend(
    sk_store: &impl SecretKeyStore,
    allocator: &mut Allocator,
    coin_spend: &CoinSpend,
    agg_sig_me_extra_data: [u8; 32],
) -> Result<Signature, SignSpendError> {
    let output = coin_spend
        .puzzle_reveal
        .run(allocator, 0, u64::MAX, &coin_spend.solution)?
        .1;

    let conditions: Vec<Condition<NodePtr>> = FromClvm::from_clvm(allocator, output)?;

    let mut aggregate_signature = Signature::default();

    for condition in conditions {
        let Some(required) = RequiredSignature::try_from_condition(
            &coin_spend.coin,
            condition,
            agg_sig_me_extra_data,
        ) else {
            continue;
        };

        let Some(sk) = sk_store.to_secret_key(required.public_key()).await else {
            return Err(SignSpendError::MissingKey);
        };

        aggregate_signature += &sign(&sk, &required.message());
    }

    Ok(aggregate_signature)
}

/// Signs each of the coin spends in a spend bundle.
pub async fn sign_spend_bundle(
    sk_store: &impl SecretKeyStore,
    allocator: &mut Allocator,
    spend_bundle: &SpendBundle,
    agg_sig_me_extra_data: [u8; 32],
) -> Result<Signature, SignSpendError> {
    let mut aggregate_signature = Signature::default();
    for coin_spend in &spend_bundle.coin_spends {
        let signature =
            sign_coin_spend(sk_store, allocator, coin_spend, agg_sig_me_extra_data).await?;
        aggregate_signature += &signature;
    }
    Ok(aggregate_signature)
}

#[cfg(test)]
mod tests {
    use chia_protocol::{Bytes, Bytes32, Coin, Program};
    use clvm_traits::{clvm_list, FromNodePtr, ToClvm};
    use hex_literal::hex;

    use crate::{testing::SECRET_KEY, PublicKeyStore, SkDerivationStore};

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

    macro_rules! condition {
        ($name:ident, $pk:expr, $msg:expr) => {
            Condition::<NodePtr>::$name {
                public_key: $pk,
                message: $msg,
            }
        };
    }

    #[tokio::test]
    async fn test_sign_spend() {
        let sk_store = SkDerivationStore::new(&SECRET_KEY);
        sk_store.derive_to_index(1).await;

        let sk = sk_store.secret_key(0).await.unwrap();
        let pk = sk.public_key();
        let msg = Bytes::new(vec![42; 42]);

        let mut a = Allocator::new();

        macro_rules! test_conditions {
            ( $( $name:ident: $hex:expr ),* ) => { $(
                let coin_spend = CoinSpend::new(
                    coin1(),
                    serialize(1),
                    serialize(clvm_list!(condition!($name, pk.clone(), msg.clone()))),
                );

                let signature = sign_coin_spend(&sk_store, &mut a, &coin_spend, AGG_SIG_ME).await.unwrap();

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
