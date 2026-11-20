# Security policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report privately through GitHub: go to the repository's **Security** tab → **Report a vulnerability** (this opens a private advisory). If you can't use that, email **daveproxy80@gmail.com**.

Include what you can: affected component, steps to reproduce, and impact. We aim to acknowledge within a few days and will coordinate a fix and disclosure with you.

## Severity Definitions

Use these when filing a report so the team can triage immediately.

| Severity | Definition | Examples from this codebase |
|---|---|---|
| **Critical** | Direct loss or theft of user funds with no special preconditions beyond calling a contract function. | Arithmetic overflow in `deposit`/`withdraw` share calculation that lets an attacker drain the vault; auth bypass on a fund-moving entry point. |
| **High** | Loss or lock of funds that requires a privileged position (e.g. a malicious registry, an influenceable oracle), or a complete bypass of auth on a state-mutating entry point. | Malicious registry returning inflated scores to inflate NAV and over-issue shares (accepted-risk path documented in `investment_vault/src/lib.rs:4–17`); `set_registry` callable without admin auth. |
| **Medium** | An attacker can cause material but bounded harm or extract value only under unusual conditions; denial-of-service making the vault temporarily unusable; privilege escalation that does not directly move funds. | Griefing via spam registrations that exhaust ledger entry limits; an event emitting an incorrect amount that misleads off-chain indexers. |
| **Low / Informational** | No direct financial impact; deviations from best practice, documentation gaps, or issues that require admin cooperation to exploit. | A getter leaking non-sensitive internal state; a missing `extend_ttl` that could cause storage expiry on a low-traffic deployment. |

## Scope

This is testnet, pre-production software. The smart contracts have not yet been audited. Treat anything on-chain as experimental until a release notes otherwise.

### In scope

- **InvestmentVault** (`investment_vault/`) — all public entry points: `deposit`, `withdraw`, `receive_yield`, `set_registry`, `pause`/`unpause`, admin functions
- **ProjectRegistry** (`project_registry/`) — all public entry points: `register_project`, `update_project`, `whitelist`/`delist`, score reads
- **Cross-contract trust path** — the vault's reliance on registry-supplied `ProjectData`, scores, and `total_projects()` for NAV calculation; a report demonstrating that a reachable code path lets a malicious or substituted registry cause the vault to mis-price shares or over-issue them is **in scope and treated as High**
- **Oracle / score ingestion** — any path where score data flows into vault NAV or yield calculations and can be manipulated without compromising the oracle operator's key
- **Auth and pausability** — missing `require_auth` or `require_not_paused` on state-mutating entry points
- **Arithmetic correctness** — overflow, truncation, or rounding that affects deposit/withdraw/yield payout amounts

### Out of scope

- **Owner-key / admin-key compromise** — the protocol makes no claim about key-management security; a report whose only attack vector is "the admin key is stolen" will be treated as informational
- **Wormhole guardian-set compromise** — the bridge's guardian consensus is an upstream dependency; vulnerabilities in Wormhole's own guardian set should be reported to the Wormhole team
- **Testnet-only deployments** — until a mainnet release is announced, all deployed contracts are experimental; economic exploits on testnet tokens are out of scope
- **Third-party dependencies** — vulnerabilities in `soroban-sdk`, `stellar-xdr`, or other upstream crates should be reported to their respective maintainers; we will update promptly once patches are available
- **Social engineering / phishing** targeting team members or users

## Dependency Updates

[`.github/dependabot.yml`](.github/dependabot.yml) opens weekly update PRs for the Cargo workspace, the `notification-service` and `sdk` npm packages, and GitHub Actions. `soroban-sdk` and the exact-pinned `stellar-*` crates are grouped into a single PR because they must be upgraded together. Treat these PRs like any other: CI (including `cargo audit`) must be green before merging.

Maintainers should also keep **Dependabot alerts** and **Dependabot security updates** enabled under *Settings → Code security*; that setting cannot be turned on from a file in the repo.

## Threat Model and Trust Assumptions

### Trust Boundaries

There are three distinct trust boundaries in this codebase.

**1. Owner / admin key**
Administrative functions (`set_registry`, `pause`, `upgrade`, fee adjustments) are gated to a single owner address assumed to be uncompromised. Reports whose only attack surface is a stolen admin key are out of scope.

