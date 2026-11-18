#!/usr/bin/env python3
"""Diff freshly-built contract WASM hashes against deploy/testnet.json (#274).

Two modes:
  check   Fail (exit 1) if the sha256 of a locally built WASM file doesn't
          match the hash recorded in the deploy manifest. Used to catch a
          stale/incorrect manifest, or a deploy that silently didn't update
          the on-chain contract to the code it claims.
  update  Recompute the sha256 of each built WASM and write it into the
          deploy manifest. Run this right after a successful deploy so the
          manifest stays in sync going forward.

Usage:
  check_deploy_wasm_hash.py check  <manifest.json> <name>=<wasm-path> [<name>=<wasm-path> ...]
  check_deploy_wasm_hash.py update <manifest.json> <name>=<wasm-path> [<name>=<wasm-path> ...]
"""

import hashlib
import json
import sys
from pathlib import Path


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_pairs(args):
    pairs = {}
    for arg in args:
        if "=" not in arg:
            print(f"invalid argument (expected name=path): {arg}", file=sys.stderr)
            sys.exit(2)
        name, path = arg.split("=", 1)
        if not name:
            print(f"invalid argument: contract name must not be empty: {arg}", file=sys.stderr)
            sys.exit(2)
        if name in pairs:
            print(f"duplicate contract name {name!r}: supplied more than once", file=sys.stderr)
            sys.exit(2)
        pairs[name] = Path(path)
    return pairs


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__, file=sys.stderr)
        return 2

    mode = sys.argv[1]
    manifest_path = Path(sys.argv[2])
    pairs = parse_pairs(sys.argv[3:])

    manifest = json.loads(manifest_path.read_text())

    if mode == "update":
        for name, wasm_path in pairs.items():
            manifest[f"{name}_wasm_hash"] = sha256_of(wasm_path)
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
        print(f"Updated {manifest_path} with current WASM hashes.")
        return 0

    if mode == "check":
        failed = False
        for name, wasm_path in pairs.items():
            recorded = manifest.get(f"{name}_wasm_hash", "")
            actual = sha256_of(wasm_path)
            if not recorded:
                print(f"[{name}] no hash recorded in {manifest_path} yet — skipping (first deploy?)")
                continue
            if recorded != actual:
                failed = True
                print(f"[{name}] HASH MISMATCH")
                print(f"  recorded: {recorded}")
                print(f"  actual:   {actual}")
            else:
                print(f"[{name}] OK ({actual})")
        if failed:
            print(
                f"\n{manifest_path} does not match the freshly built WASM. "
                "The on-chain contract may not be running the code this repo expects. "
                "If this deploy is intentional, re-run in `update` mode after confirming "
                "the on-chain instance was actually upgraded.",
                file=sys.stderr,
            )
            return 1
        return 0

    print(f"unknown mode: {mode!r} (expected 'check' or 'update')", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
