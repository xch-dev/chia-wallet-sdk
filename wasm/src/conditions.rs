use chia_sdk_bindings::Memos;
use clvmr::NodePtr;
use js_sys::BigInt;
use paste::paste;
use wasm_bindgen::prelude::*;

use crate::traits::{IntoJsWithClvm, IntoRust};
use crate::{Clvm, Program, PublicKey};

macro_rules! conditions {
    ( $( $condition:ident $( < $( $generic:ty ),* > )? { $alloc:literal $parse:literal $function:ident( $( $name:ident $( $string:literal )?: $ty:ty $( => $remap:ty )? ),* ) }, )* ) => {
        $( #[wasm_bindgen(getter_with_clone)]
        #[derive(Clone)]
        pub struct $condition {
            $( $( #[wasm_bindgen(js_name = $string)] )? pub $name: $ty, )*
        } )*

        $( paste! {
            #[wasm_bindgen]
            impl Clvm {
                #[wasm_bindgen(js_name = $alloc)]
                pub fn $function( &mut self, $( $( #[wasm_bindgen(js_name = $string)] )? $name: $ty ),* ) -> Result<Program, JsError> {
                    $( let $name $( : $remap )? = IntoRust::rust($name)?; )*
                    let ptr = self.0.write().unwrap().encode(
                        chia_sdk_bindings::$condition::new( $( $name ),* )
                    )?;
                    Ok(Program { clvm: self.0.clone(), node_ptr: ptr })
                }
            }

            #[wasm_bindgen]
            impl Program {
                #[wasm_bindgen(js_name = $parse)]
                #[allow(unused)]
                pub fn [< parse_ $function >]( &self ) -> Result<Option<$condition>, JsError> {
                    let Some(condition) =
                        self.clvm.read().unwrap().decode::<
                            chia_sdk_bindings::$condition $( ::< $( $generic ),* > )?
                        >(self.node_ptr).ok() else
                    {
                        return Ok(None);
                    };

                    Ok(Some($condition {
                        $( $name: condition.$name.js_with_clvm(&self.clvm)?, )*
                    }))
                }
            }
        } )*
    };
}

conditions!(
    Remark<NodePtr> {
        "remark" "parseRemark"
        remark(rest: Program => NodePtr)
    },
    AggSigParent {
        "aggSigParent" "parseAggSigParent"
        agg_sig_parent(public_key "publicKey": PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigPuzzle {
        "aggSigPuzzle" "parseAggSigPuzzle"
        agg_sig_puzzle(public_key "publicKey": PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigAmount {
        "aggSigAmount" "parseAggSigAmount"
        agg_sig_amount(public_key "publicKey": PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigPuzzleAmount {
        "aggSigPuzzleAmount" "parseAggSigPuzzleAmount"
        agg_sig_puzzle_amount(public_key "publicKey": PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigParentAmount {
        "aggSigParentAmount" "parseAggSigParentAmount"
        agg_sig_parent_amount(public_key "publicKey": PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigParentPuzzle {
        "aggSigParentPuzzle" "parseAggSigParentPuzzle"
        agg_sig_parent_puzzle(public_key "publicKey": PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigUnsafe {
        "aggSigUnsafe" "parseAggSigUnsafe"
        agg_sig_unsafe(public_key "publicKey": PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    AggSigMe {
        "aggSigMe" "parseAggSigMe"
        agg_sig_me(public_key "publicKey": PublicKey => chia_bls::PublicKey, message: Vec<u8>)
    },
    CreateCoin<NodePtr> {
        "createCoin" "parseCreateCoin"
        create_coin(puzzle_hash "puzzleHash": Vec<u8>, amount: BigInt, memos: Option<Program> => Option<Memos<NodePtr>>)
    },
    ReserveFee {
        "reserveFee" "parseReserveFee"
        reserve_fee(amount: BigInt)
    },
    CreateCoinAnnouncement {
        "createCoinAnnouncement" "parseCreateCoinAnnouncement"
        create_coin_announcement(message: Vec<u8>)
    },
    CreatePuzzleAnnouncement {
        "createPuzzleAnnouncement" "parseCreatePuzzleAnnouncement"
        create_puzzle_announcement(message: Vec<u8>)
    },
    AssertCoinAnnouncement {
        "assertCoinAnnouncement" "parseAssertCoinAnnouncement"
        assert_coin_announcement(announcement_id "announcementId": Vec<u8>)
    },
    AssertPuzzleAnnouncement {
        "assertPuzzleAnnouncement" "parseAssertPuzzleAnnouncement"
        assert_puzzle_announcement(announcement_id "announcementId": Vec<u8>)
    },
    AssertConcurrentSpend {
        "assertConcurrentSpend" "parseAssertConcurrentSpend"
        assert_concurrent_spend(coin_id "coinId": Vec<u8>)
    },
    AssertConcurrentPuzzle {
        "assertConcurrentPuzzle" "parseAssertConcurrentPuzzle"
        assert_concurrent_puzzle(puzzle_hash "puzzleHash": Vec<u8>)
    },
    AssertSecondsRelative {
        "assertSecondsRelative" "parseAssertSecondsRelative"
        assert_seconds_relative(seconds: BigInt)
    },
    AssertSecondsAbsolute {
        "assertSecondsAbsolute" "parseAssertSecondsAbsolute"
        assert_seconds_absolute(seconds: BigInt)
    },
    AssertHeightRelative {
        "assertHeightRelative" "parseAssertHeightRelative"
        assert_height_relative(height: u32)
    },
    AssertHeightAbsolute {
        "assertHeightAbsolute" "parseAssertHeightAbsolute"
        assert_height_absolute(height: u32)
    },
    AssertBeforeSecondsRelative {
        "assertBeforeSecondsRelative" "parseAssertBeforeSecondsRelative"
        assert_before_seconds_relative(seconds: BigInt)
    },
    AssertBeforeSecondsAbsolute {
        "assertBeforeSecondsAbsolute" "parseAssertBeforeSecondsAbsolute"
        assert_before_seconds_absolute(seconds: BigInt)
    },
    AssertBeforeHeightRelative {
        "assertBeforeHeightRelative" "parseAssertBeforeHeightRelative"
        assert_before_height_relative(height: u32)
    },
    AssertBeforeHeightAbsolute {
        "assertBeforeHeightAbsolute" "parseAssertBeforeHeightAbsolute"
        assert_before_height_absolute(height: u32)
    },
    AssertMyCoinId {
        "assertMyCoinId" "parseAssertMyCoinId"
        assert_my_coin_id(coin_id "coinId": Vec<u8>)
    },
    AssertMyParentId {
        "assertMyParentId" "parseAssertMyParentId"
        assert_my_parent_id(parent_id "parentId": Vec<u8>)
    },
    AssertMyPuzzleHash {
        "assertMyPuzzleHash" "parseAssertMyPuzzleHash"
        assert_my_puzzle_hash(puzzle_hash "puzzleHash": Vec<u8>)
    },
    AssertMyAmount {
        "assertMyAmount" "parseAssertMyAmount"
        assert_my_amount(amount: BigInt)
    },
    AssertMyBirthSeconds {
        "assertMyBirthSeconds" "parseAssertMyBirthSeconds"
        assert_my_birth_seconds(seconds: BigInt)
    },
    AssertMyBirthHeight {
        "assertMyBirthHeight" "parseAssertMyBirthHeight"
        assert_my_birth_height(height: u32)
    },
    AssertEphemeral {
        "assertEphemeral" "parseAssertEphemeral"
        assert_ephemeral()
    },
    SendMessage<NodePtr> {
        "sendMessage" "parseSendMessage"
        send_message(mode: u8, message: Vec<u8>, data: Vec<Program> => Vec<NodePtr>)
    },
    ReceiveMessage<NodePtr> {
        "receiveMessage" "parseReceiveMessage"
        receive_message(mode: u8, message: Vec<u8>, data: Vec<Program> => Vec<NodePtr>)
    },
    Softfork<NodePtr> {
        "softfork" "parseSoftfork"
        softfork(cost: BigInt, rest: Program => NodePtr)
    },
);
