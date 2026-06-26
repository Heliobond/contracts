# Bridge Security — Heliobond HBS Cross-Chain Transfers

This document covers security considerations for the Wormhole-based bridge enabling HBS tokens to move across chains.

---

## Threat Model

### Assets at risk
- **HBS tokens** (total supply managed by `InvestmentVault` on Stellar)
- **User funds** deposited into the vault backing HBS share value
- **Bridge authority** — the ability to mint HBS on destination chains

### Trust assumptions
1. **Wormhole Guardian Network** — 2/3+ of guardians must be honest and available to produce valid VAAs.
2. **Bridge operator (contract owner)** — controls `set_bridge`, `set_wormhole_core`, and `set_trusted_emitter`. Must hold the owner key securely (multisig or timelock recommended).
3. **Stellar network finality** — deposits are considered final after standard Stellar confirmation (classic + Soroban consensus).

---

## Attack Vectors & Mitigations

### 1. Unauthorised minting via fake VAAs

**Risk:** An attacker submits a forged VAA to `complete_bridge_transfer` to mint HBS without a legitimate burn on the source chain.

**Mitigations:**
- VAA verification is delegated entirely to the Wormhole core contract, which validates ECDSA signatures from 2/3+ of the current guardian set.
- The VAA digest is recorded (`ConsumedVaa`) with SHA-256 to prevent replay attacks.
- Only pre-authorised emitter addresses (`TrustedEmitter`) can mint — even a valid VAA from an unknown bridge contract is rejected.

### 2. Replay attacks

**Risk:** The same VAA is submitted multiple times to mint HBS repeatedly.

**Mitigation:**
- `complete_bridge_transfer` computes `SHA-256(vaa_bytes)` and checks `ConsumedVaa(digest)` before processing.
- Once consumed, the digest is stored persistently and any re-submission panics.

### 3. Bridge authority compromise

**Risk:** The contract owner's key is stolen, allowing an attacker to:
- Change the Wormhole core contract address to a malicious contract
- Add a malicious `TrustedEmitter`
- Change the bridge address (via `set_bridge`)

**Mitigations:**
- `set_wormhole_core`, `set_trusted_emitter`, and `set_bridge` are protected by `#[only_owner]` (stellar-access Ownable).
- **Recommendation:** Deploy with a multisig account as the owner, or use a timelock + governance contract before mainnet.
- The owner should not be an EOA (externally owned account). Use Gnosis Safe, Squads, or Stellar multisig.

### 4. Wormhole core contract manipulation

**Risk:** An attacker deploys a fake Wormhole core contract and the owner is tricked into pointing `set_wormhole_core` at it.

**Mitigation:**
- The Wormhole core contract address should be verified from official Wormhole documentation.
- On Stellar testnet/mainnet, the core contract address is deterministic and should be validated before setting.
- Consider hard-coding the core contract address in a future upgrade after mainnet deployment.

### 5. Griefing via small bridging amounts

**Risk:** An attacker burns tiny amounts of HBS repeatedly, causing many Wormhole messages to be emitted and bloating state.

**Mitigation:**
- `amount > 0` is enforced in both `bridge_burn` and `initiate_bridge_transfer`.
- Consider adding a minimum bridge amount in a future iteration.
- Wormhole messages carry a fee which discourages spam.

### 6. Payload tampering between chains

**Risk:** A relayer modifies the bridge payload between source and destination chains.

**Mitigation:**
- The bridge payload is included inside the Wormhole VAA, which is signed by the guardian network.
- Any tampering invalidates the guardian signatures, causing VAA verification to fail.

---

## Operational Security

### Deployment checklist

| Item | Status |
|---|---|
| Contract owner is a multisig or DAO | ❌ Not yet (testnet) |
| Wormhole core contract address verified | ❌ Stellar support TBD by Wormhole |
| Trusted emitters pinned to known addresses | ❌ Configured post-deployment |
| Emergency pause mechanism | ❌ Not implemented (planned) |
| Monitoring alerts for bridge events | ❌ Off-chain infrastructure needed |

### Recommended configuration

1. **Set Wormhole core contract** — only after the address is confirmed via Wormhole's official contract registry.
2. **Add trusted emitters** — for each supported chain, add the corresponding bridge contract address with `set_trusted_emitter`.
3. **Test with small amounts** — perform test transfers on testnet before enabling mainnet.
4. **Monitor events** — watch `BridgeTransferInitiated` and `BridgeTransferCompleted` events for anomalous activity.

---

## Cryptographic Dependencies

| Component | Algorithm | Notes |
|---|---|---|
| VAA signing | ECDSA (secp256k1) | Performed by Wormhole guardians |
| VAA verification | ECDSA (secp256k1) | Delegated to Wormhole core contract |
| Replay protection | SHA-256 | Computed in-contract on raw VAA bytes |
| Payload encoding | Custom binary format | Prefix `HBS\0` + fixed-width fields |

---

## Recovery Procedures

### Stuck bridge transfer
If a VAA is valid but fails to process:
1. Verify the VAA is not already consumed.
2. Check that the emitter is in `TrustedEmitter`.
3. Ensure the Wormhole core address is correct.
4. If all checks pass, re-submit the same VAA bytes.

### Accidental bridge misconfiguration
If `set_wormhole_core` or `set_trusted_emitter` is set incorrectly:
1. The contract owner can call the setter again with the correct value.
2. There is no timelock — configure carefully.

---

## Future Improvements

- **Timelock** on owner-only bridge configuration functions.
- **Emergency pause** to halt all bridge operations.
- **Rate limiting** on `initiate_bridge_transfer` per address.
- **Minimum bridge amount** to prevent dust attacks.
- **Governance** — migrate owner to a DAO after mainnet launch.
- **IBC alternative** — add IBC integration as a second bridge provider for redundancy.
