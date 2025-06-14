use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_driver::SpendContext;
use chia_sdk_types::conditions::{self, Memos, TradePrice};
use clvm_traits::{FromClvm, ToClvm};
use clvmr::NodePtr;
use paste::paste;

use crate::{Clvm, Program};

trait Convert<T> {
    fn convert(self, clvm: &Arc<Mutex<SpendContext>>) -> Result<T>;
}

impl Convert<Program> for NodePtr {
    fn convert(self, clvm: &Arc<Mutex<SpendContext>>) -> Result<Program> {
        Ok(Program(clvm.clone(), self))
    }
}

impl Convert<NodePtr> for Program {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<NodePtr> {
        Ok(self.1)
    }
}

impl Convert<PublicKey> for PublicKey {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<PublicKey> {
        Ok(self)
    }
}

impl Convert<Bytes> for Bytes {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<Bytes> {
        Ok(self)
    }
}

impl Convert<Bytes32> for Bytes32 {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<Bytes32> {
        Ok(self)
    }
}

impl Convert<u64> for u64 {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<u64> {
        Ok(self)
    }
}

impl Convert<u32> for u32 {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<u32> {
        Ok(self)
    }
}

impl Convert<u8> for u8 {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<u8> {
        Ok(self)
    }
}

impl Convert<TradePrice> for TradePrice {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<TradePrice> {
        Ok(self)
    }
}

impl Convert<Memos<NodePtr>> for Option<Program> {
    fn convert(self, _clvm: &Arc<Mutex<SpendContext>>) -> Result<Memos<NodePtr>> {
        Ok(self.map_or(Memos::None, |program| Memos::Some(program.1)))
    }
}

impl Convert<Option<Program>> for Memos<NodePtr> {
    fn convert(self, clvm: &Arc<Mutex<SpendContext>>) -> Result<Option<Program>> {
        Ok(match self {
            Memos::None => None,
            Memos::Some(value) => Some(Program(clvm.clone(), value)),
        })
    }
}

impl<T, U> Convert<Vec<U>> for Vec<T>
where
    T: Convert<U>,
{
    fn convert(self, clvm: &Arc<Mutex<SpendContext>>) -> Result<Vec<U>> {
        self.into_iter()
            .map(|value| T::convert(value, clvm))
            .collect()
    }
}

impl<T, U> Convert<Option<U>> for Option<T>
where
    T: Convert<U>,
{
    fn convert(self, clvm: &Arc<Mutex<SpendContext>>) -> Result<Option<U>> {
        self.map(|value| T::convert(value, clvm)).transpose()
    }
}

macro_rules! conditions {
    ( $( $condition:ident $( < $( $generic:ty ),* > )? { $function:ident( $( $name:ident: $ty:ty ),* ) }, )* ) => {
        $( #[derive(Clone)]
        pub struct $condition {
            $( pub $name: $ty, )*
        } )*

        $( paste! {
            impl Clvm {
                pub fn $function( &self, $( $name: $ty ),* ) -> Result<Program> {
                    let mut ctx = self.0.lock().unwrap();
                    $( let $name = Convert::convert($name, &self.0)?; )*
                    let ptr = conditions::$condition $( ::< $( $generic ),* > )? ::new( $( $name ),* )
                    .to_clvm(&mut **ctx)?;
                    Ok(Program(self.0.clone(), ptr))
                }
            }

            impl Program {
                #[allow(unused)]
                pub fn [< parse_ $function >]( &self ) -> Result<Option<$condition>> {
                    let ctx = self.0.lock().unwrap();

                    let Some(condition) = conditions::$condition $( ::< $( $generic ),* > )? ::from_clvm(&**ctx, self.1).ok() else {
                        return Ok(None);
                    };

                    Ok(Some($condition {
                        $( $name: Convert::convert(condition.$name, &self.0.clone())?, )*
                    }))
                }
            }
        } )*
    };
}

