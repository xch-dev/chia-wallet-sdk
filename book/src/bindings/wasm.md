# WebAssembly Bindings

The Chia Wallet SDK provides WebAssembly (WASM) bindings that allow you to use the SDK's functionality in web applications.

## Installation

Install the package using npm:

```bash
npm install @chia/wallet-sdk-wasm
# or
yarn add @chia/wallet-sdk-wasm
# or
pnpm add @chia/wallet-sdk-wasm
```

## Usage in a Web Application

Import the WASM module in your JavaScript/TypeScript code:

```javascript
import * as wallet from '@chia/wallet-sdk-wasm';

// Initialize the module (this is an async operation)
async function init() {
  await wallet.default();
  
  // Now you can use the wallet SDK functions
  const result = wallet.someFunction();
}

init();
```

## Key Management

Create and work with keys:

```javascript
// Generate a new key from seed
const seed = new Uint8Array(32); // Your seed bytes
const sk = wallet.SecretKey.fromSeed(seed);

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
const address = new wallet.Address(puzzleHash, 'xch');

// Encode the address
const encodedAddress = address.encode();
console.log('XCH address:', encodedAddress);

// Decode an address
const decodedAddress = wallet.Address.decode(encodedAddress);
```

## Creating Transactions

Use the CLVM (ChiaLisp Virtual Machine) to create transactions:

```javascript
const clvm = new wallet.Clvm();

// Create a coin
const parentCoinInfo = new Uint8Array(32); // Parent coin ID
const puzzleHash = new Uint8Array(32); // Recipient's puzzle hash
const amount = 1000n; // Amount in mojos
const coin = new wallet.Coin(parentCoinInfo, puzzleHash, amount);

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
const aggregatedSignature = wallet.Signature.aggregate([signature]);
const spendBundle = new wallet.SpendBundle(coinSpends, aggregatedSignature);
```

## Interacting with the Network

Connect to the Chia network and query coin records:

```javascript
// Connect to testnet
const client = wallet.CoinsetClient.testnet11();

// Or connect to mainnet
// const client = wallet.CoinsetClient.mainnet();

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

## Memory Management

When working with WASM, be aware of memory management:

```javascript
// Create objects
const obj = new wallet.SomeObject();

// Use the object
obj.doSomething();

// When done, free the memory
obj.free();
```

## Working in Different Environments

The WASM bindings can be used in various environments:

### In a Browser

```html
<script type="module">
  import * as wallet from '@chia/wallet-sdk-wasm';

  async function init() {
    await wallet.default();
    // Use the SDK
  }

  init();
</script>
```

### In Node.js

```javascript
const wallet = require('@chia/wallet-sdk-wasm');

async function init() {
  await wallet.default();
  // Use the SDK
}

init();
```

## Error Handling

Handle errors from the SDK:

```javascript
try {
  // SDK operations
} catch (error) {
  console.error('Error:', error);
}
```
