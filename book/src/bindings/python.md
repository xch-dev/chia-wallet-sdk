# Python Bindings

The Chia Wallet SDK provides Python bindings that allow you to use the SDK's functionality in Python applications.

## Installation

Install the package using pip:

```bash
pip install chia-wallet-sdk
```

## Usage

Import the classes and functions you need:

```python
from chia_wallet_sdk import (
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
    standard_puzzle_hash,
    bytes_equal
)
```

## Key Management

Create and work with keys using mnemonics:

```python
# Generate a new mnemonic (24 words by default)
mnemonic = Mnemonic.generate(True)  # True for 24 words, False for 12 words
print(f"Mnemonic: {str(mnemonic)}")

# Convert mnemonic to seed
seed = mnemonic.to_seed("")  # Empty password

# Create a key from seed
sk = SecretKey.from_seed(seed)

# Get the public key
pk = sk.public_key()

# Sign a message
message = b"Hello, Chia!"
signature = sk.sign(message)

# Verify the signature with the public key
is_valid = PublicKey.from_bytes(pk.to_bytes()).verify(message, signature)
print(f"Signature valid: {is_valid}")
```

## Working with Addresses

Convert between puzzle hashes and addresses:

```python
# Create an address from a puzzle hash
puzzle_hash = standard_puzzle_hash(pk)  # Get puzzle hash from public key
address = Address(puzzle_hash, "xch")

# Encode the address
encoded_address = address.encode()
print(f"XCH address: {encoded_address}")

# Decode an address
decoded_address = Address.decode(encoded_address)
print(f"Puzzle hash matches: {bytes_equal(decoded_address.puzzle_hash, puzzle_hash)}")
```

## Creating Transactions

Use the simulator for testing transactions:

```python
# Create a simulator for testing
simulator = Simulator()

# Create a test key pair with a coin
alice = simulator.bls(1000)

# Create a CLVM instance
clvm = Clvm()

# Create a simple spend with conditions
conditions = [
    clvm.create_coin(alice.puzzle_hash, 900),
    clvm.reserve_fee(100)
]

# Create a delegated spend
delegated_spend = clvm.delegated_spend(conditions)

# Spend the standard coin (this handles the standard_spend internally)
clvm.spend_standard_coin(alice.coin, alice.pk, delegated_spend)

# Get the coin spends
coin_spends = clvm.coin_spends()

# Submit the transaction to the simulator
simulator.spend_coins(coin_spends, [alice.sk])
```

## Interacting with the Network

Connect to the Chia network and query coin records:

```python
import asyncio

# Connect to testnet
client = CoinsetClient.testnet11()

# Or connect to mainnet
# client = CoinsetClient.mainnet()

async def fetch_coins():
    try:
        # Get blockchain state
        state = await client.get_blockchain_state()
        print(f"Blockchain state: {state}")
        
        # Query coins by puzzle hash
        puzzle_hash = standard_puzzle_hash(pk)  # Use a real puzzle hash
        coin_records = await client.get_coin_records_by_puzzle_hash(puzzle_hash)
        print(f"Coin records: {coin_records}")
        
        # Create and push a transaction
        # ... create a spend bundle as shown above
        response = await client.push_tx(spend_bundle)
        print(f"Transaction result: {response}")
    except Exception as e:
        print(f"Error: {e}")

# Run the async function
asyncio.run(fetch_coins())
```

## Working with CATs (Colored Coins)

To create and manage CAT tokens:

```python
# Create a simulator for testing
simulator = Simulator()
alice = simulator.bls(1000)

# Create a CLVM instance
clvm = Clvm()

# Create memos for the CAT
memos = clvm.alloc(alice.puzzle_hash)

# Create conditions for the issuance
conditions = clvm.create_coin(alice.puzzle_hash, 1000, memos)

# Issue a new CAT
issue_cat, cat = Cat.single_issuance_eve(
    clvm, 
    alice.coin.coin_id(), 
    1000, 
    conditions
)

# Spend the standard coin to issue the CAT
clvm.spend_standard_coin(alice.coin, alice.pk, issue_cat)

# Create a CAT spend
new_cat = cat.wrapped_child(alice.puzzle_hash, 1000)
cat_spends = [
    CatSpend(
        new_cat,
        clvm.delegated_spend([
            clvm.create_coin(alice.puzzle_hash, 1000, memos)
        ])
    )
]

# Spend the CAT
clvm.spend_cat_coins(cat_spends)

# Submit the transaction
simulator.spend_coins(clvm.coin_spends(), [alice.sk])
```

## Working with NFTs

To create and manage NFTs:

```python
# Create a simulator for testing
simulator = Simulator()
alice = simulator.bls(1000)

# Create a CLVM instance
clvm = Clvm()

# Define NFT metadata
metadata = {
    "name": "My NFT",
    "description": "A test NFT",
    "uri": "https://example.com/nft.json"
}

# Create an NFT mint request
nft_mint = NftMint(
    launcher_id=None,  # Will be generated
    target_address=alice.puzzle_hash,
    metadata=metadata,
    royalty_address=alice.puzzle_hash,
    royalty_percentage=10,  # 10% royalty
    did_id=None  # No DID association
)

# Mint the NFT
minted_nfts = clvm.mint_nfts(alice.coin.coin_id(), [nft_mint])

# Spend the standard coin to mint the NFT
clvm.spend_standard_coin(alice.coin, alice.pk, minted_nfts.spend)

# Submit the transaction
simulator.spend_coins(clvm.coin_spends(), [alice.sk])
```

## Error Handling

Handle errors from the SDK:

```python
try:
    # SDK operations
except Exception as e:
    print(f"Error type: {type(e).__name__}")
    print(f"Error message: {str(e)}")
```

## Asynchronous Programming

Many functions in the SDK are asynchronous and should be used with Python's asyncio:

```python
import asyncio

async def main():
    # Asynchronous SDK operations
    pass

asyncio.run(main())
```
