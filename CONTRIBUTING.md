# Contributing to Heliobond contracts

These are the Soroban smart contracts behind Heliobond — the `investment_vault` and `project_registry`, written in Rust. Thanks for helping out.

## Pick something to work on

Browse [open issues](https://github.com/heliobond/contracts/issues). Issues tagged **good first issue** are scoped for newcomers; **help wanted** are ready for anyone. Each issue has scope, acceptance criteria, and file pointers. Comment to claim it before you start.

---

## Development environment

### Prerequisites

| Tool        | Version         | Install                                                                     |
| ----------- | --------------- | --------------------------------------------------------------------------- |
| Rust        | stable (≥ 1.78) | `rustup update stable`                                                      |
| wasm target | `wasm32v1-none` | `rustup target add wasm32v1-none`                                           |
| Stellar CLI | ≥ 26.1.0        | [docs.stellar.org/tools/cli](https://developers.stellar.org/docs/tools/cli) |

```bash
# Clone and verify the setup
git clone https://github.com/heliobond/contracts
cd contracts
cargo test        # run the test suite
make build        # stellar contract build → target/wasm32v1-none/release/
make install-hooks # install the cargo fmt --check pre-commit hook
```

### Editor

Any editor works. If you use VS Code, the `rust-analyzer` extension provides inline type hints and error squiggles. The repo ships a `.vscode/` config; accept the recommended extensions when prompted.

---

## Workflow

1. **Fork** the repo and create a branch from `main`:
   ```
   git checkout -b fix/short-description   # bug fixes
   git checkout -b feat/short-description  # new features
   git checkout -b test/short-description  # tests only
   git checkout -b docs/short-description  # documentation
   ```
2. **Write your change.** Keep it scoped to one issue.
3. **Run the full quality gate locally** before pushing:
   ```bash
   cargo fmt --all                  # format (CI checks this)
   cargo test --all                 # all tests must pass
   make build                       # contracts must compile to WASM
   ```
4. **Open a PR** with `Closes #<issue>` in the body. CI runs `cargo fmt --check`, `cargo test`, and a WASM size check — all must be green.

### Commit message format

We use [Conventional Commits](https://www.conventionalcommits.org/). The changelog is generated from commit messages automatically on each release.

```
<type>(<scope>): <short summary>

[optional body]

[optional footer: Closes #NNN]
```

**Types:** `feat`, `fix`, `test`, `docs`, `ci`, `refactor`, `chore`  
**Scopes:** `investment_vault`, `project_registry`, `ci`, `adr` (or omit for cross-cutting)

Examples:

```
feat(investment_vault): add MAX_DEPOSIT cap to prevent overflow
fix(project_registry): guard u32 counter against overflow at u32::MAX
docs(adr): add ADR-003 explaining share vault model
ci: add WASM size budget check to CI
```

### Changelog

`CHANGELOG.md` is generated automatically, not written by hand — do not edit it in your PR. [`.github/workflows/changelog.yml`](.github/workflows/changelog.yml) runs on every `v*` tag push (or manually via `workflow_dispatch`), scans commit history with `conventional-changelog` (Angular preset), regenerates `CHANGELOG.md`, and publishes it as the GitHub release notes.

This means your changelog entry _is_ your commit message, so it has to follow the format above correctly:

- `feat:` commits bump the minor version and appear under "Features"
- `fix:` (and anything else conventional-changelog treats as a fix) bumps patch and appears under "Bug Fixes"
- A `BREAKING CHANGE:` footer, or a `!` after the type/scope (e.g. `feat(investment_vault)!: ...`), bumps major
- `docs:`, `ci:`, `chore:`, `refactor:`, `test:` commits are included in the changelog under their own sections but don't trigger a version bump on their own

If a PR is squash-merged, the squash commit message is what the changelog picks up — make sure the squash message (not just the individual commits) follows the format.

---

## Testing requirements

Every PR that changes contract logic **must** include tests. We use the [Soroban test SDK](https://docs.rs/soroban-sdk/latest/soroban_sdk/testutils/index.html).

### Where tests live

```
investment_vault/src/test.rs      ← vault tests
project_registry/src/test.rs      ← registry tests
```

### What to test

| Change type                            | Minimum tests required                                   |
| -------------------------------------- | -------------------------------------------------------- |
| New function                           | Happy path + at least one error case                     |
| Bug fix                                | Regression test that would have caught the original bug  |
| Edge case guard (overflow, zero, etc.) | Test that triggers the guard and asserts the panic/error |
| Math / share calculations              | Rounding test + extreme value test                       |

### Money paths are sacred

Anything touching `deposit`, `withdraw`, `fund_project`, or share math needs tests for:

- First deposit into an empty vault
- Vault with non-zero assets and shares
- Rounding direction (truncation should favour the vault, never the user)
- The relevant edge case for your change (zero amount, max amount, etc.)

### Running a single test

```bash
cargo test -p investment-vault test_deposit   # filter by test name
cargo test --all -- --nocapture               # see println! output
```

---

## Code style

- **`cargo fmt`** — non-negotiable. CI rejects unformatted code.
- **No `std`** — contracts are `#![no_std]`. Do not add `std`-dependent crates.
- **No panics in library paths** — panics in `#[contractimpl]` are fine (they become Soroban errors); panics inside utility functions called from tests are not.
- **Events for every state change** — every mutation must emit a Soroban event so the indexer can reconstruct state. See `events.rs` in each contract.
- **Comments on non-obvious decisions** — explain _why_, not _what_. Reference the issue number for workarounds (`// #112: cap prevents i128 overflow in share calc`).

---

## Security review requirements

Any change to the following requires a dedicated security note in the PR description:

- **Share math** (`convert_to_shares`, `convert_to_assets`) — show that division cannot produce zero denominators and that rounding favours the vault.
- **Fund flows** (`deposit`, `withdraw`, `fund_project`) — show that tokens are transferred in the correct direction and that balances are updated atomically.
- **Access control** (`#[only_owner]`, `whitelister`) — confirm the auth guard fires before any state change.
- **New storage keys** — confirm the correct storage tier (instance vs persistent) and document TTL implications.

If you find a security vulnerability, **do not open a public issue**. See [SECURITY.md](./SECURITY.md) for the private disclosure process.

---

## Architecture decisions

Significant architectural choices are documented in [`adr/`](./adr/). Before making a change that alters a storage pattern, access control model, or the share vault logic, read the relevant ADR. If your change supersedes a decision, open a new ADR and mark the old one as _Superseded_.

---

## Seeing the whole flow end-to-end

New to the codebase and want to see how the pieces fit together before diving
into an issue? [`scripts/example_full_flow.sh`](./scripts/example_full_flow.sh)
walks through the full happy path — whitelist a creator, create a project,
deposit into the vault, fund the project, and certify it — against an
already-deployed local or testnet instance, printing each `stellar contract
invoke` call as it runs (#259).

## Deployment procedures

See [`.github/workflows/deploy.yml`](.github/workflows/deploy.yml) for the automated deployment pipeline. Manual deployments must follow the same build → test → deploy → verify sequence defined in that workflow.

---

## Reporting issues

Bugs and ideas: [open an issue](https://github.com/heliobond/contracts/issues/new). Security problems: see [SECURITY.md](./SECURITY.md) — report privately, not in a public issue.

By contributing you agree your work is licensed under [Apache-2.0](./LICENSE), and you agree to the [Code of Conduct](./CODE_OF_CONDUCT.md).
