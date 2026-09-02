# ADR-002: Persistent vs instance storage partitioning

**Status:** Accepted

## Context

Soroban offers three storage tiers:

| Tier | Lifetime | Cost | Use case |
|------|----------|------|----------|
| Instance | Lives as long as the contract instance | Cheapest reads | Config set once, read often |
| Persistent | Survives as long as rent is paid | Moderate | Long-lived per-entity state |
| Temporary | Automatically expires after TTL | Cheapest writes | Short-lived scratch state |

Every persistent entry has a TTL that must be extended (rent paid) or the entry is evicted. Incorrect partitioning means either paying unnecessary rent or losing data.

## Decision

**Instance storage** holds contract-level configuration that never changes after deployment:
- `VaultKey::UsdcSac` — the USDC SAC address
- `VaultKey::Registry` — the ProjectRegistry contract address
- `DataKey::Whitelister` — the whitelister address
- `DataKey::ProjectCounter` — the auto-increment project ID (updated on every `create_project`)

Rationale: instance storage is bumped automatically when any function is invoked on the contract, so no explicit TTL management is needed for these entries.

**Persistent storage** holds per-entity state that must outlive individual invocations:
- `DataKey::Project(id)` — project metadata
- `DataKey::Whitelist(addr)` — per-address whitelist status
- `VaultKey::ProjectInvestment(id)` — USDC invested per project
- `VaultKey::TotalInvestments` — aggregate investment counter

Rationale: project records and investment ledgers must survive indefinitely. Writes do **not** auto-extend TTL beyond the network minimum; each persistent write must call `extend_ttl` (vault: `storage::set_persistent`; registry: `write_project`). The CI enforces that contract size stays small so deployment + rent costs remain low.

**Temporary storage** is not currently used. It would be appropriate for short-lived proof-of-intent or nonce entries if added in future.

## Consequences

**Positive:**
- No manual TTL calls needed for instance-stored config.
- Clean separation: adding a new config value → instance; adding a new per-entity record → persistent.

**Negative / trade-offs:**
- Persistent entries can still be evicted if a key is never rewritten for longer than its remaining TTL (~30 days after the last write under the current policy). Operators must invoke a state-changing entrypoint that rewrites the relevant keys, or restore archived entries.
- `ProjectCounter` in instance storage means it is trivially readable but also updated on every project creation, slightly increasing instance storage cost over time (Soroban charges for updated bytes).
