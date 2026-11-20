"""Unit tests for check_gas_budgets.py pass/fail logic (#250)."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "check_gas_budgets.py"
FIXTURES = Path(__file__).resolve().parent / "fixtures"
BUDGETS = FIXTURES / "budgets.json"
BUDGETS_WITH_METADATA = FIXTURES / "budgets_with_metadata.json"


class CheckGasBudgetsTest(unittest.TestCase):
    def run_check(self, profile_name, budgets_path=BUDGETS):
        with tempfile.TemporaryDirectory() as tmp:
            report_path = Path(tmp) / "gas-report.md"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    str(budgets_path),
                    str(FIXTURES / profile_name),
                    str(report_path),
                ],
                capture_output=True,
                text=True,
            )
            report = report_path.read_text() if report_path.exists() else ""
        return result, report

    def test_within_budget_passes(self):
        result, report = self.run_check("profile_pass.txt")
        self.assertEqual(result.returncode, 0)
        self.assertIn("| `contract.func` | 900 | 1000 | 90 | 100 | OK |", report)

    def test_over_instructions_budget_fails(self):
        result, report = self.run_check("profile_over_instructions.txt")
        self.assertEqual(result.returncode, 1)
        self.assertIn("| `contract.func` | 1100 | 1000 | 90 | 100 | FAIL |", report)

    def test_over_fee_budget_fails(self):
        result, report = self.run_check("profile_over_fee.txt")
        self.assertEqual(result.returncode, 1)
        self.assertIn("| `contract.func` | 900 | 1000 | 150 | 100 | FAIL |", report)

    def test_missing_measurement_fails(self):
        result, report = self.run_check("profile_missing.txt")
        self.assertEqual(result.returncode, 1)
        self.assertIn("| `contract.func` | missing | 1000 | missing | 100 | FAIL |", report)

    def test_extra_unbudgeted_measurement_does_not_fail(self):
        result, report = self.run_check("profile_extra.txt")
        self.assertEqual(result.returncode, 0)
        self.assertIn("## Unbudgeted Measurements", report)
        self.assertIn("- `contract.other_func`", report)

    # Issue #446: exercise the informational-metadata-entry filter that the real
    # gas-budgets.json relies on to keep `storage_key_sizes` out of pass/fail
    # evaluation, since it has neither `instructions` nor `fee` keys.
    def test_metadata_only_entry_is_excluded_from_evaluation(self):
        result, report = self.run_check(
            "profile_pass.txt", budgets_path=BUDGETS_WITH_METADATA
        )
        self.assertEqual(result.returncode, 0)
        self.assertIn("| `contract.func` | 900 | 1000 | 90 | 100 | OK |", report)
        self.assertNotIn("storage_key_sizes", report)

    def test_wrong_argument_count_errors(self):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), str(BUDGETS)],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage:", result.stderr)


if __name__ == "__main__":
    unittest.main()