conditions!(
    Remark<NodePtr> {
        remark(rest: Program)
    },
    AggSigParent {
        agg_sig_parent(public_key: PublicKey, message: Bytes)
    },
    AggSigPuzzle {
        agg_sig_puzzle(public_key: PublicKey, message: Bytes)
    },
    AggSigAmount {
        agg_sig_amount(public_key: PublicKey, message: Bytes)
    },
    AggSigPuzzleAmount {
        agg_sig_puzzle_amount(public_key: PublicKey, message: Bytes)
    },
    AggSigParentAmount {
        agg_sig_parent_amount(public_key: PublicKey, message: Bytes)
    },
    AggSigParentPuzzle {
        agg_sig_parent_puzzle(public_key: PublicKey, message: Bytes)
    },
    AggSigUnsafe {
        agg_sig_unsafe(public_key: PublicKey, message: Bytes)
    },
    AggSigMe {
        agg_sig_me(public_key: PublicKey, message: Bytes)
    },
    CreateCoin {
        create_coin(puzzle_hash: Bytes32, amount: u64, memos: Option<Program>)
    },
    ReserveFee {
        reserve_fee(amount: u64)
    },
    CreateCoinAnnouncement {
        create_coin_announcement(message: Bytes)
    },
    CreatePuzzleAnnouncement {
        create_puzzle_announcement(message: Bytes)
    },
    AssertCoinAnnouncement {
        assert_coin_announcement(announcement_id: Bytes32)
    },
    AssertPuzzleAnnouncement {
        assert_puzzle_announcement(announcement_id: Bytes32)
    },
    AssertConcurrentSpend {
        assert_concurrent_spend(coin_id: Bytes32)
    },
    AssertConcurrentPuzzle {
        assert_concurrent_puzzle(puzzle_hash: Bytes32)
    },
    AssertSecondsRelative {
        assert_seconds_relative(seconds: u64)
    },
    AssertSecondsAbsolute {
        assert_seconds_absolute(seconds: u64)
    },
    AssertHeightRelative {
        assert_height_relative(height: u32)
    },
    AssertHeightAbsolute {
        assert_height_absolute(height: u32)
    },
    AssertBeforeSecondsRelative {
        assert_before_seconds_relative(seconds: u64)
    },
    AssertBeforeSecondsAbsolute {
        assert_before_seconds_absolute(seconds: u64)
    },
    AssertBeforeHeightRelative {
        assert_before_height_relative(height: u32)
    },
    AssertBeforeHeightAbsolute {
        assert_before_height_absolute(height: u32)
    },
    AssertMyCoinId {
        assert_my_coin_id(coin_id: Bytes32)
    },
    AssertMyParentId {
        assert_my_parent_id(parent_id: Bytes32)
    },
    AssertMyPuzzleHash {
        assert_my_puzzle_hash(puzzle_hash: Bytes32)
    },
    AssertMyAmount {
        assert_my_amount(amount: u64)
    },
    AssertMyBirthSeconds {
        assert_my_birth_seconds(seconds: u64)
    },
    AssertMyBirthHeight {
        assert_my_birth_height(height: u32)
    },
    AssertEphemeral {
        assert_ephemeral()
    },
    SendMessage<NodePtr> {
        send_message(mode: u8, message: Bytes, data: Vec<Program>)
    },
    ReceiveMessage<NodePtr> {
        receive_message(mode: u8, message: Bytes, data: Vec<Program>)
    },
    Softfork<NodePtr> {
        softfork(cost: u64, rest: Program)
    },
    MeltSingleton {
        melt_singleton()
    },
    TransferNft {
        transfer_nft(launcher_id: Option<Bytes32>, trade_prices: Vec<TradePrice>, singleton_inner_puzzle_hash: Option<Bytes32>)
    },
    RunCatTail<NodePtr, NodePtr> {
        run_cat_tail(program: Program, solution: Program)
    },
    UpdateNftMetadata<NodePtr, NodePtr> {
        update_nft_metadata(updater_puzzle_reveal: Program, updater_solution: Program)
    },
    UpdateDataStoreMerkleRoot {
        update_data_store_merkle_root(new_merkle_root: Bytes32, memos: Vec<Bytes>)
    },
);
