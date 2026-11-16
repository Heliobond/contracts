"""Exercise run provenance and artifact rejection without network access."""

import copy
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch
import unittest

spec = importlib.util.spec_from_file_location("ci_wasm_artifact", Path(__file__).parents[1] / "ci_wasm_artifact.py")
artifact = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifact)
SHA = "a" * 40
RUN = "123"


class ArtifactTests(unittest.TestCase):
    def test_trusted_main_push_only(self):
        event = {"repository": {"full_name": artifact.REPOSITORY}, "workflow_run": {
            "head_repository": {"full_name": artifact.REPOSITORY}, "name": "CI",
            "status": "completed", "conclusion": "success", "event": "push",
            "head_branch": "main", "id": 123, "head_sha": SHA}}
        self.assertTrue(artifact.trusted_run(event))
        for key, value in (("event", "pull_request"), ("event", "workflow_dispatch"),
                           ("conclusion", "failure"), ("status", "in_progress"),
                           ("head_branch", "feature"), ("head_sha", "bad"),
                           ("head_repository", {"full_name": "fork/contracts"}),
                           ("id", True), ("name", "Other workflow")):
            with self.subTest(key=key, value=value):
                changed = copy.deepcopy(event)
                changed["workflow_run"][key] = value
                self.assertFalse(artifact.trusted_run(changed))
        event["repository"]["full_name"] = "fork/contracts"
        self.assertFalse(artifact.trusted_run(event))
        self.assertFalse(artifact.trusted_run({}))

    def test_guard_entrypoint_preserves_manual_and_schedule_fallback(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            event = directory / "event.json"
            output = directory / "output"
            event.write_text("{}")
            for name in ("workflow_dispatch", "schedule", "workflow_run", "pull_request"):
                with self.subTest(event=name):
                    output.write_text("")
                    with patch.dict("os.environ", {
                        "GITHUB_EVENT_NAME": name, "GITHUB_EVENT_PATH": str(event),
                        "GITHUB_OUTPUT": str(output), "GITHUB_SHA": SHA,
                    }):
                        artifact.guard()
                    expected = name in ("workflow_dispatch", "schedule")
                    self.assertIn(f"should_run={'true' if expected else 'false'}", output.read_text())
                    if expected:
                        self.assertIn(f"sha={SHA}", output.read_text())
                    else:
                        self.assertNotIn("sha=", output.read_text())

    def test_artifact_integrity_and_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name in artifact.FILES:
                (directory / name).write_bytes(b"\0asm" + name.encode())
            data = artifact.manifest(directory, SHA, RUN)
            path = directory / artifact.MANIFEST
            path.write_text(json.dumps(data))
            artifact.verify(directory, SHA, RUN)
            for key, value in (("sha", "b" * 40), ("run_id", "124"),
                               ("repository", "fork/contracts"), ("files", {})):
                with self.subTest(key=key):
                    changed = copy.deepcopy(data)
                    changed[key] = value
                    path.write_text(json.dumps(changed))
                    with self.assertRaises(ValueError):
                        artifact.verify(directory, SHA, RUN)
            path.write_text(json.dumps(data))
            first = directory / artifact.FILES[0]
            first.write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                artifact.verify(directory, SHA, RUN)
            first.unlink()
            with self.assertRaises(ValueError):
                artifact.verify(directory, SHA, RUN)
            path.unlink()
            with self.assertRaises(FileNotFoundError):
                artifact.verify(directory, SHA, RUN)

    def test_linked_or_empty_artifacts_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            first = directory / artifact.FILES[0]
            first.touch()
            with self.assertRaises(ValueError):
                artifact.manifest(directory, SHA, RUN)
            first.unlink()
            first.symlink_to(__file__)
            with self.assertRaises(ValueError):
                artifact.manifest(directory, SHA, RUN)


if __name__ == "__main__":
    unittest.main()
