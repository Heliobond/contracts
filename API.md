# Heliobond Contracts API Reference

> **This file is a convenience entry point.** For the authoritative,
> CI-verified contract API documentation, see:
>
> - **[INTERFACE.md](./INTERFACE.md)** — full function signatures, types,
>   error codes, and events for both `ProjectRegistry` and `InvestmentVault`.
> - **[CONTRACTS.md](./CONTRACTS.md)** — architecture, cross-contract flow,
>   and upgrade procedures.
>
> The sections below provide a brief orientation; defer to INTERFACE.md for
> exact parameter types and error names.

---

## ProjectRegistry

Manages project lifecycle, certification, reputation, and collateral.

**Key operations:**
- `create_project` — register a new project (requires whitelisted creator)
- `get_project` — read a project's on-chain state
- `update_impact_score` / `update_credit_quality_score` — oracle-driven score updates
- `certify_project` — set certification status (Pending / Certified / Revoked)
- `create_proposal` / `cast_vote` / `execute_proposal` — governance

**Error enum:** `RegistryError` (see INTERFACE.md for all 40 variants)

---

## InvestmentVault

Handles deposits, withdrawals, project funding, yield distribution, and
secondary-market features.

**Key operations:**
- `deposit` — deposit underlying tokens, receive vault shares
- `withdraw` — burn shares, reclaim underlying tokens (with slippage protection)
- `fund_project` — allocate vault funds to a registered project
- `claim_yield` / `claim_insurance` — distribute returns

**Error enum:** `VaultError` (see INTERFACE.md for all 58 variants)

---

## General Notes

- All base token values use **7 decimal places** (Stellar USDC standard).
- Contracts will **panic** on arithmetic overflow or if SDK constraints are
  violated — always simulate before submitting.
