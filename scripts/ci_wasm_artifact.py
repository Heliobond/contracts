"""Bind post-CI WASM reuse to a successful canonical main-push run."""

import hashlib
import json
import os
from pathlib import Path
import re
import sys

REPOSITORY = "Heliobond/contracts"
WASM_DIR = Path("target/wasm32v1-none/release")
FILES = ("project_registry.wasm", "investment_vault.wasm")
MANIFEST = "verified-wasm.json"


def trusted_run(event):
    run = event.get("workflow_run") or {}
    return (
        (event.get("repository") or {}).get("full_name") == REPOSITORY
        and (run.get("head_repository") or {}).get("full_name") == REPOSITORY
        and run.get("name") == "CI"
        and run.get("status") == "completed"
        and run.get("conclusion") == "success"
        and run.get("event") == "push"
        and run.get("head_branch") == "main"
        and type(run.get("id")) is int
        and run["id"] > 0
        and type(run.get("run_attempt")) is int
        and run["run_attempt"] > 0
        and isinstance(run.get("head_sha"), str)
        and re.fullmatch(r"[0-9a-f]{40}", run["head_sha"]) is not None
    )


def manifest(directory, sha, run_id, attempt):
    if (
        not re.fullmatch(r"[0-9a-f]{40}", sha)
        or not re.fullmatch(r"[1-9][0-9]*", run_id)
        or not re.fullmatch(r"[1-9][0-9]*", attempt)
    ):
        raise ValueError("Invalid artifact commit SHA, run ID or attempt")
    digests = {}
    for name in FILES:
        path = directory / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"Missing or invalid WASM file: {name}")
        digests[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return {
        "format": "heliobond-wasm-v1",
        "repository": REPOSITORY,
        "sha": sha,
        "run_id": run_id,
        "run_attempt": attempt,
        "files": digests,
    }


def verify(directory, sha, run_id, attempt):
    path = directory / MANIFEST
    if path.is_symlink():
        raise ValueError("Artifact manifest must be a regular file")
    actual = json.loads(path.read_text())
    if actual != manifest(directory, sha, run_id, attempt):
        raise ValueError("Artifact manifest does not match the expected repository, SHA, run, attempt or file digests")


def guard():
    event_name = os.environ["GITHUB_EVENT_NAME"]
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    automatic = event_name == "workflow_run"
    allowed = trusted_run(event) if automatic else event_name in ("workflow_dispatch", "schedule")
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"should_run={'true' if allowed else 'false'}\n")
        if allowed:
            run = (event.get("workflow_run") or {}) if automatic else {}
            output.write(f"sha={run.get('head_sha', os.environ['GITHUB_SHA'])}\n")
            output.write(f"run_id={run.get('id', '')}\n")
            output.write(f"run_attempt={run.get('run_attempt', '')}\n")


if __name__ == "__main__":
    try:
        mode = sys.argv[1]
        if mode == "guard":
            guard()
        elif mode == "create":
            data = manifest(
                WASM_DIR, os.environ["GITHUB_SHA"], os.environ["GITHUB_RUN_ID"],
                os.environ["GITHUB_RUN_ATTEMPT"],
            )
            (WASM_DIR / MANIFEST).write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
        elif mode == "verify":
            verify(
                WASM_DIR, os.environ["EXPECTED_SHA"], os.environ["EXPECTED_RUN_ID"],
                os.environ["EXPECTED_RUN_ATTEMPT"],
            )
        else:
            raise ValueError("Unknown artifact operation")
    except (ValueError, OSError, KeyError, IndexError) as error:
        sys.exit(str(error))
