# ADR-004: Owner-only admin pattern and whitelist access control

**Status:** Accepted

## Context

Two distinct trust boundaries exist in the protocol:

1. **Protocol admin** — deploys contracts, can fund projects, update impact scores. Must be highly restricted.
2. **Project creators** — third parties submitting green projects. Must be vetted before they can create projects, but not trusted with admin power.

Options for admin access control:

- **Multisig at the Stellar account layer** — admin is a Stellar account with multiple signers; threshold enforced by the network, not the contract.
- **Role list in contract** — contract stores a list of addresses with specific roles.
- **Single owner in contract** — contract stores one owner address; owner can be a multisig account.

Options for creator access control:

- **Open permissionless** — anyone can create a project.
- **NFT-gated** — creator must hold a specific NFT.
- **Whitelist** — a designated whitelister address approves creator addresses.

## Decision

**Admin control:** use the `stellar_access::ownable` crate's single-owner pattern. The `owner` is stored in instance storage; `#[only_owner]` macro guards admin functions (`fund_project`, `update_impact_score`). The deployer sets the owner at construction time and can transfer ownership.

**Creator access control:** use a dedicated `whitelister` address stored in instance storage. The whitelister calls `set_whitelist(account, true/false)` to approve or revoke creators. Only whitelisted addresses can call `create_project`.

Rationale:

- Single-owner is simple and auditable. Multisig complexity (threshold, key rotation) is handled at the Stellar account layer where it belongs, not duplicated in the contract.
- A separate whitelister role decouples day-to-day project onboarding from protocol admin. The admin can be a cold multisig; the whitelister can be a warmer operational key.
- A simple boolean whitelist is sufficient for the current scale. Graduated tiers or KYC attestation can be added later without breaking the existing interface.

## Consequences

**Positive:**

- `#[only_owner]` is a single-line, compiler-enforced guard. Hard to accidentally omit.
- Whitelister and owner can be different accounts, limiting blast radius if either is compromised.
- No on-chain role enumeration — no function to list all owners or whitelisters, reducing attack surface.

**Negative / trade-offs:**

- Single owner is a single point of failure if the owner key is lost. Mitigation: owner should be a Stellar multisig account with threshold ≥ 2.
- There is no on-chain timelock on `fund_project`. A compromised owner could immediately drain USDC to any project's registered owner address. Mitigation: use a multisig owner and monitor `project_funded` events.
- Whitelist revocation (`set_whitelist(addr, false)`) does not remove existing projects created by the revoked address. Existing projects remain valid.
