use chia_sdk_bindings::Memos;
use clvmr::NodePtr;
use paste::paste;
use pyo3::prelude::*;

use crate::traits::{IntoPyWithClvm, IntoRust};
use crate::{Clvm, Program, PublicKey};

macro_rules! conditions {
    ( $( $condition:ident $( < $( $generic:ty ),* > )? { $( #[pyo3(signature = ( $($signature:tt)* ))] )? $function:ident( $( $name:ident: $ty:ty $( => $remap:ty )? ),* ) }, )* ) => {
        $( #[pyclass(get_all, frozen)]
        #[derive(Clone)]
        pub struct $condition {
            $( pub $name: $ty, )*
        } )*

        $( paste! {
            #[pymethods]
            impl Clvm {
                $( #[pyo3(signature = ( $($signature)* ))] )?
                pub fn $function( &mut self, $( $name: $ty ),* ) -> PyResult<Program> {
                    $( let $name $( : $remap )? = IntoRust::rust($name)?; )*
                    let ptr = self.0.write().encode(
                        chia_sdk_bindings::$condition::new( $( $name ),* )
                    )?;
                    Ok(Program { clvm: self.0.clone(), node_ptr: ptr })
                }
            }

            #[pymethods]
            impl Program {
                #[allow(unused)]
                pub fn [< parse_ $function >]( &self ) -> PyResult<Option<$condition>> {
                    let Some(condition) =
                        self.clvm.read().decode::<
                            chia_sdk_bindings::$condition $( ::< $( $generic ),* > )?
                        >(self.node_ptr).ok() else
                    {
                        return Ok(None);
                    };

                    Ok(Some($condition {
                        $( $name: condition.$name.py_with_clvm(&self.clvm)?, )*
                    }))
                }
            }
        } )*

        pub fn add_conditions_to_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
            $( m.add_class::<$condition>()?; )*
            Ok(())
        }
    };
}

conditions!(
    Remark<NodePtr> {
        remark(rest: Program => NodePtr)
    },
    AggSigParent {
        agg_sig_parent(public_key: PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigPuzzle {
        agg_sig_puzzle(public_key: PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigAmount {
        agg_sig_amount(public_key: PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigPuzzleAmount {
        agg_sig_puzzle_amount(public_key: PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigParentAmount {
        agg_sig_parent_amount(public_key: PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigParentPuzzle {
        agg_sig_parent_puzzle(public_key: PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigUnsafe {
        agg_sig_unsafe(public_key: PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigMe {
        agg_sig_me(public_key: PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    CreateCoin<NodePtr> {
        #[pyo3(signature = (puzzle_hash, amount, memos=None))]
        create_coin(puzzle_hash: Vec<u8>, amount: u64, memos: Option<Program> => Option<Memos<NodePtr>>)
    },
    ReserveFee {
        reserve_fee(amount: u64)
    },
    CreateCoinAnnouncement {
        create_coin_announcement(message: Vec<u8>)
    },
    CreatePuzzleAnnouncement {
        create_puzzle_announcement(message: Vec<u8>)
    },
    AssertCoinAnnouncement {
        assert_coin_announcement(announcement_id: Vec<u8>)
    },
    AssertPuzzleAnnouncement {
        assert_puzzle_announcement(announcement_id: Vec<u8>)
    },
    AssertConcurrentSpend {
        assert_concurrent_spend(coin_id: Vec<u8>)
    },
    AssertConcurrentPuzzle {
        assert_concurrent_puzzle(puzzle_hash: Vec<u8>)
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
        assert_my_coin_id(coin_id: Vec<u8>)
    },
    AssertMyParentId {
        assert_my_parent_id(parent_id: Vec<u8>)
    },
    AssertMyPuzzleHash {
        assert_my_puzzle_hash(puzzle_hash: Vec<u8>)
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
        send_message(mode: u8, message: Vec<u8>, data: Vec<Program> => Vec<NodePtr>)
    },
    ReceiveMessage<NodePtr> {
        receive_message(mode: u8, message: Vec<u8>, data: Vec<Program> => Vec<NodePtr>)
    },
    Softfork<NodePtr> {
        softfork(cost: u64, rest: Program => NodePtr)
    },
);
