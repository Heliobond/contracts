"""Unit tests for check_deploy_wasm_hash.py check/update modes (#398)."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "check_deploy_wasm_hash.py"
FIXTURES = Path(__file__).resolve().parent / "fixtures"


class CheckDeployWasmHashTest(unittest.TestCase):
    def _run(self, mode, manifest_path, pairs, expect_returncode=0):
        args = [sys.executable, str(SCRIPT), mode, str(manifest_path)]
        for name, path in pairs.items():
            args.append(f"{name}={path}")
        result = subprocess.run(args, capture_output=True, text=True)
        self.assertEqual(result.returncode, expect_returncode, result.stderr)
        return result

    def test_update_writes_hash_into_manifest(self):
        wasm = FIXTURES / "sample.wasm"
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text("{}")
            self._run("update", manifest, {"project_registry": wasm})
            data = json.loads(manifest.read_text())
            self.assertIn("project_registry_wasm_hash", data)
            self.assertEqual(len(data["project_registry_wasm_hash"]), 64)

    def test_check_passes_when_hash_matches(self):
        wasm = FIXTURES / "sample.wasm"
        import hashlib
        expected_hash = hashlib.sha256(wasm.read_bytes()).hexdigest()
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(json.dumps({"project_registry_wasm_hash": expected_hash}))
            result = self._run("check", manifest, {"project_registry": wasm})
            self.assertIn("OK", result.stdout)

    def test_check_fails_when_hash_mismatches(self):
        wasm = FIXTURES / "sample.wasm"
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(json.dumps({"project_registry_wasm_hash": "0000"}))
            self._run("check", manifest, {"project_registry": wasm}, expect_returncode=1)

    def test_check_skips_when_no_hash_recorded(self):
        wasm = FIXTURES / "sample.wasm"
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text(json.dumps({}))
            result = self._run("check", manifest, {"project_registry": wasm})
            self.assertIn("skipping", result.stdout)

    def test_unknown_mode_exits_2(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text("{}")
            self._run("bogus", manifest, {}, expect_returncode=2)

    def test_invalid_argument_exits_2(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "manifest.json"
            manifest.write_text("{}")
            args = [sys.executable, str(SCRIPT), "check", str(manifest), "bad-arg"]
            result = subprocess.run(args, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
