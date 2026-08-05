# Governance Model

This document outlines the governance model of the Heliobond platform, detailing the current administrative capabilities, whitelister roles, the voting mechanism, security considerations, future plans for decentralization, and community participation guidelines.

---

## Current Role-Based Access Control

Heliobond contracts distinguish between three primary roles: **Admin (Owner)**, **Whitelister**, and **Project Creators (Whitelisted)**. This structure is designed to decouple contract administration from operational tasks.

### 1. Admin (Owner)

The Admin role manages critical protocol configuration and capital allocation.

- **Implementation:** Employs the `stellar-access::ownable` single-owner pattern.
- **Ownership Transfer:** Follows a secure 2-step transfer process (`transfer_ownership` followed by `accept_ownership` from the new owner) to avoid accidental transfer to incorrect addresses.
- **Visibility:** Emits custom, project-specific `OwnershipTransferred` events in both contracts, ensuring that ownership change proposals are auditable off-chain.

#### Admin Capabilities

| Action                        | Contract          | Description                                                                                             |
| ----------------------------- | ----------------- | ------------------------------------------------------------------------------------------------------- |
| `update_impact_score`         | `ProjectRegistry` | Sets `credit_quality` and `green_impact` scores (0–100) for a project.                                  |
| `update_credit_quality_score` | `ProjectRegistry` | Updates only the `credit_quality` score, preserving the existing `green_impact` score.                  |
| `certify_project`             | `ProjectRegistry` | Updates the certification status of a registered project (shares this capability with the Whitelister). |
| `fund_project`                | `InvestmentVault` | Disburses capital from the vault to the registered project creator's address.                           |
| `receive_yield`               | `InvestmentVault` | Registers interest/yield payments received from project owners.                                         |
| `claim_insurance`             | `InvestmentVault` | Authorizes default insurance payouts to affected investors from the insurance reserve.                  |
| `set_management_fee`          | `InvestmentVault` | Sets the vault management fee (hard-capped at 5.00% / 500 bps).                                         |
| `enable_secondary_trading`    | `InvestmentVault` | Enables DEX listing discovery and updates official secondary market listing status.                     |
| `pause` / `unpause`           | Both              | Temporarily freezes deposits, withdrawals, and proposal voting under emergency conditions.              |

### 2. Whitelister

An operational role focused on onboarding project creators and certifying projects. Separating this role prevents daily tasks from requiring the high-security Admin key.

#### Whitelister Capabilities

| Action            | Contract          | Description                                                                                       |
| ----------------- | ----------------- | ------------------------------------------------------------------------------------------------- |
| `set_whitelist`   | `ProjectRegistry` | Approves or revokes creator addresses, granting or denying project registration rights.           |
| `certify_project` | `ProjectRegistry` | Updates the certification status of a registered project (shares this capability with the Admin). |

### 3. Project Creators (Whitelisted)

- Must be explicitly whitelisted by the Whitelister.
- Can call `create_project` to register new project metadata (IPFS URI and maturity date) on-chain.

---

## Multisig Approval Threshold Configuration

Both contracts support an optional multisig gate on their most sensitive Admin actions — `InvestmentVault::fund_project` / `batch_fund_projects` / `claim_insurance`, and `ProjectRegistry::update_impact_score_approved` — as the on-chain implementation of the "Phase 1: Multisig Administration" step in the roadmap below. This is separate from `stellar-access::ownable` (which is a single `Address`); the multisig config is its own signer list + threshold stored per-contract.

### How it's configured

Each contract exposes the same three functions (owner-only unless noted):

| Function                                 | Contract          | Effect                                                                                 |
| ---------------------------------------- | ----------------- | -------------------------------------------------------------------------------------- |
| `set_multisig_admin(signers, threshold)` | Both              | Sets the approver list and required approval count. `#[only_owner]`.                   |
| `get_multisig_admin()`                   | Both              | Returns `(signers, threshold)`. No auth required — anyone can read the current config. |
| `clear_multisig_admin()`                 | `ProjectRegistry` | Resets `threshold` to `0` and `signers` to empty, disabling multisig. `#[only_owner]`. |

`set_multisig_admin` validates the config before storing it:

- `signers.len()` must not exceed `MAX_MULTISIG_SIGNERS` (10 in both contracts) — panics with `TooManyMultiSigSigners` otherwise.
- `threshold` must be greater than `0` and no greater than `signers.len()` — panics with `InvalidMultiSigThreshold` otherwise (so you cannot require more approvals than there are signers, and cannot set a threshold with no signers).
- `signers` must not contain duplicate addresses — panics with `DuplicateApproval` otherwise.

### How it changes call behaviour

Once `threshold > 0`, the plain single-owner variant of a gated function (e.g. `fund_project`, `claim_insurance`) becomes unusable — it panics via an internal `require_multisig_disabled` guard. Callers must switch to the `_with_approvals` variant instead (e.g. `fund_project_with_approvals`, `claim_insurance_with_approvals`, `update_impact_score_approved`), passing a `Vec<Address>` of the approving signers. For each address in that list, `require_admin_approval`:

1. Panics with `DuplicateApproval` if it already appeared earlier in the same list.
2. Panics with `NotMultiSigSigner` if it isn't in the stored `signers` set.
3. Calls `.require_auth()` on it — every listed approver must independently authorize the transaction, not just be named in the list.

If fewer than `threshold` addresses pass all three checks, it panics with `InsufficientApprovals`.

