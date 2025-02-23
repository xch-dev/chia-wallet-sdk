use chia_sdk_bindings::Memos;
use clvmr::NodePtr;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use paste::paste;

use crate::traits::{IntoJsWithClvm, IntoRust};
use crate::{Clvm, Program, PublicKey};

macro_rules! conditions {
    ( $( $condition:ident $( < $( $generic:ty ),* > )? { $hint:literal $function:ident( $( $name:ident: $ty:ty $( => $remap:ty )? ),* ) }, )* ) => {
        $( #[napi(object)]
        pub struct $condition {
            $( pub $name: $ty, )*
        } )*

        $( paste! {
            #[napi]
            impl Clvm {
                #[napi]
                pub fn $function( &mut self, env: Env, this: This<'_>, $( $name: $ty ),* ) -> Result<Program> {
                    let clvm = crate::clvm(env, this)?;

                    $( let $name $( : $remap )? = IntoRust::rust($name)?; )*
                    let ptr = self.0.encode(
                        chia_sdk_bindings::$condition::new( $( $name ),* )
                    )?;
                    Ok(Program::new(clvm, ptr))
                }
            }

            #[napi]
            impl Program {
                #[napi]
                #[allow(unused)]
                pub fn [< parse_ $function >]( &self, env: Env ) -> Result<Option<$condition>> {
                    let Some(condition) =
                        self.clvm.0.decode::<
                            chia_sdk_bindings::$condition $( ::< $( $generic ),* > )?
                        >(self.node_ptr).ok() else
                    {
                        return Ok(None);
                    };

                    Ok(Some($condition {
                        $( $name: condition.$name.js_with_clvm(env, &self.clvm)?, )*
                    }))
                }
            }
        } )*
    };
}

conditions!(
    Remark<NodePtr> {
        "rest: Program"
        remark(rest: Reference<Program> => NodePtr)
    },
    AggSigParent {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_parent(public_key: Reference<PublicKey> => chia_bls::PublicKey, message: Uint8Array)
    },
    AggSigPuzzle {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_puzzle(public_key: Reference<PublicKey> => chia_bls::PublicKey, message: Uint8Array)
    },
    AggSigAmount {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_amount(public_key: Reference<PublicKey> => chia_bls::PublicKey, message: Uint8Array)
    },
    AggSigPuzzleAmount {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_puzzle_amount(public_key: Reference<PublicKey> => chia_bls::PublicKey, message: Uint8Array)
    },
    AggSigParentAmount {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_parent_amount(public_key: Reference<PublicKey> => chia_bls::PublicKey, message: Uint8Array)
    },
    AggSigParentPuzzle {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_parent_puzzle(public_key: Reference<PublicKey> => chia_bls::PublicKey, message: Uint8Array)
    },
    AggSigUnsafe {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_unsafe(public_key: Reference<PublicKey> => chia_bls::PublicKey, message: Uint8Array)
    },
    AggSigMe {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_me(public_key: Reference<PublicKey> => chia_bls::PublicKey, message: Uint8Array)
    },
    CreateCoin<NodePtr> {
        "puzzleHash: Uint8Array, amount: bigint, memos: Program | null"
        create_coin(puzzle_hash: Uint8Array, amount: BigInt, memos: Option<Reference<Program>> => Option<Memos<NodePtr>>)
    },
    ReserveFee {
        "amount: bigint"
        reserve_fee(amount: BigInt)
    },
    CreateCoinAnnouncement {
        "message: Uint8Array"
        create_coin_announcement(message: Uint8Array)
    },
    CreatePuzzleAnnouncement {
        "message: Uint8Array"
        create_puzzle_announcement(message: Uint8Array)
    },
    AssertCoinAnnouncement {
        "announcementId: Uint8Array"
        assert_coin_announcement(announcement_id: Uint8Array)
    },
    AssertPuzzleAnnouncement {
        "announcementId: Uint8Array"
        assert_puzzle_announcement(announcement_id: Uint8Array)
    },
    AssertConcurrentSpend {
        "coinId: Uint8Array"
        assert_concurrent_spend(coin_id: Uint8Array)
    },
    AssertConcurrentPuzzle {
        "puzzleHash: Uint8Array"
        assert_concurrent_puzzle(puzzle_hash: Uint8Array)
    },
    AssertSecondsRelative {
        "seconds: bigint"
        assert_seconds_relative(seconds: BigInt)
    },
    AssertSecondsAbsolute {
        "seconds: bigint"
        assert_seconds_absolute(seconds: BigInt)
    },
    AssertHeightRelative {
        "height: number"
        assert_height_relative(height: u32)
    },
    AssertHeightAbsolute {
        "height: number"
        assert_height_absolute(height: u32)
    },
    AssertBeforeSecondsRelative {
        "seconds: bigint"
        assert_before_seconds_relative(seconds: BigInt)
    },
    AssertBeforeSecondsAbsolute {
        "seconds: bigint"
        assert_before_seconds_absolute(seconds: BigInt)
    },
    AssertBeforeHeightRelative {
        "height: number"
        assert_before_height_relative(height: u32)
    },
    AssertBeforeHeightAbsolute {
        "height: number"
        assert_before_height_absolute(height: u32)
    },
    AssertMyCoinId {
        "coinId: Uint8Array"
        assert_my_coin_id(coin_id: Uint8Array)
    },
    AssertMyParentId {
        "parentId: Uint8Array"
        assert_my_parent_id(parent_id: Uint8Array)
    },
    AssertMyPuzzleHash {
        "puzzleHash: Uint8Array"
        assert_my_puzzle_hash(puzzle_hash: Uint8Array)
    },
    AssertMyAmount {
        "amount: bigint"
        assert_my_amount(amount: BigInt)
    },
    AssertMyBirthSeconds {
        "seconds: bigint"
        assert_my_birth_seconds(seconds: BigInt)
    },
    AssertMyBirthHeight {
        "height: number"
        assert_my_birth_height(height: u32)
    },
    AssertEphemeral {
        ""
        assert_ephemeral()
    },
    SendMessage<NodePtr> {
        "mode: number, message: Uint8Array, data: Array<Program>"
        send_message(mode: u8, message: Uint8Array, data: Vec<Reference<Program>> => Vec<NodePtr>)
    },
    ReceiveMessage<NodePtr> {
        "mode: number, message: Uint8Array, data: Array<Program>"
        receive_message(mode: u8, message: Uint8Array, data: Vec<Reference<Program>> => Vec<NodePtr>)
    },
    Softfork<NodePtr> {
        "cost: bigint, rest: Program"
        softfork(cost: BigInt, rest: Reference<Program> => NodePtr)
    },
);
