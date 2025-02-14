import {
  encodeAddress,
  generateMnemonic,
  mnemonicToSeed,
  SecretKey,
} from "./pkg";

console.log(
  encodeAddress(
    SecretKey.fromSeed(mnemonicToSeed(generateMnemonic(true), "")).toBytes(),
    "billybob"
  )
);
