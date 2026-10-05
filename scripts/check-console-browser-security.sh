#!/usr/bin/env bash
# Router-console browser-security guard (Plan 357).
#
# This script enforces the invariants that keep the browser boundary
# honest. It is a *strengthening* guard: fix the code, never weaken this
# script. Where a rule cannot be expressed as a grep, the named test must
# exist and must be present in the suite, so removing a test fails here
# rather than silently reducing coverage.
#
# Rules enforced:
#   1. The console is validated against a loopback authority in code.
#   2. No CORS header name appears in console source.
#   3. The centralized CSP forbids inline and remote code.
#   4. Session cookies are HttpOnly + SameSite=Strict + path-scoped.
#   5. Password material never renders through Debug/Display.
#   6. The browser secret type is not Clone and zeroizes on drop.
#   7. No trusted-proxy header is consulted for authority decisions.
#   8. The security test suites exist and are wired into the workspace.

set -euo pipefail

cd "$(dirname "$0")/.."

CONSOLE="crates/i2pr-console"
CONSOLE_SRC="${CONSOLE}/src"
DAEMON_CONFIG="crates/i2pr-daemon/src/config.rs"
failures=0

fail() {
    printf 'check-console-browser-security: FAIL: %s\n' "$1" >&2
    failures=$((failures + 1))
}

# Prints non-comment, non-doc-comment lines of the given files, so a rule can
# name the API it forbids without the file's own documentation tripping it.
code_only() {
    cat "$@" 2>/dev/null | grep -vE '^[[:space:]]*(//|/\*|\*|#)' || true
}

code_contains() {
    pattern="$1"
    shift
    code_only "$@" | grep -qE "${pattern}"
}

require_absent() {
    description="$1"
    pattern="$2"
    shift 2
    if code_contains "${pattern}" "$@"; then
        fail "${description}"
    fi
}

# --- rule 1: loopback-only authority ---------------------------------------

if ! grep -q 'NonLoopbackListener' "${CONSOLE_SRC}/security/authority.rs"; then
    fail "the authority policy must refuse a non-loopback listener"
fi
if ! code_contains 'is_loopback' "${CONSOLE_SRC}/security/authority.rs"; then
    fail "the authority policy must check is_loopback"
fi
# The daemon configuration must refuse a non-loopback console bind too.
if ! grep -q 'console.bind_address' "${DAEMON_CONFIG}"; then
    fail "the daemon must reject a non-loopback console bind address"
fi

# --- rule 2: no permissive CORS -------------------------------------------

# A `access-control-*` name may appear only inside the denylist that detects
# one, and inside the test that proves the detector fires. Anywhere else it
# would be an emitted permissive header.
cors_hits=$(awk '
    /static CORS_HEADER_NAMES/ { in_denylist = 1 }
    in_denylist && /\];/ { in_denylist = 0; next }
    in_denylist { next }
    /mod tests/ { in_tests = 1 }
    !in_tests && /access-control-/ { print FILENAME ":" FNR ":" $0 }
' $(find "${CONSOLE_SRC}" -name '*.rs') || true)
if [ -n "${cors_hits}" ]; then
    fail "console source must not emit CORS headers: ${cors_hits}"
fi

# --- rule 3: the centralized CSP -------------------------------------------

CSP_FILE="${CONSOLE_SRC}/security/headers.rs"
if ! grep -q "default-src 'none'" "${CSP_FILE}"; then
    fail "the CSP must start from default-src 'none'"
