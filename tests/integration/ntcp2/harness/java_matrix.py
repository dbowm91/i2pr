"""Plan 054 Java startup matrix and frozen-template orchestration.

This module keeps the controller code separate from
``java_startup_probe.py`` so the per-attempt driver remains the single
artifact read by external callers. It exposes:

- ``MATRIX_CELLS``: the 16-cell Plan 054 matrix (namespace x data-state
  x launcher x sequence).
- ``run_matrix``: emit one sanitized per-cell record under
  ``matrix_root/<cell>/<attempt>/``.
- ``qualify_template``: drive ten consecutive rootless ``seeded-clone``
  starts from independent clones and verify the frozen template digest
  is unchanged before and after the run.
- ``build_template_manifest``: write the immutable-template manifest
  and a deterministic tree digest for the prepared template root.

The matrix never silently drops a cell. Cells that cannot be exercised
on the current host record the typed blocker
``java-startup-cell-incomplete`` in their per-cell JSON.
"""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import java_startup_probe as _probe  # type: ignore
else:
    from . import java_startup_probe as _probe


MATRIX_NAMESPACE = ("outer", "rootless")
MATRIX_DATA_STATE = ("empty", "seeded-clone")
MATRIX_LAUNCHER = ("runplain", "wrapper")
MATRIX_SEQUENCE = ("single", "generate-live")

TEMPLATE_MANIFEST_SCHEMA = "i2pr-java-template-manifest-v1"


def matrix_cells() -> list[dict[str, str]]:
    cells: list[dict[str, str]] = []
    for namespace in MATRIX_NAMESPACE:
        for data_state in MATRIX_DATA_STATE:
            for launcher in MATRIX_LAUNCHER:
                for sequence in MATRIX_SEQUENCE:
                    cells.append({
                        "namespace": namespace,
                        "data_state": data_state,
                        "launcher": launcher,
                        "sequence": sequence,
                    })
    return cells


def cell_id(cell: dict[str, str]) -> str:
    return _probe._cell_id(cell["launcher"], cell["data_state"], cell["namespace"], cell["sequence"])


