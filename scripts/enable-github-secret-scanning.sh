#!/usr/bin/env bash
# Enable GitHub's native secret-scanning features on Heliobond/contracts.
# Requires a token with admin:repo scope:  gh auth login, then run this script.
#   bash scripts/enable-github-secret-scanning.sh [OWNER/REPO]
set -euo pipefail
REPO="${1:-Heliobond/contracts}"

echo "Enabling secret scanning features on $REPO ..."

# Secret scanning + push protection (REST: secret-scanning API).
gh api -X PATCH "repos/$REPO/secret-scanning/push-protection" \
  -f state=open >/dev/null && echo "push protection: enabled"

# Validity checks, non-provider patterns (same API family).
gh api -X PATCH "repos/$REPO/secret-scanning/validity-checks" \
  -f state=open >/dev/null && echo "validity checks: enabled" || true
gh api -X PATCH "repos/$REPO/secret-scanning/non-provider-patterns" \
  -f state=open >/dev/null && echo "non-provider patterns: enabled" || true

# Dependabot security updates (separate endpoint).
gh api -X PUT "repos/$REPO/automated-security-fixes" >/dev/null \
  && echo "dependabot security updates: enabled" || true

echo "Done. Verify with:"
echo "  gh api repos/$REPO --jq '.security_and_analysis'"
