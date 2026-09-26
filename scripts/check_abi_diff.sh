#!/usr/bin/env bash
# Compare base vs PR contract ABIs (output of `stellar contract inspect`).
#
# Only lines present in the base ABI but missing from the PR ABI count as
# breaking: a removed function, a renamed one, or a changed parameter/return
# type all drop the old line. Purely added lines (new functions, new types)
# are backward-compatible and are reported as a notice without failing.
#
# Usage: check_abi_diff.sh <base_dir> <pr_dir> <contract>...
set -euo pipefail

base_dir="$1"; pr_dir="$2"; shift 2

failed=false
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

for contract in "$@"; do
  base="$base_dir/${contract}.txt"
  pr="$pr_dir/${contract}.txt"

  grep -v '^[[:space:]]*$' "$base" | sort -u > "$tmp/base"
  grep -v '^[[:space:]]*$' "$pr" | sort -u > "$tmp/pr"

  removed="$(comm -23 "$tmp/base" "$tmp/pr")"
  added="$(comm -13 "$tmp/base" "$tmp/pr")"

  if [ -n "$removed" ]; then
    echo "Breaking ABI change for ${contract} (removed or changed lines):"
    printf '%s\n' "$removed" | sed 's/^/- /'
    failed=true
  fi
  if [ -n "$added" ]; then
    echo "::notice::Additive (non-breaking) ABI change for ${contract}"
    printf '%s\n' "$added" | sed 's/^/+ /'
  fi
  if [ -z "$removed" ] && [ -z "$added" ]; then
    echo "No ABI changes for ${contract}."
  fi
done

if [ "$failed" = "true" ]; then
  echo "::error::Breaking ABI changes detected. Review the removed lines above and update integrations accordingly."
  exit 1
fi
echo "No breaking ABI changes detected."
