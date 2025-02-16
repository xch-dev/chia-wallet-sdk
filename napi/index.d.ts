/* auto-generated by NAPI-RS */
/* eslint-disable */
export declare class AddressInfo {
  puzzleHash: Uint8Array
  prefix: string
}

export declare class PublicKey {
  static infinity(): PublicKey
  static aggregate(publicKeys: Array<PublicKey>): PublicKey
  static fromBytes(bytes: Uint8Array): PublicKey
  toBytes(): Uint8Array
  fingerprint(): number
  isInfinity(): boolean
  isValid(): boolean
  deriveUnhardened(index: number): PublicKey
  deriveUnhardenedPath(path: Array<number>): PublicKey
  deriveSynthetic(): PublicKey
  deriveSyntheticHidden(hiddenPuzzleHash: Uint8Array): PublicKey
}

export declare class SecretKey {
  static fromSeed(seed: Uint8Array): SecretKey
  static fromBytes(bytes: Uint8Array): SecretKey
  toBytes(): Uint8Array
  publicKey(): PublicKey
  sign(message: Uint8Array): Signature
  deriveUnhardened(index: number): SecretKey
  deriveHardened(index: number): SecretKey
  deriveUnhardenedPath(path: Array<number>): SecretKey
  deriveHardenedPath(path: Array<number>): SecretKey
  deriveSynthetic(): SecretKey
  deriveSyntheticHidden(hiddenPuzzleHash: Uint8Array): SecretKey
}

export declare class Signature {
  static infinity(): Signature
  static aggregate(signatures: Array<Signature>): Signature
  static fromBytes(bytes: Uint8Array): Signature
  toBytes(): Uint8Array
  isInfinity(): boolean
  isValid(): boolean
}

export declare function bytesEqual(lhs: Uint8Array, rhs: Uint8Array): boolean

export interface Coin {
  parentCoinInfo: Uint8Array
  puzzleHash: Uint8Array
  amount: bigint
}

export interface CoinSpend {
  coin: Coin
  puzzleReveal: Uint8Array
  solution: Uint8Array
}

export interface CoinState {
  coin: Coin
  spentHeight?: number
  createdHeight?: number
}

export declare function decodeAddress(address: string): AddressInfo

export declare function encodeAddress(puzzleHash: Uint8Array, prefix: string): string

export declare function fromHex(value: string): Uint8Array

export declare function generateBytes(bytes: number): Uint8Array

export declare function generateMnemonic(use24: boolean): string

export declare function mnemonicFromEntropy(entropy: Uint8Array): string

export declare function mnemonicToEntropy(mnemonic: string): Uint8Array

export declare function mnemonicToSeed(mnemonic: string, password: string): Uint8Array

export declare function sha256(value: Uint8Array): Uint8Array

export declare function toHex(value: Uint8Array): string

export declare function treeHashAtom(atom: Uint8Array): Uint8Array

export declare function treeHashPair(first: Uint8Array, rest: Uint8Array): Uint8Array

export declare function verifyMnemonic(mnemonic: string): boolean
