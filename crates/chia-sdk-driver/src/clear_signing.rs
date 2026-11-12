//! Clear signing for vaults: parsing a vault spend into a summary that a person can review before signing.
//!
//! A vault's custody signs a delegated puzzle hash, which on its own says nothing about what the transaction does.
//! [`parse_vault_transaction`] takes the delegated spend, the vault's launcher id, and the untrusted [`Reveals`] of
//! the rest of the transaction, and returns a [`VaultTransaction`] that only contains facts that are guaranteed by
//! the signature (if the transaction is valid at all):
//!
//! - The vault's own output ([`VaultOutput`]) and [`DropCoin`]s.
//! - The [`VerifiedSpend`]s that the vault authorizes with messages, and the coins they create ([`ParsedChild`]),
//!   including the payments of revealed Secure the Bag roots ([`SecureTheBagInfo`]).
//! - CAT [`Issuance`]s, asserted settlement payments ([`AssertedPayment`]), and future offers ([`LinkedOffer`]).
//! - The reserved fee, and an [`AssetFlow`] per asset that shows how much value is unaccounted for.
//! - The arbitrary messages that the vault signs, if its delegated spend is a vault message envelope.
//! - The [`VaultAuthorization`]s of controlled vaults, if the vault being signed for authorizes the spend of a vault
//!   that it controls, and the [`SigningVault`] that the message to sign should be calculated from.
//!
//! Anything that can't be verified is left out, and anything linked to the vault that can't be verified is an error.
//! See [`parse_vault_transaction`] for exactly what is and isn't guaranteed.

mod children;
mod delegated_spend;
mod facts;
mod inner_spend;
mod internal_custody;
mod issuance;
mod linked_offer;
mod memos;
mod parsed_spend;
mod requested_payments;
mod reveals;
mod signing;
mod vault_message;
mod vault_spend;
mod vault_transaction;

pub use children::*;
pub use delegated_spend::*;
pub use facts::*;
pub use inner_spend::*;
pub use internal_custody::*;
pub use issuance::*;
pub use linked_offer::*;
pub use memos::*;
pub use parsed_spend::*;
pub use requested_payments::*;
pub use reveals::*;
pub use signing::*;
pub use vault_message::*;
pub use vault_spend::*;
pub use vault_transaction::*;

#[cfg(test)]
mod controlled_vault_tests;
#[cfg(test)]
mod tests;
