# Node.js Bindings

The Chia Wallet SDK provides Node.js bindings that allow you to use the SDK's functionality in TypeScript applications.

## Installation

Install the package from npm:

```bash
npm install chia-wallet-sdk
# or
yarn add chia-wallet-sdk
# or
pnpm add chia-wallet-sdk
```

## Usage

Import the classes and functions you need:

```typescript
import {
  Mnemonic,
  SecretKey,
  PublicKey,
  Signature,
  Address,
  Coin,
  SpendBundle,
  Clvm,
  CoinsetClient,
  Simulator,
  BlsPairWithCoin
} from 'chia-wallet-sdk';
```

## Key Management

Create and work with keys using mnemonics:

```typescript
// Generate a new mnemonic (24 words by default)
const mnemonic = Mnemonic.generate(true); // true for 24 words, false for 12 words
console.log('Mnemonic:', mnemonic.toString());

// Convert mnemonic to seed
const seed = mnemonic.toSeed(''); // Empty password

// Create a key from seed
const sk = SecretKey.fromSeed(seed);

// Get the public key
const pk = sk.publicKey();

// Sign a message
const message = new TextEncoder().encode('Hello, Chia!');
const signature = sk.sign(message);

// Verify the signature with the public key
const isValid = PublicKey.fromBytes(pk.toBytes()).verify(message, signature);
console.log('Signature valid:', isValid);
```

## Working with Addresses

Convert between puzzle hashes and addresses:

```typescript
// Create an address from a puzzle hash
const puzzleHash = standardPuzzleHash(pk); // Get puzzle hash from public key
const address = new Address(puzzleHash, 'xch');

// Encode the address
const encodedAddress = address.encode();
console.log('XCH address:', encodedAddress);

// Decode an address
const decodedAddress = Address.decode(encodedAddress);
console.log('Puzzle hash matches:', bytesEqual(decodedAddress.puzzleHash, puzzleHash));
```

## Creating Transactions

Use the simulator for testing transactions:

```typescript
// Create a simulator for testing
const simulator = new Simulator();

// Create a test key pair with a coin
const alice = simulator.bls(1000n);

// Create a CLVM instance
const clvm = new Clvm();

// Create a simple spend with conditions
const conditions = [
  clvm.createCoin(alice.puzzleHash, 900n),
  clvm.reserveFee(100n)
];

// Create a delegated spend
const delegatedSpend = clvm.delegatedSpend(conditions);

// Spend the standard coin (this handles the standardSpend internally)
clvm.spendStandardCoin(alice.coin, alice.pk, delegatedSpend);

// Get the coin spends
const coinSpends = clvm.coinSpends();

// Submit the transaction to the simulator
simulator.spendCoins(coinSpends, [alice.sk]);
```

## Interacting with the Network

Connect to the Chia network and query coin records:

```typescript
// Connect to testnet
const client = CoinsetClient.testnet11();

// Or connect to mainnet
// const client = CoinsetClient.mainnet();

async function fetchCoins() {
  try {
    // Get blockchain state
    const state = await client.getBlockchainState();
    console.log('Blockchain state:', state);
    
    // Query coins by puzzle hash
    const puzzleHash = new Uint8Array(32); // Your puzzle hash
    const coinRecords = await client.getCoinRecordsByPuzzleHash(puzzleHash);
    console.log('Coin records:', coinRecords);
    
    // Create and push a transaction
    // ... create a spend bundle as shown above
    const response = await client.pushTx(spendBundle);
    console.log('Transaction result:', response);
  } catch (error) {
    console.error('Error:', error);
  }
}

fetchCoins();
```

## Working with CATs (Colored Coins)

To create and manage CAT tokens:

```typescript
// Create a simulator for testing
const simulator = new Simulator();
const alice = simulator.bls(1000n);

// Create a CLVM instance
const clvm = new Clvm();

// Create memos for the CAT
const memos = clvm.alloc(alice.puzzleHash);

// Create conditions for the issuance
const conditions = clvm.createCoin(alice.puzzleHash, 1000n, memos);

// Issue a new CAT
const [issueCat, cat] = Cat.singleIssuanceEve(
  clvm, 
  alice.coin.coinId(), 
  1000n, 
  conditions
);

// Spend the standard coin to issue the CAT
clvm.spendStandardCoin(alice.coin, alice.pk, issueCat);

// Create a CAT spend
const newCat = cat.wrappedChild(alice.puzzleHash, 1000n);
const catSpends = [
  new CatSpend(
    newCat,
    clvm.delegatedSpend([
      clvm.createCoin(alice.puzzleHash, 1000n, memos)
    ])
  )
];

// Spend the CAT
clvm.spendCatCoins(catSpends);

// Submit the transaction
simulator.spendCoins(clvm.coinSpends(), [alice.sk]);
```

## Error Handling

Handle errors from the SDK:

```typescript
try {
  // SDK operations
} catch (error) {
  console.error('Error type:', error instanceof Error ? error.constructor.name : typeof error);
  console.error('Error message:', error instanceof Error ? error.message : String(error));
}
```
