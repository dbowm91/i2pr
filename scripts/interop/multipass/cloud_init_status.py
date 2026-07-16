#!/usr/bin/env python3
"""Sanitized Plan 050 cloud-init status parser.

This module owns the strict, version-tolerant classification of
``cloud-init`` outputs into typed Plan 050 blockers. It never retains raw
output text, raw log paths, or arbitrary command lines: free-form bytes are
normalized to a fixed taxonomy that the host scripts and validator can
consume.

Allowed ``cloud_init_state`` values:

  - running
  - done
  - degraded
  - error
  - disabled
  - timeout
  - unknown

Allowed ``cloud_init_stage`` values:

  - local
  - network
  - config
  - final
  - post-verify
  - unknown

Allowed ``failure_class`` values are the Plan 050 typed blockers. They are
listed in :data:`FAILURE_CLASSES`. The generic ``blocked_cloud_init_failed``
is retained only as a compatibility alias.

Allowed ``recommended_action`` values:

  - retry-status
  - resume-provisioning
  - recreate-owned
  - repair-package-source
  - repair-cloud-init-config
  - operator-inspection-required
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1

CLOUD_INIT_STATES = frozenset(
    {
        "running",
        "done",
        "degraded",
        "error",
        "disabled",
        "timeout",
        "unknown",
    }
)

CLOUD_INIT_STAGES = frozenset(
    {
        "local",
        "network",
        "config",
        "final",
        "post-verify",
        "unknown",
    }
)

RECOMMENDED_ACTIONS = frozenset(
    {
        "retry-status",
        "resume-provisioning",
        "recreate-owned",
        "repair-package-source",
        "repair-cloud-init-config",
        "operator-inspection-required",
    }
)

EXIT_STATUS_CLASSES = frozenset({"zero", "nonzero", "absent", "unknown"})

ELAPSED_BUCKETS = frozenset({"<60s", "60-300s", "300-900s", "900-1800s", ">1800s", "unknown"})

FAILURE_CLASSES = frozenset(
    {
        "blocked_cloud_init_timeout",
        "blocked_cloud_init_terminal_error",
        "blocked_cloud_init_degraded",
        "blocked_cloud_init_status_unparseable",
        "blocked_cloud_init_package_failure",
        "blocked_cloud_init_network_failure",
        "blocked_cloud_init_sysctl_failure",
        "blocked_cloud_init_user_creation_failure",
        "blocked_cloud_init_group_contract_failure",
        "blocked_cloud_init_ownership_contract_failure",
        "blocked_cloud_init_toolchain_failure",
        "blocked_cloud_init_filesystem_permission_failure",
        "blocked_cloud_init_service_failure",
        "blocked_cloud_init_post_verify_failure",
        "blocked_cloud_init_resume_unsafe",
        "blocked_cloud_init_failed",
    }
)


def utc_now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def classify_state(value: object) -> str:
    if not isinstance(value, str):
        return "unknown"
    key = value.strip().lower()
    aliases = {
        "running": "running",
        "start": "running",
        "processing": "running",
        "in progress": "running",
        "done": "done",
        "complete": "done",
        "completed": "done",
        "success": "done",
        "succeeded": "done",
        "degraded": "degraded",
        "degraded:": "degraded",
        "partial": "degraded",
        "warning": "degraded",
        "warn": "degraded",
        "error": "error",
        "failed": "error",
        "failure": "error",
        "errored": "error",
        "fatal": "error",
        "disabled": "disabled",
        "off": "disabled",
        "stopped": "disabled",
        "timeout": "timeout",
        "timed out": "timeout",
        "timed-out": "timeout",
    }
    return aliases.get(key, "unknown")


def classify_stage(value: object) -> str:
    if not isinstance(value, str):
        return "unknown"
    key = value.strip().lower()
    aliases = {
        "local": "local",
        "init": "local",
        "init-local": "local",
        "init/local": "local",
        "network": "network",
        "init-network": "network",
        "init/network": "network",
        "config": "config",
        "init-config": "config",
        "init/config": "config",
        "modules-config": "config",
        "modules:config": "config",
        "final": "final",
        "modules-final": "final",
        "modules:final": "final",
        "post-verify": "post-verify",
        "post_verify": "post-verify",
        "postverify": "post-verify",
        "verify": "post-verify",
    }
    return aliases.get(key, "unknown")


def classify_exit_status(value: object) -> str:
    if value is None or value == "":
        return "absent"
    if isinstance(value, bool):
        return "zero" if not value else "nonzero"
    if isinstance(value, int):
        return "zero" if value == 0 else "nonzero"
    if isinstance(value, str):
        key = value.strip().lower()
        if not key or key in {"none", "null", "absent"}:
            return "absent"
        if key in {"0", "zero", "ok", "success", "succeeded"}:
            return "zero"
        if key in {"unknown", "n/a"}:
            return "unknown"
        return "nonzero"
    return "unknown"


def classify_elapsed(value: object) -> str:
    if not isinstance(value, (int, float)):
        return "unknown"
    try:
        seconds = float(value)
    except (TypeError, ValueError):
        return "unknown"
    if seconds < 60:
        return "<60s"
    if seconds < 300:
        return "60-300s"
    if seconds < 900:
        return "300-900s"
    if seconds < 1800:
        return "900-1800s"
    return ">1800s"


_HEX64 = re.compile(r"^[0-9a-f]{64}$")


def classify_failed_module(value: object) -> str:
    if not isinstance(value, str):
        return "unknown"
    key = value.strip().lower()
    if not key:
        return "unknown"
    fixed_modules = {
        "package_update": "package-update",
        "package-upgrade": "package-upgrade",
        "package_install": "package-install",
        "apt": "package-install",
        "runcmd": "runcmd",
        "bootcmd": "bootcmd",
        "write_files": "write-files",
        "users": "users",
        "groups": "groups",
        "sysctl": "sysctl",
        "set_sysctl": "sysctl",
        "timezone": "timezone",
        "hostname": "hostname",
        "ntp": "ntp",
        "snap": "snap",
        "landscape": "landscape",
        "puppet": "puppet",
        "chef": "chef",
        "salt": "salt",
        "mcollective": "mcollective",
        "rsyslog": "rsyslog",
        "users-groups": "users-groups",
        "ssh": "ssh",
        "resizefs": "resizefs",
        "set_hostname": "set-hostname",
        "update_etc_hosts": "update-etc-hosts",
        "update-etc-hosts": "update-etc-hosts",
    }
    if key in fixed_modules:
        return fixed_modules[key]
    if "package" in key or "apt" in key:
        return "package-install"
    if "sysctl" in key:
        return "sysctl"
    if "user" in key:
        return "users"
    if "group" in key:
        return "groups"
    if "write" in key or "file" in key:
        return "write-files"
    if "run" in key or "cmd" in key:
        return "runcmd"
    if "ssh" in key:
        return "ssh"
    if "ntp" in key:
        return "ntp"
    return "unknown"


def is_compatibility_alias(outcome: str) -> bool:
    return outcome == "blocked_cloud_init_failed"


def _stage_failure(stage: str) -> str:
    return {
        "local": "blocked_cloud_init_terminal_error",
        "network": "blocked_cloud_init_network_failure",
        "config": "blocked_cloud_init_terminal_error",
        "final": "blocked_cloud_init_terminal_error",
        "post-verify": "blocked_cloud_init_post_verify_failure",
        "unknown": "blocked_cloud_init_terminal_error",
    }.get(stage, "blocked_cloud_init_terminal_error")


def _module_failure(module: str) -> str:
    return {
        "package-install": "blocked_cloud_init_package_failure",
        "package-update": "blocked_cloud_init_package_failure",
        "package-upgrade": "blocked_cloud_init_package_failure",
        "sysctl": "blocked_cloud_init_sysctl_failure",
        "users": "blocked_cloud_init_user_creation_failure",
        "groups": "blocked_cloud_init_group_contract_failure",
        "users-groups": "blocked_cloud_init_group_contract_failure",
        "write-files": "blocked_cloud_init_filesystem_permission_failure",
        "runcmd": "blocked_cloud_init_terminal_error",
        "bootcmd": "blocked_cloud_init_terminal_error",
        "set-hostname": "blocked_cloud_init_terminal_error",
        "ssh": "blocked_cloud_init_service_failure",
        "ntp": "blocked_cloud_init_service_failure",
        "resizefs": "blocked_cloud_init_service_failure",
        "update-etc-hosts": "blocked_cloud_init_terminal_error",
        "unknown": "blocked_cloud_init_terminal_error",
    }.get(module, "blocked_cloud_init_terminal_error")


def classify_failure(
    *,
    state: str,
    stage: str,
    module: str | None = None,
    exit_status: str | None = None,
    retry_safe: bool = True,
) -> tuple[str, str, bool]:
    """Return ``(failure_class, recommended_action, retry_safe)``."""

    normalized_state = classify_state(state)
    normalized_stage = classify_stage(stage)
    normalized_module = classify_failed_module(module) if module is not None else "unknown"
    normalized_exit = classify_exit_status(exit_status) if exit_status is not None else "unknown"
    if normalized_state == "timeout":
        return "blocked_cloud_init_timeout", "retry-status", True
    if normalized_state == "disabled":
        return "blocked_cloud_init_terminal_error", "operator-inspection-required", False
    if normalized_state == "running":
        return "blocked_cloud_init_timeout", "retry-status", True
    if normalized_state == "degraded":
        return "blocked_cloud_init_degraded", "resume-provisioning", retry_safe
    if normalized_state == "done":
        return "blocked_cloud_init_post_verify_failure", "resume-provisioning", retry_safe
    if normalized_state in {"error", "unknown"}:
        if normalized_exit == "nonzero" and normalized_module != "unknown":
            return _module_failure(normalized_module), "retry-status", retry_safe
        if normalized_exit == "nonzero":
            return _stage_failure(normalized_stage), "retry-status", retry_safe
        if normalized_stage in {"post-verify", "final"}:
            return "blocked_cloud_init_post_verify_failure", "resume-provisioning", retry_safe
        return _stage_failure(normalized_stage), "retry-status", retry_safe
    return "blocked_cloud_init_status_unparseable", "operator-inspection-required", False


def parse_long_status(raw: str) -> dict[str, str]:
    """Parse the bounded ``cloud-init status --long`` key/value shape."""

    result: dict[str, str] = {}
    for line in raw.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        result[key.strip().lower()] = value.strip()
    return result


def parse_json_status(raw: str) -> dict[str, Any]:
    try:
        value = json.loads(raw)
    except (TypeError, ValueError):
        return {}
    if not isinstance(value, dict):
        return {}
    result: dict[str, Any] = {}
    for key, item in value.items():
        if isinstance(key, str):
            result[key.strip().lower()] = item
    return result


def parse_status(
    *,
    long_output: str | None = None,
    json_output: str | None = None,
    service_status: dict[str, str] | None = None,
    boot_finished_present: bool | None = None,
    version: str | None = None,
) -> dict[str, Any]:
    """Return the bounded sanitized record."""

    if json_output:
        parsed = parse_json_status(json_output)
    else:
        parsed = {}
    if not parsed and long_output:
        parsed = {key: value for key, value in parse_long_status(long_output).items()}
    state_value = parsed.get("status") or parsed.get("state")
    errors = parsed.get("errors") or parsed.get("error")
    stage_value = parsed.get("stage") or parsed.get("current_stage")
    module_value = parsed.get("module") or parsed.get("last_module")
    elapsed_value = parsed.get("elapsed")
    if elapsed_value is None and "last updated" in parsed:
        elapsed_value = parsed.get("elapsed") or 0
    detail = parsed.get("detail")
    if not long_output and not json_output:
        return {
            "schema_version": SCHEMA_VERSION,
            "cloud_init_state": "unknown",
            "cloud_init_stage": "unknown",
            "failure_class": "blocked_cloud_init_status_unparseable",
            "failed_module": "unknown",
            "exit_status_class": "absent",
            "boot_finished_present": boot_finished_present,
            "cloud_init_version": version or "",
            "elapsed_bucket": "unknown",
            "retry_safe": False,
            "recommended_action": "operator-inspection-required",
        }
    if state_value is None:
        return {
            "schema_version": SCHEMA_VERSION,
            "cloud_init_state": "unknown",
            "cloud_init_stage": "unknown",
            "failure_class": "blocked_cloud_init_status_unparseable",
            "failed_module": "unknown",
            "exit_status_class": "absent",
            "boot_finished_present": boot_finished_present,
            "cloud_init_version": version or "",
            "elapsed_bucket": "unknown",
            "retry_safe": False,
            "recommended_action": "operator-inspection-required",
        }
    state = classify_state(state_value)
    stage = classify_stage(stage_value)
    module = classify_failed_module(module_value)
    exit_class = classify_exit_status(
        errors if errors is not None else parsed.get("exit_status")
    )
    elapsed_bucket = classify_elapsed(elapsed_value)
    if isinstance(errors, str) and errors.strip().lower() in {"none", "null", "0"}:
        errors = None
    if state == "done" and not module_value and not errors and detail is None:
        failure_class, recommended_action, retry_safe = (
            "blocked_cloud_init_post_verify_failure",
            "resume-provisioning",
            True,
        )
    else:
        failure_class, recommended_action, retry_safe = classify_failure(
            state=state,
            stage=stage,
            module=module,
            exit_status=exit_class,
        )
    if state == "done" and not errors and module == "unknown" and stage == "unknown":
        failure_class = "blocked_cloud_init_post_verify_failure"
    if service_status is not None:
        inactive = [name for name, value in service_status.items() if value.strip().lower() != "active"]
        if inactive and state != "done":
            failure_class = "blocked_cloud_init_service_failure"
            recommended_action = "repair-cloud-init-config"
            retry_safe = False
    record = {
        "schema_version": SCHEMA_VERSION,
        "cloud_init_state": state,
        "cloud_init_stage": stage,
        "failure_class": failure_class,
        "failed_module": module,
        "exit_status_class": exit_class,
        "boot_finished_present": boot_finished_present,
        "cloud_init_version": version or "",
        "elapsed_bucket": elapsed_bucket,
        "retry_safe": retry_safe,
        "recommended_action": recommended_action,
    }
    return record


def attach_run_metadata(
    record: dict[str, Any],
    *,
    run_id: str,
    instance_generation: int,
    environment_manifest_sha256: str,
    cloud_init_sha256: str,
) -> dict[str, Any]:
    updated = dict(record)
    updated.update(
        {
            "run_id": run_id,
            "instance_generation": instance_generation,
            "environment_manifest_sha256": environment_manifest_sha256,
            "cloud_init_sha256": cloud_init_sha256,
            "completed_at_utc": utc_now(),
        }
    )
    return updated


def is_retry_safe_failure(outcome: str) -> bool:
    """Plan 050 retry-safe allowlist for ``--resume-owned``."""

    return outcome in {
        "blocked_cloud_init_timeout",
        "blocked_cloud_init_status_unparseable",
        "blocked_cloud_init_degraded",
        "blocked_cloud_init_post_verify_failure",
    }


def recommend_resume(outcome: str) -> str:
    if outcome in {
        "blocked_cloud_init_terminal_error",
        "blocked_cloud_init_package_failure",
        "blocked_cloud_init_sysctl_failure",
        "blocked_cloud_init_user_creation_failure",
        "blocked_cloud_init_group_contract_failure",
        "blocked_cloud_init_ownership_contract_failure",
        "blocked_cloud_init_filesystem_permission_failure",
        "blocked_cloud_init_service_failure",
        "blocked_cloud_init_toolchain_failure",
    }:
        return "recreate-owned"
    if outcome == "blocked_cloud_init_resume_unsafe":
        return "recreate-owned"
    if outcome in {
        "blocked_cloud_init_timeout",
        "blocked_cloud_init_status_unparseable",
        "blocked_cloud_init_degraded",
        "blocked_cloud_init_post_verify_failure",
    }:
        return "resume-provisioning"
    return "operator-inspection-required"


def _validate_record(record: dict[str, Any]) -> None:
    for field in (
        "schema_version", "cloud_init_state", "cloud_init_stage", "failure_class",
        "failed_module", "exit_status_class", "elapsed_bucket", "recommended_action",
    ):
        if field not in record:
            raise ValueError(f"missing field: {field}")
    if record["schema_version"] != SCHEMA_VERSION:
        raise ValueError("unsupported schema version")
    if record["cloud_init_state"] not in CLOUD_INIT_STATES:
        raise ValueError("invalid cloud_init_state")
    if record["cloud_init_stage"] not in CLOUD_INIT_STAGES:
        raise ValueError("invalid cloud_init_stage")
    if record["failure_class"] not in FAILURE_CLASSES:
        raise ValueError("invalid failure_class")
    if record["exit_status_class"] not in EXIT_STATUS_CLASSES:
        raise ValueError("invalid exit_status_class")
    if record["elapsed_bucket"] not in ELAPSED_BUCKETS:
        raise ValueError("invalid elapsed_bucket")
    if record["recommended_action"] not in RECOMMENDED_ACTIONS:
        raise ValueError("invalid recommended_action")
    if not isinstance(record["retry_safe"], bool):
        raise ValueError("retry_safe must be a bool")
    if "environment_manifest_sha256" in record and not _HEX64.fullmatch(record["environment_manifest_sha256"]):
        raise ValueError("environment_manifest_sha256 is not a SHA-256")
    if "cloud_init_sha256" in record and not _HEX64.fullmatch(record["cloud_init_sha256"]):
        raise ValueError("cloud_init_sha256 is not a SHA-256")


def _main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="operation", required=True)
    classify = sub.add_parser("classify")
    classify.add_argument("--long-output")
    classify.add_argument("--json-output")
    classify.add_argument("--service-status", default="{}")
    classify.add_argument("--boot-finished-present", choices=["yes", "no", "unknown"], default="unknown")
    classify.add_argument("--version", default="")
    record_run = sub.add_parser("attach-run")
    record_run.add_argument("--path", type=Path, required=True)
    for name in ("run-id", "instance-generation", "environment-manifest-sha256", "cloud-init-sha256"):
        record_run.add_argument(f"--{name}", required=True)
    args = parser.parse_args()
    try:
        if args.operation == "classify":
            services = json.loads(args.service_status) if args.service_status else {}
            if not isinstance(services, dict):
                raise ValueError("service status is not an object")
            boot_finished = {"yes": True, "no": False, "unknown": None}[args.boot_finished_present]
            value = parse_status(
                long_output=args.long_output,
                json_output=args.json_output,
                service_status={str(k): str(v) for k, v in services.items()},
                boot_finished_present=boot_finished,
                version=args.version or None,
            )
            _validate_record(value)
            print(json.dumps(value, sort_keys=True, separators=(",", ":")))
        elif args.operation == "attach-run":
            value = json.loads(args.path.read_text(encoding="utf-8"))
            updated = attach_run_metadata(
                value,
                run_id=args.run_id,
                instance_generation=int(args.instance_generation),
                environment_manifest_sha256=args.environment_manifest_sha256,
                cloud_init_sha256=args.cloud_init_sha256,
            )
            _validate_record(updated)
            updated["schema_version"] = SCHEMA_VERSION
            args.path.write_text(
                json.dumps(updated, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            print(json.dumps(updated, sort_keys=True, separators=(",", ":")))
    except (ValueError, OSError, json.JSONDecodeError) as exc:
        print(json.dumps({"schema": 1, "type": "cloud-init-status", "outcome": "blocked_cloud_init_status_unparseable", "detail": str(exc)}, separators=(",", ":")))
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(_main())