**2. ProjectRegistry → InvestmentVault data path**
The vault trusts the registry to return valid `ProjectData` including legitimate owner addresses and accurate scores used in NAV calculation. The registry address is validated at construction via `total_projects()` and can only be changed by the admin. As noted in `investment_vault/src/lib.rs:4–17`, a compromised or malicious registry could return manipulated data ("Consider using a registry interface trait" is a listed mitigation — not yet implemented). **This path is in scope.** A report demonstrating that a reachable code path allows a malicious registry to inflate NAV or over-issue shares is treated as High severity.

**3. External oracles and bridge (Wormhole)**
The vault assumes any configured oracle provides accurate and timely pricing. Wormhole guardian-set compromise is an upstream risk reported to the Wormhole team, not here. Oracle price-feed manipulation achievable without compromising the guardian set itself (e.g. a permissioned feed update or a vulnerable feed-aggregation path) is **in scope**.

### Known Limitations
- The contracts currently rely on a centralized whitelister for project creation.
- Maximum URI lengths and specific string size bounds are strictly enforced to prevent ledger bloat.
- The registry–vault trust boundary has not yet been hardened with an interface trait; this is accepted pre-audit risk.

## Pre-Merge Checklist for New Contract Functions

Before merging a PR that adds or modifies a public entry point on either
contract, walk through this checklist (#265):

- [ ] **Auth checks**: Does this function require the right caller's
      authorization? `#[only_owner]` for admin-only actions, an explicit
      `caller.require_auth()` for actions gated to a specific non-admin
      party (e.g. a project owner). Confirm the check happens *before* any
      state mutation, not after. If the function is admin-only, add it to
      `test_all_only_owner_functions_reject_non_admin_caller` in the
      relevant crate's `test.rs` (#266) — don't rely solely on a one-off
      test, since that consolidated test is what catches an entry point
      that's accidentally left ungated.
- [ ] **Pausability**: Should this function be blocked while the contract is
      paused? If it mutates state (not a pure getter), it almost certainly
      should call `require_not_paused` / `require_current_state`. Getters
      are typically allowed to keep working while paused.
- [ ] **Overflow / bounds checks**: Validate amounts are positive where
      negative or zero doesn't make sense; check scores/percentages are
      within their documented range (e.g. 0..100, 0..10_000 bps); confirm
      arithmetic (multiplication in particular) can't silently overflow
      `i128`/`u32` for the value ranges the function accepts.
- [ ] **Event emission**: Does the function emit a `#[contractevent]` on
      success, so off-chain indexers/monitoring can observe the state
      change? See `EVENTS.md` for the existing catalogue and naming
      conventions.
- [ ] **Storage rent**: New persistent storage entries need an explicit TTL
      extension (`extend_ttl`) at write time; instance storage doesn't need
      this but is billed on every ledger close regardless of use, so prefer
      persistent storage for anything per-project/per-address.
- [ ] **Interface docs**: Add the function to `INTERFACE.md`'s function
      table for its contract, and update `ProjectData`/other type docs if
      you changed a struct. `scripts/check_interface_docs.py` (wired into CI,
      #273) will fail the build if you forget — but it only catches missing
      *names*, not incorrect auth/return/notes columns, so still write them
      accurately by hand.
- [ ] **Tests**: A happy-path test, at least one negative/panic test for
      the primary validation failure mode, and (if admin-gated) an entry in
      the consolidated admin-only test mentioned above.

## Security Best Practices for Integrators

1. **Verify Contract State**: Always query the latest on-chain state before executing critical transactions.
2. **Handle Errors Gracefully**: Expect and handle custom contract errors (`RegistryError`, `VaultError`) appropriately in your dApp.
3. **Validate Inputs**: While the contracts perform internal validation, integrators should also validate user inputs (e.g., URIs, amounts) on the client side to provide better UX and avoid unnecessary transaction fees.
4. **Monitor Events**: Listen to contract events to keep off-chain state synchronized with on-chain actions.

## Incident Response Procedures

If a critical vulnerability is discovered and verified:
1. **Triage**: The core team will assess the severity and potential impact within 24 hours.
2. **Mitigation**: If necessary and feasible, administrative functions may be used to pause certain contract operations to prevent further exploitation.
3. **Patch & Deploy**: A fix will be developed, tested, and deployed as a contract upgrade.
4. **Disclosure**: A post-mortem will be published detailing the vulnerability, its impact, and the steps taken to resolve it.
