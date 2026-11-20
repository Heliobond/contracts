"""Unit tests for check_interface_docs.py (#399)."""

import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS_DIR))

FIXTURES = Path(__file__).resolve().parent / "fixtures"


class CheckInterfaceDocsTest(unittest.TestCase):
    def _run_with_files(self, lib_rs_content, interface_md_content):
        """Run the checker with custom source/doc content."""
        import check_interface_docs as mod

        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            lib_path = tmp / "lib.rs"
            interface_path = tmp / "INTERFACE.md"
            lib_path.write_text(lib_rs_content)
            interface_path.write_text(interface_md_content)

            original_contracts = mod.CONTRACTS.copy()
            original_interface = (mod.REPO_ROOT / "INTERFACE.md").read_text() if (mod.REPO_ROOT / "INTERFACE.md").exists() else ""
            mod.CONTRACTS = {"ProjectRegistry": lib_path}
            # Patch REPO_ROOT so INTERFACE.md points to our temp dir
            original_root = mod.REPO_ROOT
            mod.REPO_ROOT = tmp
            try:
                result = mod.main()
            finally:
                mod.CONTRACTS = original_contracts
                mod.REPO_ROOT = original_root
        return result

    def test_synced_passes(self):
        lib = (FIXTURES / "lib_synced.rs").read_text()
        doc = (FIXTURES / "interface_synced.md").read_text()
        self.assertEqual(self._run_with_files(lib, doc), 0)

    def test_undocumented_function_fails(self):
        lib = (FIXTURES / "lib_undocumented.rs").read_text()
        doc = (FIXTURES / "interface_synced.md").read_text()
        self.assertEqual(self._run_with_files(lib, doc), 1)

    def test_stale_doc_entry_fails(self):
        lib = (FIXTURES / "lib_stale.rs").read_text()
        doc = (FIXTURES / "interface_stale.md").read_text()
        self.assertEqual(self._run_with_files(lib, doc), 1)


if __name__ == "__main__":
    unittest.main()
