#!/usr/bin/env python3
"""Confirm shared event field names use the same type in both contracts (#258).

Extracts every `pub field: Type` inside a `#[contractevent] pub struct ... {}`
block from both investment_vault/src/events.rs and
project_registry/src/events.rs, groups by field name, and fails if the same
field name maps to different types across the two contracts. A field like
`project_id` appearing as `u32` in one contract and `u64` in the other would
be a real integration hazard for off-chain indexers that assume a shared
schema — this is exactly the kind of drift EVENTS.md can't catch on its own
since it's hand-written prose, not machine-checked against the source.
"""

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

EVENT_FILES = {
    "investment_vault": REPO_ROOT / "investment_vault" / "src" / "events.rs",
    "project_registry": REPO_ROOT / "project_registry" / "src" / "events.rs",
}

# (struct_name, field_name) pairs known to legitimately diverge from the
# "same name -> same type" rule, with the reason why. Checked *before*
# reporting a struct as part of an inconsistency, so only these specific
# struct+field combinations are exempted, not the field name globally.
ALLOWED_DIVERGENCES = {
    ("BridgeTransferInitiated", "recipient"): (
        "Wormhole bridge recipient is a chain-agnostic 32-byte address "
        "(BytesN<32>) for the *destination* chain, not a Soroban Address "
        "for this chain — intentionally different from same-chain "
        "`recipient` fields like CollateralReleased/InsuranceClaimed."
    ),
    ("WhitelistSet", "status"): (
        "WhitelistSet.status is a simple on/off toggle (bool), while "
        "ProjectCertified.status is a CertificationStatus enum — "
        "intentionally different domain concepts sharing a generic name."
    ),
    ("ProjectCertified", "status"): (
        "ProjectCertified.status is a CertificationStatus enum — "
        "intentionally different from WhitelistSet.status (bool), "
        "which is a simple on/off toggle."
    ),
}

# Matches `#[contractevent] ... pub struct Name { ...fields... }`, non-greedy
# up to the first closing brace (event structs here are flat, no nested {}).
STRUCT_RE = re.compile(
    r"#\[contractevent\][^\n]*\n(?:[^\n]*\n)*?pub struct (\w+)\s*\{(.*?)\}",
    re.DOTALL,
)
FIELD_RE = re.compile(r"pub (\w+):\s*([^,\n]+),?")


def extract_fields(path: Path):
    """Yield (struct_name, field_name, field_type) for every event struct."""
    text = path.read_text()
    for struct_match in STRUCT_RE.finditer(text):
        struct_name, body = struct_match.group(1), struct_match.group(2)
        for field_match in FIELD_RE.finditer(body):
            field_name, field_type = field_match.group(1), field_match.group(2).strip()
            yield struct_name, field_name, field_type


def main() -> int:
    # field_name -> {field_type: [(contract, struct_name), ...]}
    by_field = {}
    for contract, path in EVENT_FILES.items():
        for struct_name, field_name, field_type in extract_fields(path):
            by_field.setdefault(field_name, {}).setdefault(field_type, []).append(
                (contract, struct_name)
            )

    failed = False
    for field_name, types in sorted(by_field.items()):
        if len(types) <= 1:
            continue

        # Drop any (struct, field) pair that's an explicitly allowed
        # divergence before deciding whether this field is actually a problem.
        remaining = {}
        for ftype, locs in types.items():
            kept = [
                (c, s) for c, s in locs if (s, field_name) not in ALLOWED_DIVERGENCES
            ]
            if kept:
                remaining[ftype] = kept
        if len(remaining) <= 1:
            continue

        contracts_involved = {c for locs in remaining.values() for c, _ in locs}
        if len(contracts_involved) <= 1:
            # Inconsistent within a single contract's own events — still a
            # real hazard for off-chain indexers that normalize fields across
            # events from the same contract (#397).
            failed = True
            print(f"Field `{field_name}` has inconsistent types within a single contract:")
            for ftype, locs in remaining.items():
                for contract, struct_name in locs:
                    print(f"  {contract}::{struct_name}.{field_name}: {ftype}")
            continue
        failed = True
        print(f"Field `{field_name}` has inconsistent types across contracts:")
        for ftype, locs in remaining.items():
            for contract, struct_name in locs:
                print(f"  {contract}::{struct_name}.{field_name}: {ftype}")

    if failed:
        print(
            "\nShared field names should use the same type across events "
            "(both within and across contracts) so off-chain indexers can "
            "rely on a consistent schema.",
            file=sys.stderr,
        )
        return 1

    print("All shared event field names use consistent types.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
