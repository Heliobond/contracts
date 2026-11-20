# Vault Economic Invariants

This document lists the invariants that must hold at all times for the
investment vault's accounting to be correct.  Every invariant is paired with
the code that is supposed to maintain it and the test that proves it.  This is
the artefact an auditor asks for first; it also allows the accounting issues in
the issue tracker to be reasoned about and closed as a group rather than one at
a time (#939).

---

## I-1  Share-price identity

```
total_supply × share_price ≈ total_assets
```

Where `share_price = total_assets / total_supply` (integer, rounded down).

**What maintains it:**
- `deposit` mints `usdc_amount × total_supply / total_assets` shares, keeping
  the ratio constant.
- `withdraw` burns `shares_amount` and pays `shares_amount × total_assets /
  total_supply` USDC.
- `record_yield` increments `TotalAssets`; `realize_loss` decrements it.

**What can violate it:**
- A virtual-offset asymmetry between minted shares and credited assets on the
  very first deposit (first-depositor attack vector).
- Double-counted yield increments (each `record_yield` call must be idempotent
  per epoch or guarded against double-dispatch).
- An insurance premium that is debited from `total_assets` but never matched
  by a corresponding debit from USDC.

**Test:** `invariant_share_price_identity` in
`investment_vault/src/invariants_test.rs`.

---

## I-2  Queue liability sum

```
queued_liabilities == Σ QueueEntry(i).usdc_owed   for i in [QueueHead, QueueTail)
```

The vault tracks the total USDC it owes to queued withdrawers.  This sum must
equal the sum of individual `QueuedClaim.usdc_owed` entries so that
`process_queue` can never pay out more than is owed.

**What maintains it:**
- `withdraw` (liquid-insufficient path) enqueues a `QueuedClaim` whose
  `usdc_owed` is the USDC equivalent at the time of enqueueing, and increments
  `QueuedLiabilities` by the same amount.
- `claim` dequeues the entry and decrements `QueuedLiabilities` by
  `entry.usdc_owed`.

**What can violate it:**
- Double-counting a single queue entry (the entry is enqueued twice, or
  `QueuedLiabilities` is incremented twice for the same withdrawal).
- A queue head/tail pointer bug that skips or replays an entry.

**Test:** `invariant_queue_liability_sum` in
`investment_vault/src/invariants_test.rs`.

---

## I-3  Investment reconciliation

```
TotalInvestments == Σ ProjectInvestment(project_id)
```

`TotalInvestments` is a cached aggregate.  Its value must always equal the sum
of every per-project investment stored under `VaultKey::ProjectInvestment`.

**What maintains it:**
- `invest` increments both `TotalInvestments` and `ProjectInvestment(id)` by
  the same amount.
- `record_repayment` decrements both by the repaid principal.

**What can violate it:**
- An update to `ProjectInvestment` that does not update `TotalInvestments`.
- A loss realisation that zeroes `ProjectInvestment` but does not subtract from
  `TotalInvestments`.

**Test:** `invariant_total_investments_sum` in
`investment_vault/src/invariants_test.rs`.

---

## I-4  Liquidity coverage

```
liquid_usdc + insurance_fund + outstanding_yield_debt <= usdc_balance
```

Where `usdc_balance` is the vault's actual USDC token balance on the ledger.
No accounting entry should reference USDC that the vault does not hold.

**What maintains it:**
- `deposit` transfers USDC in before crediting shares.
- `claim` transfers USDC out only after decrementing `QueuedLiabilities`.
- `insurance_fund_balance` is funded only from explicit `fund_insurance`
  calls, not from thin air.

**What can violate it:**
- A `.max(0)` guard that suppresses an underflow rather than surfacing it —
  the vault could report positive liquidity while the underlying USDC is
  already gone.
- An outstanding yield accumulator that grows past the USDC actually received.

**Surfaced by `health_check`:** the `utilization_bps` field in `HealthStatus`
already hints at this when `> 10_000`; a future `health_check` extension
should expose `liquid_usdc - insurance_fund - outstanding_yield_debt` directly.

**Test:** `invariant_liquidity_coverage` in
`investment_vault/src/invariants_test.rs`.

---

## I-5  Yield debt coverage

```
Σ claimable_yield(account) <= received_yield
```

The sum of all accounts' claimable yield must never exceed the USDC yield
actually deposited into the vault.  Violations mean the vault promises more
yield than it holds.

**What maintains it:**
- `record_yield` increments the per-share yield accumulator
  (`YieldAccumPerShare`) by `amount / total_shares` and must be called with
  the amount that was physically transferred in.
- `claim_yield` decrements the caller's `YieldDebt` by exactly what it pays
  out.

**What can violate it:**
- Rounding errors in `YieldAccumPerShare` that accumulate upward.
- A `record_yield` call that increments the accumulator without a matching
  USDC transfer.
- Three distinct `YieldDebt` accounting bugs filed separately in the issue
  tracker — all are violations of this invariant.

**Test:** `invariant_yield_debt_coverage` in
`investment_vault/src/invariants_test.rs`.

---

## I-6  Share price is non-decreasing except on loss realisation

```
share_price(t+1) >= share_price(t)   unless realize_loss was called between t and t+1
```

A share that is never sold into a loss should never be worth less than when it
was minted.  Specifically:
- `deposit` must not dilute existing holders (minting too many shares).
- `record_yield` increases `total_assets`, raising the share price.
- `donate` (direct USDC transfer to the vault address without minting) raises
  the share price — but the first depositor can exploit this to manipulate the
  initial price (see I-1).

**Test:** `invariant_share_price_nondecreasing` in
`investment_vault/src/invariants_test.rs`.

---

## Coverage in `health_check`

The table below shows which invariants are cheaply checkable on-chain in the
`health_check` entrypoint.  Those marked ✓ should be asserted there to give
operators an on-chain early-warning signal.

| Invariant | Checkable in `health_check`? | Cost |
|-----------|------------------------------|------|
| I-1 share-price identity | ✓ (compare `total_supply × price` vs `total_assets`) | O(1) |
| I-2 queue liability sum | ✗ (requires iterating the queue) | O(n) |
| I-3 investment reconciliation | ✗ (requires iterating projects) | O(n) |
| I-4 liquidity coverage | ✓ (token balance vs accounting sum) | O(1) |
| I-5 yield debt coverage | ✗ (requires iterating accounts) | O(n) |
| I-6 share price non-decreasing | ✓ (compare stored `LastSharePrice` vs current) | O(1) |

The O(1) checks (I-1, I-4, I-6) should be added to `HealthStatus` and
asserted by `health_check`.  The O(n) checks are unsuitable for on-chain
execution but are covered by the off-chain invariant test suite.

---

## References

- `investment_vault/src/lib.rs` — `deposit`, `withdraw`, `invest`,
  `record_yield`, `claim_yield`, `health_check`
- `investment_vault/src/types.rs` — `HealthStatus`, `QueuedClaim`
- `investment_vault/src/invariants_test.rs` — test suite for all six invariants
- Issue #939 — tracking issue for this document and the test suite
