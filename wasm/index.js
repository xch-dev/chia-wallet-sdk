import { decodeAddress, encodeAddress } from "./pkg";

console.log(decodeAddress(encodeAddress(new Uint8Array(32), "txch")).prefix);
