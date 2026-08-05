# ADR-001: Use Soroban / Stellar for smart contracts

**Status:** Accepted

## Context

Heliobond finances green infrastructure projects through tokenised investment. The protocol needs a smart-contract platform that is:

- Cheap to deploy and invoke (projects are climate-focused; high gas costs are reputationally inconsistent)
- Safe for financial contracts (deterministic execution, formal fee model)
- Able to handle real-world assets denominated in a stable coin (USDC via Stellar's asset model)
- Accessible to the Stellar ecosystem's existing user base and liquidity

Alternatives considered: Ethereum/EVM, CosmWasm (Cosmos), Near.

## Decision

Use **Soroban** on the **Stellar** network.

Key reasons:

1. **USDC native support** — Stellar's native USDC SAC (Stellar Asset Contract) is the protocol's unit of account. Using Soroban means zero bridging risk.
2. **Predictable fees** — Soroban uses a resource-fee model (instructions, ledger entries, bytes). Fees are low and bounded, which matters for small green-bond tranches.
3. **`no_std` Rust** — Contracts are compiled to `wasm32v1-none` (WASM 2.0) with no heap allocator. The constrained environment eliminates whole classes of memory-safety bugs.
4. **Stellar ecosystem alignment** — The target LP and project-owner audience already uses Stellar wallets (Freighter, Lobstr). No bridge onboarding friction.

## Consequences

**Positive:**

- Very low invocation cost (fractions of a cent per call).
- Soroban's host-managed storage tiers (instance / persistent / temporary) give predictable data lifecycle without manual expiry logic.
- `stellar_tokens` and `stellar_access` crates provide audited primitives for fungible tokens and ownership.

**Negative / trade-offs:**

- Smaller developer ecosystem than EVM; fewer ready-made audit firms.
- No native multi-sig in contracts (must be handled at the Stellar account layer).
- `no_std` limits available Rust crates; anything requiring `std` must be avoided or rewritten.
- WASM size budget (~128 KiB) requires careful dependency management.
