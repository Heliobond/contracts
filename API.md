# Heliobond Contracts API Reference

This document provides a comprehensive API reference for the two core smart contracts: `ProjectRegistry` and `InvestmentVault`.

## ProjectRegistry

The `ProjectRegistry` contract manages project lifecycle, certification, reputation, and collateral.

### Core Data Types

- **`ProjectData`**: Represents a project's state.
  ```rust
  pub struct ProjectData {
      pub owner: Address,
      pub uri: String,
      pub credit_quality: u32,
      pub green_impact: u32,
      pub maturity_date: u64,
      pub certification_status: CertificationStatus,
      pub last_update_timestamp: u64,
      pub archived: bool,
  }
  ```
- **`CertificationStatus`**: `None`, `Pending`, `Certified`, `Revoked`.
- **`Proposal`**: 
  ```rust
  pub struct Proposal {
      pub description: String,
      pub proposer: Address,
      pub voting_ends_at: u64,
      pub votes_for: i128,
      pub votes_against: i128,
      pub executed: bool,
  }
  ```

### Key Functions

#### `create_project(env: Env, creator: Address, uri: String, maturity_date: u64) -> u32`
Creates a new project. 
- **Auth**: `creator` must authorize.
- **Parameters**: 
  - `creator`: Project owner. Must be whitelisted.
  - `uri`: Project metadata URI.
  - `maturity_date`: Future Unix timestamp (0 for open-ended).
- **Errors**: `NotWhitelisted`, `UriTooShort`, `UriTooLong`, `InvalidUriScheme`, `MaturityDateInPast`.
- **Example Usage**:
  ```javascript
  const tx = await contract.invoke({
    method: "create_project",
    args: [creator, "https://example.com/project1", 0]
  });
  ```

#### `get_project(env: Env, id: u32) -> ProjectData`
Returns the state of a project.
- **Errors**: `ProjectNotFound`.

#### `update_impact_score(env: Env, project_id: u32, credit_quality: u32, green_impact: u32)`
Updates the impact score (admin only).
- **Auth**: Admin.
- **Errors**: `ProjectNotFound`.

#### `deposit_collateral(env: Env, project_id: u32, depositor: Address, token: Address, amount: i128)`
Deposits collateral for a project.
- **Auth**: `depositor`.
- **Errors**: `ProjectNotFound`, `AmountMustBePositive`.

#### `certify_project(env: Env, project_id: u32, status: CertificationStatus)`
Updates a project's certification status.
- **Auth**: Admin.
- **Errors**: `ProjectNotFound`.

## InvestmentVault

The `InvestmentVault` contract handles funding projects, claiming yields, and withdrawing investments.

### Core Data Types

- **`VaultConfig`**:
  ```rust
  pub struct VaultConfig {
      pub admin: Address,
      pub registry_address: Address,
      pub token_address: Address,
      pub performance_fee_bps: u32,
      pub vault_cap: i128,
  }
  ```
- **`InvestmentStrategy`**: `RiskAverse`, `Balanced`, `HighGrowth`.

### Key Functions

#### `deposit(env: Env, caller: Address, amount: i128)`
Deposits underlying tokens into the vault and mints shares.
- **Auth**: `caller`.
- **Parameters**: `amount` to deposit.
- **Errors**: `VaultIsPaused`, `AmountMustBePositive`, `VaultCapExceeded`.
- **Example Usage**:
  ```javascript
  const tx = await vault.invoke({
    method: "deposit",
    args: [caller, 100000000] // 10 tokens with 7 decimals
  });
  ```

#### `withdraw(env: Env, caller: Address, share_amount: i128)`
Burns shares and returns underlying tokens.
- **Auth**: `caller`.
- **Errors**: `VaultIsPaused`, `AmountMustBePositive`, `InsufficientShares`.

#### `fund_project(env: Env, project_id: u32, amount: i128)`
Funds a registered project (admin only).
- **Auth**: Admin.
- **Errors**: `VaultIsPaused`, `InsufficientVaultFunds`, `ProjectNotCertified`.

## General Considerations & Panics
- All base token values have 7 decimal places unless noted.
- Contract will panic on arithmetic overflow or if SDK constraints are violated.
