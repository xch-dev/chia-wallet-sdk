use bindy::Result;
use chia_protocol::{Bytes32, BytesImpl};
use chia_sdk_driver::P2Eip712MessageLayer;
use chia_sdk_types::Mod;
use chia_sdk_types::puzzles::{Eip712Member, Eip712PrefixAndDomainSeparator};
use clvm_utils::TreeHash;

use crate::K1PublicKey;

/// CHIP-0037 EIP-712 type hash for the canonical
/// `ChiaCoinSpend(bytes32 coin_id,bytes32 delegated_puzzle_hash)` schema.
///
/// Off-chain wallets need this value to assemble the EIP-712 typed-data
/// envelope (alongside the network-specific domain separator) before
/// presenting the message to a signer.
pub fn eip712_type_hash() -> Result<Bytes32> {
    Ok(P2Eip712MessageLayer::type_hash())
}

/// Compute the EIP-712 domain separator for a given Chia network.
///
/// `genesis_challenge` is the network's genesis challenge (e.g. mainnet's
/// `ccd5bb71...e5fbb`); the resulting 32-byte digest is the canonical
/// CHIP-0037 domain separator (`{ name: "Chia Coin Spend", version: "1",
/// salt: <genesis_challenge> }`).
pub fn eip712_domain_separator(genesis_challenge: Bytes32) -> Result<Bytes32> {
    Ok(P2Eip712MessageLayer::domain_separator(genesis_challenge))
}

/// Compute the 34-byte concatenation `\x19\x01 || domainSeparator(network)`
/// that gets curried into `Eip712Member` and `p2_eip712_message`.
///
/// The returned value is what callers should pass as `prefix_and_domain_separator`
/// when constructing an [`Eip712Member`](chia_sdk_types::puzzles::Eip712Member)
/// or a [`p2_eip712_message`](chia_sdk_types::puzzles::P2Eip712MessageArgs).
pub fn eip712_prefix_and_domain_separator(genesis_challenge: Bytes32) -> Result<BytesImpl<34>> {
    let prefix_and_domain: Eip712PrefixAndDomainSeparator =
        P2Eip712MessageLayer::prefix_and_domain_separator(genesis_challenge);
    Ok(prefix_and_domain)
}

/// Compute the 32-byte EIP-712 digest the off-chain wallet must sign:
/// `keccak256(prefix_and_domain || keccak256(typeHash || coin_id ||
/// delegated_puzzle_hash))`.
///
/// Equivalent to what MetaMask's `eth_signTypedData_v4` would internally
/// hash for the CHIP-0037 schema; exposed here so callers that already hold
/// a raw `K1SecretKey` can sign the prehash directly without round-tripping
/// through a typed-data JSON envelope.
pub fn eip712_hash_to_sign(
    genesis_challenge: Bytes32,
    coin_id: Bytes32,
    delegated_puzzle_hash: Bytes32,
) -> Result<Bytes32> {
    let prefix_and_domain = P2Eip712MessageLayer::prefix_and_domain_separator(genesis_challenge);
    Ok(P2Eip712MessageLayer::compute_hash_to_sign(
        &prefix_and_domain,
        coin_id,
        delegated_puzzle_hash,
    ))
}

/// Compute the bare ``Eip712Member::curry_tree_hash`` — the puzzle hash of an
/// `Eip712Member` curried with `(prefix_and_domain_separator, type_hash,
/// public_key)` and **no MIPS wrapper layer**.
///
/// Use this when the `Eip712Member` sits directly as a leaf in a hand-rolled
/// admin / one-of-N structure that walks its own leaf list (e.g. the
/// `populis_protocol` `admin_authority_v2` inner puzzle, which checks
/// `(= (sha256tree approving_member_reveal) <leaf_in_admin>)` on the bare
/// curried `Eip712Member`).
///
/// Use [`eip712_member_hash`](crate::eip712_member_hash) instead when the
/// `Eip712Member` is one leaf among many in a CHIP-0043 `m_of_n` / `n_of_n` /
/// `one_of_n` MIPS quorum tree — that helper additionally wraps with the
/// `index_wrapper` / restriction layers required by the quorum dispatcher.
pub fn eip712_member_inner_puzzle_hash(
    genesis_challenge: Bytes32,
    public_key: K1PublicKey,
) -> Result<TreeHash> {
    let prefix_and_domain = P2Eip712MessageLayer::prefix_and_domain_separator(genesis_challenge);
    let type_hash = P2Eip712MessageLayer::type_hash();
    Ok(Eip712Member::new(prefix_and_domain, type_hash, public_key.0).curry_tree_hash())
}
