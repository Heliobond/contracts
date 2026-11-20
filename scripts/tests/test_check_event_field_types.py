"""Unit tests for check_event_field_types.py (#399)."""

import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS_DIR))

FIXTURES = Path(__file__).resolve().parent / "fixtures"


class CheckEventFieldTypesTest(unittest.TestCase):
    def _run_with_files(self, investment_vault_content, project_registry_content):
        """Run the checker with custom event file content by importing the module."""
        import importlib
        import check_event_field_types as mod

        with tempfile.TemporaryDirectory() as tmp:
            iv_path = Path(tmp) / "investment_vault_events.rs"
            pr_path = Path(tmp) / "project_registry_events.rs"
            iv_path.write_text(investment_vault_content)
            pr_path.write_text(project_registry_content)

            # Temporarily override EVENT_FILES
            original = mod.EVENT_FILES.copy()
            mod.EVENT_FILES = {
                "investment_vault": iv_path,
                "project_registry": pr_path,
            }
            try:
                result = mod.main()
            finally:
                mod.EVENT_FILES = original
        return result

    def test_consistent_types_pass(self):
        events = (FIXTURES / "events_consistent.rs").read_text()
        self.assertEqual(self._run_with_files(events, events), 0)

    def test_intra_contract_inconsistency_fails(self):
        intra = (FIXTURES / "events_intra_inconsistent.rs").read_text()
        consistent = (FIXTURES / "events_consistent.rs").read_text()
        self.assertEqual(self._run_with_files(intra, consistent), 1)

    def test_cross_contract_inconsistency_fails(self):
        consistent = (FIXTURES / "events_consistent.rs").read_text()
        cross = (FIXTURES / "events_cross_inconsistent.rs").read_text()
        self.assertEqual(self._run_with_files(consistent, cross), 1)

    def test_allowed_divergence_is_exempted(self):
        """BridgeTransferInitiated.recipient is an allowed cross-contract divergence."""
        iv = """
#[contractevent]
pub struct BridgeTransferInitiated {
    pub recipient: BytesN<32>,
    pub amount: i128,
}
"""
        pr = """
#[contractevent]
pub struct CollateralReleased {
    pub recipient: Address,
    pub amount: i128,
}
"""
        self.assertEqual(self._run_with_files(iv, pr), 0)


if __name__ == "__main__":
    unittest.main()
