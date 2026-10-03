// AVA tests for silent-payment address round-trip and
// Action.silentPaymentSend construction smoke through the napi facade.
//
// Asserts on `PublicKey.toBytes()` byte-equality, NOT on the encoded bech32m
// string — this keeps the test robust against future bech32m library changes
// or canonical-form normalizations that might re-shape the textual address
// without changing the underlying key material.
//
// Mnemonic fixture is the BIP-39 standard test vector — also used by the
// Rust-side `from_mnemonic_tv1_scan_pk_matches` test at
// crates/chia-sdk-utils/src/silent_payments/keys.rs:152 — so this AVA test
// transitively pins the same CHIP TV1 bytes that the Rust test does.

import test from "ava";
import {
  Action,
  fromHex,
  Mnemonic,
  SecretKey,
  SilentPaymentAddress,
  SilentPaymentKeys,
  SilentPaymentNetwork,
  SilentPayments,
  toHex,
} from "..";

const TV1_MNEMONIC =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

// SC2 — address round-trip via byte-equality on scan_pk/spend_pk
test("silent-payment address round-trip (TV1 mainnet)", (t) => {
  const mnemonic = new Mnemonic(TV1_MNEMONIC);
  const keys = SilentPaymentKeys.fromMnemonic(mnemonic);
  const address = keys.unlabeledAddress(SilentPaymentNetwork.Mainnet);
  const encoded = address.encode();
  const decoded = SilentPaymentAddress.decode(encoded);

  // Byte-equality on the scan and spend public keys — survives bech32m library churn.
  t.deepEqual(decoded.scanPk.toBytes(), keys.scanPk().toBytes());
  t.deepEqual(decoded.spendPk.toBytes(), keys.spendPk().toBytes());
  // Network round-trips correctly.
  t.is(decoded.network, SilentPaymentNetwork.Mainnet);
});

// SC2 supplementary — testnet HRP discriminator round-trips correctly
test("silent-payment address round-trip (TV1 testnet)", (t) => {
  const mnemonic = new Mnemonic(TV1_MNEMONIC);
  const keys = SilentPaymentKeys.fromMnemonic(mnemonic);
  const address = keys.unlabeledAddress(SilentPaymentNetwork.Testnet);
  const encoded = address.encode();
  // Testnet HRP confirmed in the encoded string (the only point in this test
  // where we touch the bech32m output — checked as a string-startsWith, not a
  // byte-pin, so future encoder changes won't false-positive).
  t.true(encoded.startsWith("tspxch1"));
  const decoded = SilentPaymentAddress.decode(encoded);
  t.is(decoded.network, SilentPaymentNetwork.Testnet);
  t.deepEqual(decoded.scanPk.toBytes(), keys.scanPk().toBytes());
});

// SC3 — Action.silentPaymentSend TS construction smoke (the dedicated SP-send
// surface; replaces the old opaque-handle destination class).
test("Action.silentPaymentSend composes from a SilentPaymentAddress (SC3)", (t) => {
  const mnemonic = new Mnemonic(TV1_MNEMONIC);
  const keys = SilentPaymentKeys.fromMnemonic(mnemonic);
  const address = keys.unlabeledAddress(SilentPaymentNetwork.Mainnet);

  // Action.silentPaymentSend takes the recipient address directly — no
  // destination wrapper. We don't execute the spend here; just confirm
  // construction succeeds without throwing, which proves the bindy descriptor
  // wiring for silent_payment_send is intact end-to-end.
  const action = Action.silentPaymentSend(address, 1000n, undefined);
  t.truthy(action);
});

// CHIP-0057 test vectors 1-7 treat the recipient keys as given values.
const TV1_SCAN_SK =
  "132567e4dec19a4f50d9e9a549f16283dfb5aa4ad1ffdb6a505fcfcc56a690f6";
const TV1_SPEND_SK =
  "53d140b312a0e16316314274eb6398e15706d100fe8a754990540febd931b087";

function tv1Keys(): SilentPaymentKeys {
  return SilentPaymentKeys.fromSecretKeys(
    SecretKey.fromBytes(fromHex(TV1_SCAN_SK)),
    SecretKey.fromBytes(fromHex(TV1_SPEND_SK)),
  );
}

// Label generation and the change address, against CHIP-0057 TV3 and TV7.
test("label generation and change address (TV3, TV7)", (t) => {
  const keys = tv1Keys();

  // TV3: label m = 1.
  const label = SilentPayments.generateLabel(keys.scanSk(), 1);
  t.is(
    toHex(label.scalar.toBytes()),
    "48fa440acca87f501b9984b5d23327d0b7766a4baa913dfb3001d412c48ce465",
  );
  t.is(
    toHex(label.publicKey.toBytes()),
    "a6dcff3646739745ef7f3ba8e51808dac13765fa9d5e73386d3fbd7841e0773e02a0f8d91baf57d337954322bd06d80c",
  );
  t.is(
    toHex(
      keys.labeledAddress(SilentPaymentNetwork.Mainnet, 1).spendPk.toBytes(),
    ),
    "965250fb8503cff4c244f360ab84075bfe2da01091745d0e8ce36024ab12e96277d1f02fbbe01cee412dd2ce1b7414c2",
  );

  // TV7: the change label m = 0.
  const changeLabel = SilentPayments.generateLabel(keys.scanSk(), 0);
  t.is(
    toHex(changeLabel.scalar.toBytes()),
    "3106829938a8b73a652a9a31c6c76a37e32f67f924a50e3649291d6904f22082",
  );
  const change = keys.changeAddress(SilentPaymentNetwork.Mainnet);
  t.deepEqual(change.scanPk.toBytes(), keys.scanPk().toBytes());
  t.is(
    toHex(change.spendPk.toBytes()),
    "a2c089434a6abae657b8e3a868f3d1b94299b141f3da6a6788f966b0856d68022bd58459d964b7514111529647ebd7e8",
  );

  // The change address is only available through changeAddress.
  t.throws(() => keys.labeledAddress(SilentPaymentNetwork.Mainnet, 0), {
    message: /reserved for change/,
  });
});
