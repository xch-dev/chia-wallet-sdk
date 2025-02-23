/* auto-generated by NAPI-RS */
/* eslint-disable */
export declare class Address {
  encode(): string
  static decode(address: string): Address
  constructor(puzzleHash: Uint8Array, prefix: string)
  get puzzleHash(): Uint8Array
  set puzzleHash(value: Uint8Array)
  get prefix(): string
  set prefix(value: string)
}

export declare class BlsPair {
  constructor(sk: SecretKey, pk: PublicKey)
  get sk(): SecretKey
  set sk(value: SecretKey)
  get pk(): PublicKey
  set pk(value: PublicKey)
}

export declare class BlsPairWithCoin {
  constructor(sk: SecretKey, pk: PublicKey, puzzleHash: Uint8Array, coin: Coin)
  get sk(): SecretKey
  set sk(value: SecretKey)
  get pk(): PublicKey
  set pk(value: PublicKey)
  get puzzleHash(): Uint8Array
  set puzzleHash(value: Uint8Array)
  get coin(): Coin
  set coin(value: Coin)
}

export declare class Cat {
  constructor(coin: Coin, lineageProof: LineageProof | undefined | null, assetId: Uint8Array, p2PuzzleHash: Uint8Array)
  get coin(): Coin
  set coin(value: Coin)
  get lineageProof(): LineageProof | null
  set lineageProof(value?: LineageProof | undefined | null)
  get assetId(): Uint8Array
  set assetId(value: Uint8Array)
  get p2PuzzleHash(): Uint8Array
  set p2PuzzleHash(value: Uint8Array)
}

export declare class CatSpend {
  constructor(cat: Cat, spend: Spend)
  get cat(): Cat
  set cat(value: Cat)
  get spend(): Spend
  set spend(value: Spend)
}

export declare class Clvm {
  constructor()
  addCoinSpend(coinSpend: CoinSpend): void
  coinSpends(): Array<CoinSpend>
  pair(first: Program, rest: Program): Program
  nil(): Program
  string(value: string): Program
  bool(value: boolean): Program
  atom(value: Uint8Array): Program
  list(value: Array<Program>): Program
  delegatedSpend(conditions: Array<Program>): Spend
  standardSpend(syntheticKey: PublicKey, spend: Spend): Spend
  spendStandardCoin(coin: Coin, syntheticKey: PublicKey, spend: Spend): void
  spendCatCoins(catSpends: Array<CatSpend>): void
  mintNfts(parentCoinId: Uint8Array, nftMints: Array<NftMint>): MintedNfts
  int(value: number): Program
  bigInt(value: bigint): Program
}

export declare class Coin {
  coinId(): Uint8Array
  constructor(parentCoinInfo: Uint8Array, puzzleHash: Uint8Array, amount: bigint)
  get parentCoinInfo(): Uint8Array
  set parentCoinInfo(value: Uint8Array)
  get puzzleHash(): Uint8Array
  set puzzleHash(value: Uint8Array)
  get amount(): bigint
  set amount(value: bigint)
}

export declare class CoinSpend {
  constructor(coin: Coin, puzzleReveal: Uint8Array, solution: Uint8Array)
  get coin(): Coin
  set coin(value: Coin)
  get puzzleReveal(): Uint8Array
  set puzzleReveal(value: Uint8Array)
  get solution(): Uint8Array
  set solution(value: Uint8Array)
}

export declare class CurriedProgram {
  constructor(program: Program, args: Array<Program>)
  get program(): Program
  set program(value: Program)
  get args(): Array<Program>
  set args(value: Array<Program>)
}

export declare class DidOwner {
  constructor(didId: Uint8Array, innerPuzzleHash: Uint8Array)
  get didId(): Uint8Array
  set didId(value: Uint8Array)
  get innerPuzzleHash(): Uint8Array
  set innerPuzzleHash(value: Uint8Array)
}

export declare class K1Pair {
  constructor(sk: K1SecretKey, pk: K1PublicKey)
  get sk(): K1SecretKey
  set sk(value: K1SecretKey)
  get pk(): K1PublicKey
  set pk(value: K1PublicKey)
}

export declare class K1PublicKey {
  static fromBytes(bytes: Uint8Array): K1PublicKey
  toBytes(): Uint8Array
  fingerprint(): number
  verifyPrehashed(prehashed: Uint8Array, signature: K1Signature): boolean
}

