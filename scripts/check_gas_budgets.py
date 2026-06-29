#!/usr/bin/env python3
"""Create a gas report from Soroban cost-estimate test output and enforce budgets."""

import json
import re
import sys
from pathlib import Path


LINE_RE = re.compile(
    r"gas_budget\s+(?P<name>[a-zA-Z0-9_.:-]+)\s+"
    r"instructions=(?P<instructions>\d+)\s+fee=(?P<fee>\d+)"
)


def main() -> int:
    if len(sys.argv) != 4:
        print(
            "usage: check_gas_budgets.py <budget-json> <test-output> <report-md>",
            file=sys.stderr,
        )
        return 2

    budget_path = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    report_path = Path(sys.argv[3])

    budgets = json.loads(budget_path.read_text())
    measurements = {}

    for line in output_path.read_text(errors="replace").splitlines():
        match = LINE_RE.search(line)
        if match:
            measurements[match.group("name")] = {
                "instructions": int(match.group("instructions")),
                "fee": int(match.group("fee")),
            }

    rows = ["# Gas Report", "", "| Function | Instructions | Budget | Fee | Budget | Status |", "| --- | ---: | ---: | ---: | ---: | --- |"]
    failed = False

    for name in sorted(budgets):
        budget = budgets[name]
        measured = measurements.get(name)
        if measured is None:
            rows.append(f"| `{name}` | missing | {budget['instructions']} | missing | {budget['fee']} | FAIL |")
            failed = True
            continue

        over_instructions = measured["instructions"] > budget["instructions"]
        over_fee = measured["fee"] > budget["fee"]
        status = "FAIL" if over_instructions or over_fee else "OK"
        failed = failed or over_instructions or over_fee
        rows.append(
            f"| `{name}` | {measured['instructions']} | {budget['instructions']} | "
            f"{measured['fee']} | {budget['fee']} | {status} |"
        )

    extra = sorted(set(measurements) - set(budgets))
    if extra:
        rows.extend(["", "## Unbudgeted Measurements", ""])
        rows.extend(f"- `{name}`" for name in extra)

    report_path.write_text("\n".join(rows) + "\n")
    print(report_path.read_text())
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
