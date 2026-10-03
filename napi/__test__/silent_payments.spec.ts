// AVA tests for silent-payment address round-trip and
// Action.silentPaymentSend construction smoke through the napi facade.
//
// Asserts on `PublicKey.toBytes()` byte-equality, NOT on the encoded bech32m
// string — this keeps the test robust against future bech32m library changes
// or canonical-form normalizations that might re-shape the textual address
// without changing the underlying key material.
//
// The mnemonic fixture is the BIP-39 standard test vector, whose hardened
// derivation is pinned by CHIP-0057 Test Vector 8 (see the TV8 test below).

import test from "ava";
import {
  Action,
  Clvm,
  fromHex,
  Mnemonic,
  PublicKey,
  SecretKey,
  SilentPaymentAddress,
  SilentPaymentKeys,
  SilentPaymentNetwork,
  SilentPayments,
  Simulator,
  Spends,
  toHex,
} from "..";

const TEST_MNEMONIC =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

// SC2 — address round-trip via byte-equality on scan_pk/spend_pk
test("silent-payment address round-trip (TV1 mainnet)", (t) => {
  const mnemonic = new Mnemonic(TEST_MNEMONIC);
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
  const mnemonic = new Mnemonic(TEST_MNEMONIC);
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
  const mnemonic = new Mnemonic(TEST_MNEMONIC);
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

// CHIP-0057 Test Vector 8: hardened derivation at m/12381n/8444n/12n/0n (scan)
// and m/12381n/8444n/13n/0n (spend) from the BIP-39 test mnemonic.
test("key derivation from the mnemonic is hardened (TV8)", (t) => {
  const keys = SilentPaymentKeys.fromMnemonic(new Mnemonic(TEST_MNEMONIC));
  t.is(
    toHex(keys.scanSk().toBytes()),
    "0c474f92e8945069c200bb09302d1e569a9b52f59cc04a27874b1bca2adeca9f",
  );
  t.is(
    toHex(keys.spendSk().toBytes()),
    "4f8acf271744cf7050197623569e1603b0193f84b958b5d5f1ce8fd18c908e7b",
  );
  t.is(
    keys.unlabeledAddress(SilentPaymentNetwork.Mainnet).encode(),
    "spxch1q30etue85q8xvzrf5j4gr4j09ke6u9c4s3vrnj9unt0hdj5dhpwxv6q0kp3qxcnh8u7fr0chtttlantgv0dj6xwftvfuzhrq8sjcsce9s7nwglsk5d5knclqwrwyehuvr7a5evgndm7g527yadv9lxjvjryvvu9l2",
  );
  t.is(
    keys.unlabeledAddress(SilentPaymentNetwork.Testnet).encode(),
    "tspxch1q30etue85q8xvzrf5j4gr4j09ke6u9c4s3vrnj9unt0hdj5dhpwxv6q0kp3qxcnh8u7fr0chtttlantgv0dj6xwftvfuzhrq8sjcsce9s7nwglsk5d5knclqwrwyehuvr7a5evgndm7g527yadv9lxjvjrycwanlf",
  );
});

// CHIP-0057 Test Vector 5: addresses of the given TV1 keys.
test("addresses of the given TV1 keys (TV5)", (t) => {
  const keys = tv1Keys();
  t.is(
    keys.unlabeledAddress(SilentPaymentNetwork.Mainnet).encode(),
    "spxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfdzhutqqe9az04d3y7cfnd8me9mlnyg828j5z96urn2evjvy72f7m7me3ughqsvd62zyvj5nztf6uwsmqrz7f",
  );
  t.is(
    keys.labeledAddress(SilentPaymentNetwork.Testnet, 1).encode(),
    "tspxch1q5p85qjlmlhynz9ek3x07xtfzwkasq7q52yxr2g6jjjr66atnvp6h8t0zp5cuw5g8kspnrllhntyfd9jj2rac2q707npyfumq4wzqwkl79ksppyt5t58gecmqyj4396tzwlglqtamuqwwusfd6t8pkaq5cgn3xqq3",
  );
});

// An address assembled with an identity key (it cannot come from decode) can
// neither be encoded nor sent to.
test("identity-key address is rejected by encode and by the send action", (t) => {
  const keys = tv1Keys();
  const bad = new SilentPaymentAddress(
    PublicKey.infinity(),
    keys.spendPk(),
    SilentPaymentNetwork.Testnet,
  );
  t.throws(() => bad.encode(), { message: /identity element/ });

  const sim = new Simulator();
  const clvm = new Clvm();
  const sender = sim.bls(1_000n);
  const spends = new Spends(clvm, sender.puzzleHash);
  spends.addXch(sender.coin);
  t.throws(
    () => spends.apply([Action.silentPaymentSend(bad, 100n, undefined)]),
    { message: /identity element/ },
  );
});