export declare class K1SecretKey {
  static fromBytes(bytes: Uint8Array): K1SecretKey
  toBytes(): Uint8Array
  publicKey(): K1PublicKey
  signPrehashed(prehashed: Uint8Array): K1Signature
}

export declare class K1Signature {
  static fromBytes(bytes: Uint8Array): K1Signature
  toBytes(): Uint8Array
}

export declare class LineageProof {
  constructor(parentParentCoinInfo: Uint8Array, parentInnerPuzzleHash: Uint8Array | undefined | null, parentAmount: bigint)
  get parentParentCoinInfo(): Uint8Array
  set parentParentCoinInfo(value: Uint8Array)
  get parentInnerPuzzleHash(): Uint8Array | null
  set parentInnerPuzzleHash(value?: Uint8Array | undefined | null)
  get parentAmount(): bigint
  set parentAmount(value: bigint)
}

export declare class MintedNfts {
  constructor(nfts: Array<Nft>, parentConditions: Array<Program>)
  get nfts(): Array<Nft>
  set nfts(value: Array<Nft>)
  get parentConditions(): Array<Program>
  set parentConditions(value: Array<Program>)
}

export declare class Mnemonic {
  constructor(mnemonic: string)
  static fromEntropy(entropy: Uint8Array): Mnemonic
  static generate(use24: boolean): Mnemonic
  static verify(mnemonic: string): boolean
  toString(): string
  toEntropy(): Uint8Array
  toSeed(password: string): Uint8Array
}

export declare class Nft {
  constructor(coin: Coin, lineageProof: LineageProof, info: NftInfo)
  get coin(): Coin
  set coin(value: Coin)
  get lineageProof(): LineageProof
  set lineageProof(value: LineageProof)
  get info(): NftInfo
  set info(value: NftInfo)
}

export declare class NftInfo {
  constructor(launcherId: Uint8Array, metadata: Program, metadataUpdaterPuzzleHash: Uint8Array, currentOwner: Uint8Array | undefined | null, royaltyPuzzleHash: Uint8Array, royaltyTenThousandths: number, p2PuzzleHash: Uint8Array)
  get launcherId(): Uint8Array
  set launcherId(value: Uint8Array)
  get metadata(): Program
  set metadata(value: Program)
  get metadataUpdaterPuzzleHash(): Uint8Array
  set metadataUpdaterPuzzleHash(value: Uint8Array)
  get currentOwner(): Uint8Array | null
  set currentOwner(value?: Uint8Array | undefined | null)
  get royaltyPuzzleHash(): Uint8Array
  set royaltyPuzzleHash(value: Uint8Array)
  get royaltyTenThousandths(): number
  set royaltyTenThousandths(value: number)
  get p2PuzzleHash(): Uint8Array
  set p2PuzzleHash(value: Uint8Array)
}

export declare class NftMetadata {
  constructor(editionNumber: bigint, editionTotal: bigint, dataUris: Array<string>, dataHash: Uint8Array | undefined | null, metadataUris: Array<string>, metadataHash: Uint8Array | undefined | null, licenseUris: Array<string>, licenseHash?: Uint8Array | undefined | null)
  get editionNumber(): bigint
  set editionNumber(value: bigint)
  get editionTotal(): bigint
  set editionTotal(value: bigint)
  get dataUris(): Array<string>
  set dataUris(value: Array<string>)
  get dataHash(): Uint8Array | null
  set dataHash(value?: Uint8Array | undefined | null)
  get metadataUris(): Array<string>
  set metadataUris(value: Array<string>)
  get metadataHash(): Uint8Array | null
  set metadataHash(value?: Uint8Array | undefined | null)
  get licenseUris(): Array<string>
  set licenseUris(value: Array<string>)
  get licenseHash(): Uint8Array | null
  set licenseHash(value?: Uint8Array | undefined | null)
}

export declare class NftMint {
  constructor(metadata: Program, metadataUpdaterPuzzleHash: Uint8Array, p2PuzzleHash: Uint8Array, royaltyPuzzleHash: Uint8Array, royaltyTenThousandths: number, owner?: DidOwner | undefined | null)
  get metadata(): Program
  set metadata(value: Program)
  get metadataUpdaterPuzzleHash(): Uint8Array
  set metadataUpdaterPuzzleHash(value: Uint8Array)
  get p2PuzzleHash(): Uint8Array
  set p2PuzzleHash(value: Uint8Array)
  get royaltyPuzzleHash(): Uint8Array
  set royaltyPuzzleHash(value: Uint8Array)
  get royaltyTenThousandths(): number
  set royaltyTenThousandths(value: number)
  get owner(): DidOwner | null
  set owner(value?: DidOwner | undefined | null)
}

