//! Wormhole cross-chain bridge integration types and helpers.
//!
//! Wormhole integration uses a burn/mint pattern:
//! - **Outbound**: HBS are burned on Stellar, a Wormhole message is emitted.
//! - **Inbound**: A relayer delivers a VAA, which is verified, then HBS are minted.

use soroban_sdk::xdr::{FromXdr, ToXdr};
use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env};

/// Chain identifiers matching the Wormhole chain ID registry.
pub mod chain_id {
    pub const STELLAR: u32 = 38;
    pub const ETHEREUM: u32 = 2;
    pub const SOLANA: u32 = 1;
    pub const BSC: u32 = 4;
    pub const POLYGON: u32 = 5;
    pub const AVALANCHE: u32 = 6;
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
