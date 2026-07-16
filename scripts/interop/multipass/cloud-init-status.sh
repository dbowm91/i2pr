#!/usr/bin/env bash
# Plan 050: capture a sanitized cloud-init status record.
#
# Reads structured cloud-init status and the service state for the four
# canonical units, classifies the result via cloud_init_status.py, and
# writes a sanitized JSON record to --output. It never retains raw
# cloud-init logs, raw journald output, or arbitrary stderr.
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

output_path=""
service_status='{}'
version=""
boot_finished="unknown"
json_output=""
long_output=""
while (($#)); do
  case "$1" in
    --output)
      [[ -z "$output_path" && $# -ge 2 ]] || die "duplicate or incomplete --output"
      output_path=$2
      shift
      ;;
    --service-status)
      [[ "$service_status" == '{}' && $# -ge 2 ]] || die "duplicate --service-status"
      service_status=$2
      shift
      ;;
    --version)
      [[ -z "$version" && $# -ge 2 ]] || die "duplicate --version"
      version=$2
      shift
      ;;
    --boot-finished)
      [[ "$boot_finished" == "unknown" && $# -ge 2 ]] || die "duplicate --boot-finished"
      boot_finished=$2
      shift
      ;;
    --json-output)
      [[ -z "$json_output" && $# -ge 2 ]] || die "duplicate --json-output"
      json_output=$2
      shift
      ;;
    --long-output)
      [[ -z "$long_output" && $# -ge 2 ]] || die "duplicate --long-output"
      long_output=$2
      shift
      ;;
    --help|-h) printf 'usage: cloud-init-status.sh [--output <path>] [--service-status <json>] [--version <v>] [--boot-finished yes|no|unknown] [--json-output <text>] [--long-output <text>]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

if [[ -z "$output_path" ]]; then
  die "--output is required"
fi

require_command python3
ensure_dirs

if [[ -z "$json_output" && -z "$long_output" ]]; then
  long_output=$(guest_admin_exec cloud-init status --long 2>/dev/null || true)
  if [[ -z "$json_output" ]]; then
    json_output=$(guest_admin_exec cloud-init status --format json 2>/dev/null || true)
  fi
  if [[ -z "$version" ]]; then
    version=$(guest_admin_exec cloud-init --version 2>/dev/null || true)
  fi
fi

if [[ -z "$service_status" || "$service_status" == '{}' ]]; then
  cloud_init_local=$(guest_root_exec systemctl is-active cloud-init-local.service 2>/dev/null || true)
  cloud_init_service=$(guest_root_exec systemctl is-active cloud-init.service 2>/dev/null || true)
  cloud_config=$(guest_root_exec systemctl is-active cloud-config.service 2>/dev/null || true)
  cloud_final=$(guest_root_exec systemctl is-active cloud-final.service 2>/dev/null || true)
  service_status=$(printf '{"cloud-init-local.service":"%s","cloud-init.service":"%s","cloud-config.service":"%s","cloud-final.service":"%s"}' \
    "$cloud_init_local" "$cloud_init_service" "$cloud_config" "$cloud_final")
fi

if [[ "$boot_finished" == "unknown" ]]; then
  if guest_admin_exec test -f /var/lib/cloud/instance/boot-finished >/dev/null 2>&1; then
    boot_finished="yes"
  else
    boot_finished="no"
  fi
fi

python3 - "$long_output" "$json_output" "$service_status" "$boot_finished" "$version" "$output_path" <<'PY'
import json, os, sys
from pathlib import Path
sys.path.insert(0, "/home/sugarwookie/projects/i2pr/scripts/interop/multipass")
from cloud_init_status import parse_status
long_output = sys.argv[1] or None
json_output = sys.argv[2] or None
service_status = json.loads(sys.argv[3] or "{}")
boot_finished = {"yes": True, "no": False, "unknown": None}[sys.argv[4]]
version = sys.argv[5] or None
output_path = sys.argv[6]
try:
    record = parse_status(
        long_output=long_output,
        json_output=json_output,
        service_status=service_status,
        boot_finished_present=boot_finished,
        version=version,
    )
except Exception as exc:
    record = {
        "schema_version": 1,
        "cloud_init_state": "unknown",
        "cloud_init_stage": "unknown",
        "failure_class": "blocked_cloud_init_status_unparseable",
        "failed_module": "unknown",
        "exit_status_class": "unknown",
        "boot_finished_present": None,
        "cloud_init_version": "",
        "elapsed_bucket": "unknown",
        "retry_safe": False,
        "recommended_action": "operator-inspection-required",
    }
Path(output_path).parent.mkdir(parents=True, exist_ok=True)
Path(output_path).write_text(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
os.chmod(output_path, 0o600)
print(json.dumps(record, sort_keys=True, separators=(",", ":")))
PY
