#!/usr/bin/env python3
"""Catch drift between INTERFACE.md and each contract's actual pub fn exports (#273).

Scope: compares function *names* only (not full parameter lists — markdown
formatting is too free-form to diff reliably) for each contract's primary
hand-written `impl <Contract> { ... }` block(s). Functions provided by
derived/external traits (Ownable, FungibleToken, FungibleBurnable from
stellar-access / stellar-tokens) are intentionally excluded: they're
documented by those crates, not by this repo, and aren't expected to appear
in INTERFACE.md's per-contract function tables.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Functions provided by external derive/trait impls, not hand-written contract
# entry points — excluded from the comparison (see module docstring).
TRAIT_PROVIDED_FUNCTIONS = {
    "balance", "transfer", "transfer_from", "approve", "allowance",
    "name", "symbol", "decimals", "burn", "burn_from",
    "get_owner", "transfer_ownership", "renounce_ownership",
}

CONTRACTS = {
    "ProjectRegistry": REPO_ROOT / "project_registry" / "src" / "lib.rs",
    "InvestmentVault": REPO_ROOT / "investment_vault" / "src" / "lib.rs",
}

PUB_FN_RE = re.compile(r"^\s{4}pub fn (\w+)\(", re.MULTILINE)
# Only the first backtick-quoted identifier at the start of a markdown table
# row counts as "documented" — backtick mentions inside prose/notes cells
# (e.g. "no `clear_multisig_admin()` function") must not be treated as claims
# that this contract exports that function.
DOC_FN_RE = re.compile(r"^\| `(\w+)\(", re.MULTILINE)


def extract_contract_functions(source: str) -> set[str]:
    return {m.group(1) for m in PUB_FN_RE.finditer(source)} - TRAIT_PROVIDED_FUNCTIONS


def extract_documented_functions(doc_text: str, section_start: str, section_end: str | None) -> set[str]:
    start = doc_text.index(section_start)
    end = doc_text.index(section_end, start) if section_end else len(doc_text)
    section = doc_text[start:end]
    return {m.group(1) for m in DOC_FN_RE.finditer(section)}


def main() -> int:
    interface_md = (REPO_ROOT / "INTERFACE.md").read_text()

    sections = {
        "ProjectRegistry": ("## ProjectRegistry", "## InvestmentVault"),
        "InvestmentVault": ("## InvestmentVault", None),
    }

    failed = False
    for contract_name, source_path in CONTRACTS.items():
        actual = extract_contract_functions(source_path.read_text())
        start, end = sections[contract_name]
        documented = extract_documented_functions(interface_md, start, end)

        undocumented = sorted(actual - documented)
        stale = sorted(documented - actual)

        if undocumented:
            failed = True
            print(f"[{contract_name}] exported but undocumented in INTERFACE.md: {undocumented}")
        if stale:
            failed = True
            print(f"[{contract_name}] documented in INTERFACE.md but no longer exported: {stale}")

    if failed:
        print("\nINTERFACE.md is out of sync with contract exports. Update INTERFACE.md "
              "(or the contract, if the removal was intentional) and re-run.", file=sys.stderr)
        return 1

    print("INTERFACE.md matches contract exports for all checked functions.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
