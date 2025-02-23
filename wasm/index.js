import { encodeAddress, Mnemonic, SecretKey } from "./pkg";

console.log(
  encodeAddress(
    SecretKey.fromSeed(Mnemonic.generate(true).toSeed("")).toBytes(),
    "billybob"
  )
);
