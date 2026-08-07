# Security policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report privately through GitHub: go to the repository's **Security** tab → **Report a vulnerability** (this opens a private advisory). If you can't use that, email **daveproxy80@gmail.com**.

Include what you can: affected component, steps to reproduce, and impact. We aim to acknowledge within a few days and will coordinate a fix and disclosure with you.

## Scope

This is testnet, pre-production software. The smart contracts have not yet been audited. Treat anything on-chain as experimental until a release notes otherwise.

## Threat Model and Trust Assumptions

### Trust Boundaries

- **Project Registry & Investment Vault**: These contracts trust each other explicitly for interoperability where documented. Administrative functions are restricted to a multi-sig or single highly-trusted admin key.
- **Oracles and External Data**: We assume our selected oracles (if any) provide accurate and timely data. Any compromise of the oracle may lead to incorrect valuations or interest rate calculations.
- **End Users**: Users are responsible for securing their own private keys. The contracts do not have a mechanism to recover funds sent to the wrong address or lost due to compromised keys.

### Known Limitations

- The contracts currently rely on a centralized whitelister for project creation.
- Maximum URI lengths and specific string size bounds are strictly enforced to prevent ledger bloat.

## Pre-Merge Checklist for New Contract Functions

Before merging a PR that adds or modifies a public entry point on either
contract, walk through this checklist (#265):

- [ ] **Auth checks**: Does this function require the right caller's
      authorization? `#[only_owner]` for admin-only actions, an explicit
      `caller.require_auth()` for actions gated to a specific non-admin
      party (e.g. a project owner). Confirm the check happens _before_ any
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
      _names_, not incorrect auth/return/notes columns, so still write them
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
