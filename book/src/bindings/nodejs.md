# Node.js Bindings

The Chia Wallet SDK provides Node.js bindings that allow you to use the SDK's functionality in JavaScript/TypeScript applications.

## Installation

First, install the package from npm:

```bash
npm install chia-wallet-sdk
# or
yarn add chia-wallet-sdk
# or
pnpm add chia-wallet-sdk
```

## Usage

Import the functions and classes you need:

```javascript
const {
  SecretKey,
  PublicKey,
  Signature,
  Address,
  Coin,
  SpendBundle,
  Clvm,
  CoinsetClient
} = require('chia-wallet-sdk');

// Or using ES modules:
import {
  SecretKey,
  PublicKey, 
  Signature,
  Address,
  Coin,
  SpendBundle,
  Clvm,
  CoinsetClient
} from 'chia-wallet-sdk';
```

## Key Management

Create and work with keys:

```javascript
// Generate a new key from seed
const seed = new Uint8Array(32); // Your seed bytes
const sk = SecretKey.fromSeed(seed);

// Get the public key
const pk = sk.publicKey();

// Sign a message
const message = new TextEncoder().encode('Hello, Chia!');
const signature = sk.sign(message);

// Verify the signature
const isValid = signature.isValid();
```

## Working with Addresses

Convert between puzzle hashes and addresses:

```javascript
// Create an address from a puzzle hash
const puzzleHash = new Uint8Array(32); // Your puzzle hash
const address = new Address(puzzleHash, 'xch');

// Encode the address
const encodedAddress = address.encode();
console.log('XCH address:', encodedAddress);

// Decode an address
const decodedAddress = Address.decode(encodedAddress);
```

## Creating Transactions

Use the CLVM (ChiaLisp Virtual Machine) to create transactions:

```javascript
const clvm = new Clvm();

// Create a coin
const parentCoinInfo = new Uint8Array(32); // Parent coin ID
const puzzleHash = new Uint8Array(32); // Recipient's puzzle hash
const amount = 1000n; // Amount in mojos
const coin = new Coin(parentCoinInfo, puzzleHash, amount);

// Create a simple spend
const conditions = [];
conditions.push(clvm.createCoin(puzzleHash, 900n));
conditions.push(clvm.reserveFee(100n));

const delegatedSpend = clvm.delegatedSpend(conditions);

// For a standard transaction
const standardSpend = clvm.standardSpend(pk, delegatedSpend);
clvm.spendStandardCoin(coin, pk, delegatedSpend);

// Get the coin spends
const coinSpends = clvm.coinSpends();

// Create and sign the spend bundle
const aggregatedSignature = Signature.aggregate([signature]);
const spendBundle = new SpendBundle(coinSpends, aggregatedSignature);
```

## Interacting with the Network

Connect to the Chia network and query coin records:

```javascript
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
    
    // Push a transaction
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

```javascript
// To be implemented based on the detailed CAT API
```

## Error Handling

Handle errors from the SDK:

```javascript
try {
  // SDK operations
} catch (error) {
  console.error('Error type:', error.constructor.name);
  console.error('Error message:', error.message);
}
```