While `threshold == 0` (the default, unset state), call the plain variant (e.g. `fund_project`) — the `_with_approvals` variant's `require_admin_approval` would still work in this state too (it falls back to a plain `require_auth()` on the contract owner when `threshold == 0`), but the plain variant is what `require_multisig_disabled` expects while multisig is off. Both variants call the same internal implementation (e.g. `fund_project_internal`); only the auth gate in front of it differs.

### Changing the threshold or signer set

There is no separate "update" function — call `set_multisig_admin` again with the full new `signers` list and `threshold`; it overwrites the previous config atomically. To go from, say, a 2-of-3 to a 3-of-5 setup, submit one `set_multisig_admin` call with all 5 signers and `threshold: 3`.

> [!WARNING]
> **`InvestmentVault` cannot disable multisig once enabled.** `validate_multisig_config` rejects `threshold == 0` outright (`InvalidMultiSigThreshold`), and unlike `ProjectRegistry`, `InvestmentVault` has no `clear_multisig_admin()` function. Once you call `set_multisig_admin` on the vault with a real threshold, every `_with_approvals`-gated action (`fund_project`, `batch_fund_projects`, `claim_insurance`) requires that many approvals permanently — there's no on-chain path back to single-owner operation. `ProjectRegistry` doesn't have this limitation: call `clear_multisig_admin()` there to reset `threshold` to `0` and re-enable the plain owner-only functions. Treat enabling multisig on the vault as a one-way migration, and confirm the signer set before enabling it.

Because `set_multisig_admin` is itself `#[only_owner]` and not gated by the multisig it configures, the single-owner key retains ultimate control over the multisig roster — see the "Phase 1" note below on migrating that owner key to a genuine Stellar multisig account for defense in depth.

---

## On-Chain Proposal & Voting Mechanism

To prepare the platform for future decentralization, a preliminary governance proposal system is built directly into the `ProjectRegistry` contract.

### 1. Proposal Creation (`create_proposal`)

- **Eligibility:** Any whitelisted address may propose a governance change.
- **Parameters:** Requires a text description and a voting period.
- **Constraint:** The voting duration must be at least `MIN_VOTING_PERIOD` (86,400 seconds / 24 hours) to prevent flash proposals.

### 2. Casting Votes (`cast_vote`)

- **Eligibility:** Any token holder can vote.
- **Mechanism:** Votes are cast as either `support` (for) or `against`.
- **Voting Weight:** A voter's weight corresponds to their HBS (Heliobond Shares) balance.

> [!WARNING]
> **On-Chain Voting Weight Limitation**
>
> In the current version, the `cast_vote` function takes the vote `weight` as a direct parameter supplied by the caller, **without verifying it against the actual HBS token balance on-chain**.
>
> - **Current Mitigation:** Off-chain clients and indexers must query the `InvestmentVault` contract via `balance(voter)` during simulation and submit the correct value. Any proposal executed with invalid or inflated vote weights must be filtered out or rejected during off-chain validation before executing any manual steps.
> - **Future Fix:** A cross-contract call from `ProjectRegistry` to `InvestmentVault::balance(voter)` will be integrated into `cast_vote` to enforce the voting weight programmatically.

### 3. Proposal Execution (`execute_proposal`)

- **Eligibility:** Anyone may trigger execution once the voting period has elapsed.
- **Rule:** The proposal passes if `votes_for > votes_against`.
- **State Change:** The proposal is marked as `executed` to prevent double-execution or late voting.
- **Impact:** In the current phase, proposal execution is informational/social (signaling consensus) and does not automatically trigger state changes in contract configurations.

---

## Future Governance & Decentralization Roadmap

As the platform matures, governance will transition from centralized control to a community-led DAO structure:

```mermaid
graph TD
    Phase1["Phase 1: Multisig Administration\n(Current/Near-term)\n- Admin key held by Multi-Signature Account\n- Safe configuration threshold setup"]
    Phase2["Phase 2: Hybrid Governance\n- Cross-contract voting weight verification\n- Whitelist onboarding voted by HBS holders"]
    Phase3["Phase 3: Full DAO Autonomy\n- Transition to Governor contracts\n- Proposals automatically execute on-chain payload"]
    Phase1 --> Phase2
    Phase2 --> Phase3
```

1. **Multisig Control (Phase 1):** The single owner address of both contracts will be assigned to a Stellar multi-signature account. This requires multiple keys to sign any administrative transaction (like `fund_project` or `update_impact_score`), eliminating single points of failure.
2. **On-Chain Verification (Phase 2):** Integrate cross-contract checks in `cast_vote` to fetch and verify `weight` from the HBS token contract.
3. **Autonomous DAO (Phase 3):** Transition to a decentralized Governor contract architecture. Under this model, passed proposals will programmatically execute payload transactions (e.g., updating interest rates, changing whitelisters) without relying on a centralized administrator to perform the transaction.

---

## Community Participation Guidelines

Community members can participate in Heliobond governance through the following channels:

- **Holding HBS:** Acquisition of HBS shares grants voting power. The larger your share of the pool, the more influence your votes carry.
- **Submitting Proposals:** Whitelisted creators can propose updates to the protocol rules, fees, or whitelisting guidelines.
- **Auditing Protocol Operations:** Because every governance action emits structured events (`ProposalCreated`, `VoteCast`, `ProposalExecuted`, `OwnershipTransferred`), users can run independent indexers to monitor and verify all administrative decisions.
