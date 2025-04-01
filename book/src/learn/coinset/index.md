# Coin Set Model

Chia's Coin Set model is similar to Bitcoin's UTXO model. For a more in-depth and general purpose explanation, please refer to the [Chia docs](https://docs.chia.net/coin-set-intro/) on the topic. We're going to focus primarily on what this means for implementing dApps and wallets using the Wallet SDK.

This is the single most important thing to understand when developing on Chia, since it differs pretty heavily from Ethereum's account model. These differences can make learning more complicated, but there is a powerful set of primitives you can build on top of to create secure and auditable code that interacts with the blockchain.