export declare class Output {
  constructor(value: Program, cost: bigint)
  get value(): Program
  set value(value: Program)
  get cost(): bigint
  set cost(value: bigint)
}

export declare class Pair {
  constructor(first: Program, rest: Program)
  get first(): Program
  set first(value: Program)
  get rest(): Program
  set rest(value: Program)
}

export declare class ParsedNft {
  constructor(info: NftInfo, p2Puzzle: Program)
  get info(): NftInfo
  set info(value: NftInfo)
  get p2Puzzle(): Program
  set p2Puzzle(value: Program)
}

export declare class Program {
  serialize(): Uint8Array
  serializeWithBackrefs(): Uint8Array
  run(solution: Program, maxCost: bigint, mempoolMode: boolean): Output
  curry(program: Program, args: Array<Program>): Program
  uncurry(): CurriedProgram | null
  treeHash(): Uint8Array
  length(): number
  first(): Program
  rest(): Program
  toString(): string | null
  toBool(): boolean | null
  toAtom(): Uint8Array | null
  toList(): Array<Program> | null
  toPair(): Pair | null
  puzzle(): Puzzle
  toInt(): number | null
  toBigInt(): bigint | null
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

export declare class Puzzle {
  parseNft(): ParsedNft | null
  constructor(puzzleHash: Uint8Array, program: Program, modHash: Uint8Array, args?: Program | undefined | null)
  get puzzleHash(): Uint8Array
  set puzzleHash(value: Uint8Array)
  get program(): Program
  set program(value: Program)
  get modHash(): Uint8Array
  set modHash(value: Uint8Array)
  get args(): Program | null
  set args(value?: Program | undefined | null)
}

export declare class R1Pair {
  constructor(sk: R1SecretKey, pk: R1PublicKey)
  get sk(): R1SecretKey
  set sk(value: R1SecretKey)
  get pk(): R1PublicKey
  set pk(value: R1PublicKey)
}

export declare class R1PublicKey {
  static fromBytes(bytes: Uint8Array): R1PublicKey
  toBytes(): Uint8Array
  fingerprint(): number
  verifyPrehashed(prehashed: Uint8Array, signature: R1Signature): boolean
}

export declare class R1SecretKey {
  static fromBytes(bytes: Uint8Array): R1SecretKey
  toBytes(): Uint8Array
  publicKey(): R1PublicKey
  signPrehashed(prehashed: Uint8Array): R1Signature
}

export declare class R1Signature {
  static fromBytes(bytes: Uint8Array): R1Signature
  toBytes(): Uint8Array
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

export declare class Simulator {
  constructor()
  newCoin(puzzleHash: Uint8Array, amount: bigint): Coin
  bls(amount: bigint): BlsPairWithCoin
  spendCoins(coinSpends: Array<CoinSpend>, secretKeys: Array<SecretKey>): void
}

export declare class Spend {
  constructor(puzzle: Program, solution: Program)
  get puzzle(): Program
  set puzzle(value: Program)
  get solution(): Program
  set solution(value: Program)
}

export declare class SpendBundle {
  constructor(coinSpends: Array<CoinSpend>, aggregatedSignature: Signature)
  get coinSpends(): Array<CoinSpend>
  set coinSpends(value: Array<CoinSpend>)
  get aggregatedSignature(): Signature
  set aggregatedSignature(value: Signature)
}

export declare function bytesEqual(lhs: Uint8Array, rhs: Uint8Array): boolean

export declare function catPuzzleHash(assetId: Uint8Array, innerPuzzleHash: Uint8Array): Uint8Array

export declare function curryTreeHash(program: Uint8Array, args: Array<Uint8Array>): Uint8Array

export declare function fromHex(value: string): Uint8Array

export declare function generateBytes(bytes: number): Uint8Array

export declare function sha256(value: Uint8Array): Uint8Array

export declare function standardPuzzleHash(syntheticKey: PublicKey): Uint8Array

export declare function toHex(value: Uint8Array): string

export declare function treeHashAtom(atom: Uint8Array): Uint8Array

export declare function treeHashPair(first: Uint8Array, rest: Uint8Array): Uint8Array