def _invoke_probe(record_dir: Path, *, reference_install: Path, template_root: Path, cell: dict[str, str], attempt_index: int, qualification_mode: bool) -> dict[str, Any]:
    record_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    output = record_dir / f"attempt-{attempt_index}.json"
    template = template_root if cell["data_state"] == "seeded-clone" else Path("")
    command = [
        sys.executable,
        str(Path(_probe.__file__).resolve()),
        "--reference-install", str(reference_install),
        "--data-dir", str(record_dir / "data"),
        "--data-state", cell["data_state"],
        "--launcher", cell["launcher"],
        "--namespace", cell["namespace"],
        "--sequence", cell["sequence"],
        "--attempts", "1",
        "--output", str(output),
    ]
    if template:
        command.extend(["--state-template", str(template)])
    if qualification_mode:
        command.append("--qualification-mode")
    completed = subprocess.run(command, capture_output=True, text=True, check=False)
    payload: dict[str, Any] = {
        "cell_id": cell_id(cell),
        "attempt_index": attempt_index,
        "command_exit_code": completed.returncode,
        "command_stdout_tail": completed.stdout[-512:],
        "command_stderr_tail": completed.stderr[-512:],
    }
    if output.is_file():
        try:
            payload["record"] = json.loads(output.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            payload["record"] = None
    else:
        payload["record"] = None
    return payload


def run_matrix(
    *,
    reference_install: Path,
    matrix_root: Path,
    attempts_per_cell: int = 3,
    template_root: Path | None = None,
) -> dict[str, Any]:
    matrix_root = matrix_root.resolve()
    matrix_root.mkdir(mode=0o700, parents=True, exist_ok=True)
    matrix_run_id = f"matrix-{dt.datetime.now(dt.UTC).strftime('%Y%m%dT%H%M%SZ')}-{uuid.uuid4().hex[:8]}"
    cells_summary: list[dict[str, Any]] = []
    for cell in matrix_cells():
        cell_root = matrix_root / cell_id(cell)
        attempts: list[dict[str, Any]] = []
        for attempt_index in range(1, attempts_per_cell + 1):
            attempts.append(_invoke_probe(
                cell_root,
                reference_install=reference_install,
                template_root=template_root or Path(""),
                cell=cell,
                attempt_index=attempt_index,
                qualification_mode=False,
            ))
        ready = sum(1 for attempt in attempts if (attempt.get("record") or {}).get("attempts_ready", 0) > 0)
        cells_summary.append({
            "cell_id": cell_id(cell),
            "cell": cell,
            "attempts": attempts,
            "attempts_total": attempts_per_cell,
            "attempts_ready": ready,
        })
    summary_path = matrix_root / "matrix-summary.json"
    summary = {
        "schema": "i2pr-java-startup-matrix-v1",
        "schema_version": 1,
        "type": "java-startup-matrix",
        "created_at_utc": _probe._now(),
        "matrix_run_id": matrix_run_id,
        "cells_total": len(cells_summary),
        "cells_recorded": len(cells_summary),
        "attempts_per_cell": attempts_per_cell,
        "cells": cells_summary,
    }
    _atomic_write_json(summary_path, summary)
    return summary


def _atomic_write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(json.dumps(payload, sort_keys=False, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if Path(temporary).exists():
            Path(temporary).unlink()


def build_template_manifest(template_root: Path, *, reference_version: str, reference_revision: str) -> dict[str, Any]:
    template_root = template_root.resolve()
    listing: list[tuple[str, int]] = []
    hasher = hashlib.sha256()
    hasher.update(b"i2pr-java-template-tree-v1")
    for root, _, files in os.walk(template_root):
        for entry in sorted(files):
            path = Path(root) / entry
            try:
                rel = path.relative_to(template_root).as_posix()
            except ValueError:
                continue
            if rel in {"template-manifest.json", "template-tree.sha256"}:
                continue
            data = path.read_bytes()
            listing.append((rel, len(data)))
            hasher.update(rel.encode("utf-8"))
            hasher.update(b"\0")
            hasher.update(data)
            hasher.update(b"\0")
    tree_digest = hasher.hexdigest()
    manifest = {
        "schema": TEMPLATE_MANIFEST_SCHEMA,
        "schema_version": 1,
        "reference_version": reference_version,
        "reference_revision": reference_revision,
        "template_root": str(template_root),
        "file_count": len(listing),
        "tree_sha256": tree_digest,
        "files": [{"path": rel, "size": size} for rel, size in listing],
        "created_at_utc": _probe._now(),
    }
    manifest_path = template_root / "template-manifest.json"
    _atomic_write_json(manifest_path, manifest)
    digest_path = template_root / "template-tree.sha256"
    digest_path.write_text(f"{tree_digest}  template-tree\n", encoding="ascii")
    digest_path.chmod(0o600)
    return manifest


def verify_template_unchanged(template_root: Path, expected_digest: str) -> bool:
    actual = compute_template_tree_digest(template_root)
    return actual == expected_digest


def compute_template_tree_digest(template_root: Path) -> str:
    template_root = template_root.resolve()
    hasher = hashlib.sha256()
    hasher.update(b"i2pr-java-template-tree-v1")
    for root, _, files in os.walk(template_root):
        for entry in sorted(files):
            path = Path(root) / entry
            try:
                rel = path.relative_to(template_root).as_posix()
            except ValueError:
                continue
            if rel in {"template-manifest.json", "template-tree.sha256"}:
                continue
            data = path.read_bytes()
            hasher.update(rel.encode("utf-8"))
            hasher.update(b"\0")
            hasher.update(data)
            hasher.update(b"\0")
    return hasher.hexdigest()


def qualify_template(
    *,
    reference_install: Path,
    template_root: Path,
    run_root: Path,
    consecutive_starts: int = 10,
) -> dict[str, Any]:
    run_root = run_root.resolve()
    run_root.mkdir(mode=0o700, parents=True, exist_ok=True)
    pre_manifest = build_template_manifest(template_root, reference_version="", reference_revision="")
    pre_digest = pre_manifest["tree_sha256"]
    starts: list[dict[str, Any]] = []
    for index in range(1, consecutive_starts + 1):
        clone_root = run_root / f"qualify-{index:02d}"
        clone_root.mkdir(mode=0o700, parents=True, exist_ok=True)
        record = _invoke_probe(
            clone_root,
            reference_install=reference_install,
            template_root=template_root,
            cell={
                "namespace": "rootless",
                "data_state": "seeded-clone",
                "launcher": "runplain",
                "sequence": "generate-live",
            },
            attempt_index=index,
            qualification_mode=True,
        )
        starts.append(record)
    post_digest = build_template_manifest(template_root, reference_version="", reference_revision="")["tree_sha256"]
    digest_unchanged = post_digest == pre_digest
    ready = sum(1 for start in starts if (start.get("record") or {}).get("attempts_ready", 0) > 0)
    summary = {
        "schema": "i2pr-java-startup-qualification-v1",
        "schema_version": 1,
        "type": "java-startup-qualification",
        "consecutive_starts": consecutive_starts,
        "ready_starts": ready,
        "ready_required": consecutive_starts,
        "pre_template_tree_sha256": pre_digest,
        "post_template_tree_sha256": post_digest,
        "template_digest_unchanged": digest_unchanged,
        "starts": starts,
    }
    summary_path = run_root / "qualification-summary.json"
    _atomic_write_json(summary_path, summary)
    return summary
