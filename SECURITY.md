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