fi
# Read only the constant's own definition, not the tests that assert on it.
csp_definition=$(awk '
    /pub const CONTENT_SECURITY_POLICY/ { capture = 1 }
    capture { print }
    capture && /;[[:space:]]*$/ { exit }
' "${CSP_FILE}")
if printf '%s' "${csp_definition}" | grep -q 'unsafe-'; then
    fail "the CSP must not contain any unsafe- escape hatch"
fi
if printf '%s' "${csp_definition}" | grep -qE 'https?:|\*'; then
    fail "the CSP must not authorize a remote origin or wildcard source"
fi
if ! grep -q "frame-ancestors 'none'" "${CSP_FILE}"; then
    fail "the CSP must deny framing"
fi
if ! grep -q 'nosniff' "${CSP_FILE}"; then
    fail "responses must carry X-Content-Type-Options: nosniff"
fi

# --- rule 4: cookie attributes --------------------------------------------

for attribute in HttpOnly SameSite=Strict; do
    if ! grep -q "${attribute}" "${CSP_FILE}"; then
        fail "the session cookie must declare ${attribute}"
    fi
done
if ! grep -q 'Max-Age=0' "${CSP_FILE}"; then
    fail "cookie clearing must use a matching Max-Age=0"
fi
# A Domain attribute would widen the cookie to unrelated local hosts.
if code_only "${CSP_FILE}" | grep -qE 'SESSION_COOKIE_NAME}?=[^;]*[Dd]omain='; then
    fail "the session cookie must not carry a Domain attribute"
fi

# --- rule 5: secrets never render -----------------------------------------

SECRET_FILE="${CONSOLE_SRC}/secret.rs"
if ! grep -q '<redacted>' "${SECRET_FILE}"; then
    fail "the console secret must redact its value"
fi
require_absent "the console secret must not derive Debug" \
    '#\[derive\([^)]*Debug' "${SECRET_FILE}"
if ! grep -q 'impl fmt::Display for ConsoleSecret' "${SECRET_FILE}"; then
    fail "the console secret must implement Display"
fi
# The password hash is an offline verifier and must not be printed either.
if ! grep -q 'ConsolePasswordHash(<redacted>)' "${DAEMON_CONFIG}"; then
    fail "the daemon password hash must redact its value"
fi
if ! grep -q 'password", &"<redacted>"' "${DAEMON_CONFIG}"; then
    fail "the raw console configuration Debug must redact the password"
fi

# --- rule 6: browser credentials are not Clone and are zeroized -----------

if grep -qE '#\[derive\([^)]*Clone[^)]*\)\]' "${SECRET_FILE}"; then
    fail "the console secret must not be Clone"
fi
if ! grep -q 'ZeroizeOnDrop' "${SECRET_FILE}"; then
    fail "the console secret must zeroize on drop"
fi

# --- rule 7: no trusted-proxy authority -----------------------------------

require_absent "authority must not trust X-Forwarded-Host" \
    'X-Forwarded-Host|x-forwarded-host' "${CONSOLE_SRC}/security/authority.rs"
require_absent "authority must not trust the Forwarded header" \
    '\bForwarded\b' "${CONSOLE_SRC}/security/authority.rs"
require_absent "the console must not run in a trusted-proxy mode" \
    'trusted_proxy' "${CONSOLE_SRC}"

# --- rule 8: the suites exist ---------------------------------------------

for suite in \
    "${CONSOLE}/tests/console_browser_security.rs" \
    "${CONSOLE}/tests/console_routes.rs"; do
    if [ ! -f "${suite}" ]; then
        fail "missing required test suite: ${suite}"
    fi
done

# Cargo auto-discovers `tests/*.rs`, so an empty file would be discovered and
# assert nothing. Require real test cases in each suite.
for suite in \
    "${CONSOLE}/tests/console_browser_security.rs" \
    "${CONSOLE}/tests/console_routes.rs"; do
    cases=$(grep -cE '^#\[test\]|^    fn [a-z_]+\(' "${suite}" 2>/dev/null || true)
    if [ "${cases}" -lt 5 ]; then
        fail "${suite} must contain real test cases, found ${cases}"
    fi
done

# The named behavioural tests must exist: deleting one is a coverage loss.
for test_name in \
    cross_origin_unsafe_requests_are_refused_before_the_handler \
    requests_with_a_foreign_authority_are_refused \
    requests_without_an_authority_are_refused \
    authority_validation_applies_in_authenticated_mode_too \
    a_valid_session_plus_csrf_token_permits_the_mutation \
    a_valid_session_without_the_csrf_token_is_refused \
    a_forged_session_cookie_is_refused \
    no_response_ever_carries_a_cors_header \
    login_throttle_blocks_repeated_failures \
    logout_revokes_the_session_so_it_cannot_be_replayed; do
    if ! grep -rq "fn ${test_name}" "${CONSOLE}/tests" "${CONSOLE}/src"; then
        fail "required browser-security test is missing: ${test_name}"
    fi
done

if [ "${failures}" -ne 0 ]; then
    printf 'check-console-browser-security: %d violation(s)\n' "${failures}" >&2
    exit 1
fi

printf 'check-console-browser-security: ok\n'