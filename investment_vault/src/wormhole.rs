//! Wormhole cross-chain bridge integration types and helpers.
//!
//! Wormhole integration uses a burn/mint pattern:
//! - **Outbound**: HBS are burned on Stellar, a Wormhole message is emitted.
//! - **Inbound**: A relayer delivers a VAA, which is verified, then HBS are minted.
//!
//! # Cross-chain bridge readiness (#48)
//!
//! The [`BridgeInterface`] trait defines a chain-agnostic bridge API that can be
//! implemented on Stellar, EVM, Solana, or any other blockchain. The contract
//! implements this trait internally; off-chain indexers and relayers SHOULD
//! rely on the standardised events and types defined here rather than
//! chain-specific formats.

use soroban_sdk::xdr::{FromXdr, ToXdr};
use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env};


pub mod chain_id {
    #![allow(dead_code)]

    pub const STELLAR: u32 = 38;
    pub const ETHEREUM: u32 = 2;
    pub const SOLANA: u32 = 1;
    pub const BSC: u32 = 4;
    pub const POLYGON: u32 = 5;
    pub const AVALANCHE: u32 = 6;
    pub const COSMOS: u32 = 18;
    /// IBC-enabled Cosmos chains (#48).
    pub const IBC: u32 = 19;
}

/// A chain-agnostic address represented as 32 raw bytes.
/// - Stellar: right-padded XDR of the `Address` type.
/// - EVM: left-padded 20-byte address.
/// - Solana: 32-byte ed25519 pubkey.
/// - Cosmos/IBC: 32-byte bech32 decoded pubkey.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CrossChainAddress {
    pub chain_id: u32,
    pub data: BytesN<32>,
}

// ── Bridge interface (#48) ───────────────────────────────────────────────────

/// Chain-agnostic bridge interface that can be implemented on any blockchain.
///
/// # Standardised bridge flow
///
/// 1. **Outbound**: `initiate_transfer` burns tokens on the source chain and
///    emits a `BridgeTransferInitiated` event. A relayer picks up the event
///    and submits a VAA/message to the target chain.
/// 2. **Inbound**: `complete_transfer` verifies the incoming message,
///    authenticates the emitter, and mints tokens to the recipient.
///
/// # Events (chain-agnostic)
///
/// All bridge events use the `BridgeTransferInitiated` and
/// `BridgeTransferCompleted` topics with standardised payload fields so that
/// off-chain indexers can process them uniformly regardless of the source chain.
pub trait BridgeInterface {
    /// Initialise the bridge with the core bridge contract address.
    fn set_bridge_core(env: Env, core: Address);

    /// Register or unregister a trusted cross-chain emitter.
    fn set_trusted_emitter(env: Env, chain_id: u32, emitter: BytesN<32>, trusted: bool);

    /// Initiate a cross-chain transfer. Burns `amount` of the local token
    /// and emits a bridge event for relayers to pick up.
    fn initiate_transfer(env: Env, from: Address, amount: i128, recipient: CrossChainAddress) -> u64;

    /// Complete an incoming cross-chain transfer. Verifies the VAA/message,
    /// authenticates the emitter, and mints tokens to the recipient.
    fn complete_transfer(env: Env, message: Bytes);
}

/// Payload for an HBS bridge transfer (binary-encoded in Wormhole messages).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeTransferPayload {
    pub token_address: BytesN<32>,
    pub recipient: BytesN<32>,
    pub amount: i128,
    pub source_chain: u32,
    pub target_chain: u32,
    pub nonce: u64,
}

/// Parsed VAA returned by the Wormhole core contract.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedVaa {
    pub emitter_chain: u32,
    pub emitter_address: BytesN<32>,
    pub payload: Bytes,
}

/// Bridge-related storage keys.
#[contracttype]
pub enum BridgeDataKey {
    WormholeCore,
    TrustedEmitter(u32, BytesN<32>),
    ConsumedVaa(BytesN<32>),
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

const PAYLOAD_PREFIX: &[u8] = b"HBS\x00";

/// Encode an Address into 32 bytes via the Env's XDR serialization.
pub fn address_to_bytes32(env: &Env, addr: &Address) -> BytesN<32> {
    let xdr = addr.to_xdr(env);
    let len = xdr.len();
    let mut buf = [0u8; 32];
    let copy_len = core::cmp::min(len as usize, 32usize);
    for i in 0..copy_len {
        buf[32 - copy_len + i] = xdr.get(i as u32).unwrap_or(0);
    }
    BytesN::from_array(env, &buf)
}

/// Decode an Address from 32 bytes via the Env's XDR deserialization.
pub fn bytes32_to_address(env: &Env, b32: &BytesN<32>) -> Address {
    let raw: Bytes = b32.clone().into();
    Address::from_xdr(env, &raw).expect("invalid address in bridge payload")
}

/// Serialize a `BridgeTransferPayload` into a `Bytes` payload for Wormhole.
pub fn serialize_bridge_payload(env: &Env, p: &BridgeTransferPayload) -> Bytes {
    let mut buf = Bytes::new(env);
    buf.append(&Bytes::from_slice(env, PAYLOAD_PREFIX));

    let token: [u8; 32] = p.token_address.clone().into();
    buf.append(&Bytes::from_slice(env, &token));

    let rec: [u8; 32] = p.recipient.clone().into();
    buf.append(&Bytes::from_slice(env, &rec));

    buf.append(&Bytes::from_slice(env, &p.amount.to_be_bytes()));
    buf.append(&Bytes::from_slice(env, &p.source_chain.to_be_bytes()));
    buf.append(&Bytes::from_slice(env, &p.target_chain.to_be_bytes()));
    buf.append(&Bytes::from_slice(env, &p.nonce.to_be_bytes()));
    buf
}

/// Parse a `BridgeTransferPayload` from raw Wormhole payload bytes.
pub fn parse_bridge_payload(env: &Env, raw: &Bytes) -> BridgeTransferPayload {
    let prefix_len: u32 = 4;
    let mut offset: u32 = prefix_len;

    let mut token_arr = [0u8; 32];
    for i in 0..32u32 {
        token_arr[i as usize] = raw.get(offset + i).unwrap_or(0);
    }
    offset += 32;

    let mut rec_arr = [0u8; 32];
    for i in 0..32u32 {
        rec_arr[i as usize] = raw.get(offset + i).unwrap_or(0);
    }
    offset += 32;

    let mut amount_buf = [0u8; 16];
    for i in 0..16u32 {
        amount_buf[i as usize] = raw.get(offset + i).unwrap_or(0);
    }
    offset += 16;

    let mut src_buf = [0u8; 4];
    for i in 0..4u32 {
        src_buf[i as usize] = raw.get(offset + i).unwrap_or(0);
    }
    offset += 4;

    let mut tgt_buf = [0u8; 4];
    for i in 0..4u32 {
        tgt_buf[i as usize] = raw.get(offset + i).unwrap_or(0);
    }
    offset += 4;

    let mut nonce_buf = [0u8; 8];
    for i in 0..8u32 {
        nonce_buf[i as usize] = raw.get(offset + i).unwrap_or(0);
    }

    BridgeTransferPayload {
        token_address: BytesN::from_array(env, &token_arr),
        recipient: BytesN::from_array(env, &rec_arr),
        amount: i128::from_be_bytes(amount_buf),
        source_chain: u32::from_be_bytes(src_buf),
        target_chain: u32::from_be_bytes(tgt_buf),
        nonce: u64::from_be_bytes(nonce_buf),
    }
}
