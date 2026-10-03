#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
evidence_root=${1:-"${repo_root}/target/interop/anonymity/http-profile-evidence"}
if [[ "${evidence_root}" != /* ]]; then evidence_root="${repo_root}/${evidence_root}"; fi
python3 "${repo_root}/scripts/check-http-anonymity-evidence.py" "${repo_root}" "${evidence_root}"
