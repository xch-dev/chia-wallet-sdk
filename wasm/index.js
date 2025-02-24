import { Address, Mnemonic, SecretKey } from "./pkg";

console.log(
  new Address(
    SecretKey.fromSeed(Mnemonic.generate(true).toSeed("")).toBytes(),
    "billybob"
  ).encode()
);
