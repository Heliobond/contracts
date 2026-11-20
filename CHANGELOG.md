# Changelog

All notable changes to this project are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Starting from v1.0.0 this file is generated and committed automatically by
`.github/workflows/changelog.yml` on every tag push.

---

## [1.0.0] – 2026-11-20

Contracts reach a stable public API.  `CONTRACT_VERSION = "1.0.0"` in
`project_registry` now matches a real release.

### Added

- `get_compliance_events_page` with continuation-sequence pagination (#1053)
- Oracle role dedicated to score updates (#748)
- Queue excess withdrawals at high utilization; split pause flags with expiry (#747, #750)
- On-chain vote weight, whitelist expiry, and timelock for privileged ops (#749–#754)
- Dependabot configuration for Cargo, npm, and GitHub Actions (#679)
- Repo-side secret scanning (Gitleaks): push-protection hook, CI scan (#656)
- `cargo-deny` supply-chain checks: advisories, bans, licenses, sources
- `contractmeta!` build metadata and CI build attestations (#751)
- npm dist-tags for stable/prerelease SDK channels (#1027)
- Wave malware indicator screening for contributions (#977)
- Compliance events page endpoint with cap and boundary tests (#700)
- RPC retention-gap detection: checkpoint advances past the gap, operator log emitted (#986)

### Changed

- Changelog workflow runs serialised with a repo-wide concurrency group (#672)
- Notification service Node version pinned to 20–22; read from `.nvmrc` (#685)
- Rust toolchain bumped to 1.91.0 so WASM builds resolve the lockfile
- `cargo-audit` pinned to 0.23.0, `cargo-deny` to 0.21.0 (#1043)
- CI caches notification-service npm downloads and bounds job execution (#983)

### Fixed

- Score-history ordering: ledger time now advances between updates
- Deposit slippage/deadline, share-inflation protection, insurance NAV exclusion,
  expected-returns caching (#743–#746)
- Transfer restrictions, no lock on incoming transfers, `total_assets` pure read,
  deposit cache investable (#739–#742)
- Registry page-size cap, status-transition enforcement, `created_at` preservation,
  unbounded-iterator removal (#767–#770)
- YieldDebt settled on mint/transfer; unclaimed yield excluded from NAV
- Flash-loan revenue accounting, overflow checks, multisig upgrade, collateral TTL (#705–#709)
- Governance quorum, vote-weight docs, TTL extension, bridge address round-trip (#755–#758)
- Collateral checks, repay refunds, multisig error, emergency unpause guard (#759–#762)
- VAA replay protection, amount validation, typed errors, claim reserves (#759–#762)
- Liquidate-collateral gates, duplicate batch guard, fee-tier validation, compliance field bounds
- Multisig tests, whitelist check, min description, reject negative threshold (#711–#716)
- CI input injection, governance tests, multisig tests, expected-returns tests (#730–#734)
- Testnet smoke-test workflow repair (#982)
- Contracts CI security and compile failures (#980)
- `get_compliance_events` capped at 100 entries (#700)
- `claim_insurance` only pays settled, impaired projects (#700)
- Registry pagination, status archiving (#769)
- WASM hash checker rejects duplicate and empty contract names (#1039)
- Cargo fmt alignment (#667)

### Security

- Checked arithmetic throughout vault and registry entry-points
- Emergency unpause guard prevents re-entry during active incident
- Whitelist expiry enforced on-chain for stale approvals

---

## [0.1.0] – 2026-06-15

Initial public preview. Core contracts (`investment_vault`, `project_registry`)
deployed to Stellar testnet. Notification service wired to on-chain events.
