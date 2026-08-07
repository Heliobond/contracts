# ADR-003: ERC-4626-inspired share vault for investments

**Status:** Accepted

## Context

The protocol accepts USDC from LPs and deploys it into green projects. LPs need a fungible, transferable claim on the pool's assets so they can exit without waiting for project repayment. Options considered:

1. **Simple ledger** — track each LP's USDC deposit directly; withdrawals return exact deposits plus a separate yield claim.
2. **Share token (vault model)** — mint HBS (Heliobond Shares) proportional to the LP's share of total assets; share price rises as investments earn returns.
3. **NFT receipts** — each deposit mints a unique NFT with its own accounting.

## Decision

Use a **share token vault** modelled on ERC-4626 (`convert_to_shares` / `convert_to_assets` / `deposit` / `withdraw`).

Key reasons:

1. **Composability** — HBS is a standard fungible token (SEP-41). Any Stellar DEX or lending protocol can integrate it without custom adapters.
2. **Proportional yield** — All LPs automatically benefit from investment returns through a rising share price; no per-LP yield distribution loop.
3. **Atomic entry/exit** — Deposit mints shares and withdraw burns shares in a single transaction. No separate claim step.
4. **Established pattern** — ERC-4626 has been battle-tested across hundreds of EVM vaults. The logic is well-understood and auditable.

`total_assets` = liquid USDC + invested USDC + expected returns (credit quality × green impact score).

## Consequences

**Positive:**

- Share price naturally incorporates all value in the vault including projected returns.
- LPs can trade HBS on any SEP-41-compatible DEX as a secondary exit.
- First depositor receives 1:1 shares (guarded by `total_shares == 0 || total_assets == 0` check).

**Negative / trade-offs:**

- Share price depends on `get_expected_returns`, which reads every project in the registry. This is O(n) in the number of projects; gas cost grows linearly. A future optimisation may maintain a running expected-return accumulator.
- The share model means early LPs dilute later LPs if returns are recognised before new deposits — this is standard vault behaviour but must be communicated clearly to users.
- Rounding is in favour of the vault (integer truncation), which may leave tiny dust amounts unclaimable.
