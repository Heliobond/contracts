# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for the Heliobond smart contracts. Each ADR documents a significant architectural choice, the context that drove it, the options considered, and the trade-offs accepted.

## Index

| # | Title | Status | Date | Superseded by |
|---|-------|--------|------|---------------|
| [001](001-soroban-platform.md) | Use Soroban / Stellar for smart contracts | Accepted | 2026-06-26 | - |
| [002](002-storage-patterns.md) | Persistent vs instance storage partitioning | Accepted | 2026-06-26 | - |
| [003](003-share-vault-model.md) | ERC-4626-inspired share vault for investments | Accepted | 2026-06-26 | - |
| [004](004-security-model.md) | Owner-only admin pattern and whitelist access control | Accepted | 2026-06-26 | - |

## Decisions not yet recorded

These questions come up repeatedly in the issue tracker and have no ADR yet:

- Virtual-share offset versus dead shares for the vault's share accounting.
- The registry-versus-vault rate model split.
- Whether archived projects are frozen.
- The persistent-storage TTL policy.
- Whether `upgrade()` is in multisig scope.

Each can be a short ADR in the format below.

## When to write a new ADR

Write a new ADR when a change is architecturally significant — it's hard or costly to reverse, and future contributors would otherwise have to reconstruct the reasoning from git history or ask around. Concretely, that includes:

- Adopting, replacing, or dropping a platform, language target, or major dependency (e.g. the Soroban/Stellar choice in [001](001-soroban-platform.md)).
- Changing how contract state is modelled or partitioned across storage tiers (e.g. [002](002-storage-patterns.md)).
- Introducing or changing a core financial or accounting model (e.g. the share-vault design in [003](003-share-vault-model.md)).
- Changing the access-control or trust model — who can call privileged functions and how that's enforced (e.g. [004](004-security-model.md)).
- Any decision that trades off security, gas cost, or upgradeability in a way that isn't obvious from reading the code, and that a reviewer or auditor would need to know to evaluate the contracts correctly.

You do **not** need an ADR for:

- Bug fixes, gas optimisations, or refactors that don't change the decision itself (e.g. shortening a storage key name is a `gas-budgets.json` update, not an ADR).
- Adding a function, event, or admin parameter that follows an existing accepted pattern.
- Documentation, tooling, or CI changes.

When in doubt, prefer writing the ADR — a short record of a decision that turned out to be minor costs little, but a missing record for a significant one costs future contributors real time. If a later decision changes course, add a new ADR and mark the old one "Superseded by ADR-NNN" rather than editing it in place — ADRs are a historical record, not living documentation.

## Format

```
# ADR-NNN: Title

**Status:** Proposed | Accepted | Deprecated | Superseded by ADR-NNN

## Context
Why does this decision need to be made?

## Decision
What did we decide?

## Consequences
What are the trade-offs?
```
