#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_file="${repo_root}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
runner="${repo_root}/scripts/run-java-source-lock-tests.sh"
workflow="${repo_root}/.github/workflows/ci.yml"
expected=(
  p246_default_ack_delay_is_500_on_frozen_helper
  p246_packet_handler_sets_deadline_before_event
  p246_received_reschedule_calls_connection_timer
  p246_transition_add_event_uses_fresh_wrapper
  p246_timer_wrapper_delegates_to_connection_event
  p246_connection_event_reenters_scheduler_chooser
  p246_scheduler_precedence_is_source_locked
  p247_no_ack_delay_override
)
fail() { echo "Java source-lock gating check failed: $*" >&2; exit 1; }
[[ -f "${runner}" ]] || fail "missing explicit source-lock runner"
grep -q '9134f808337b401e8e53c73734c81fab04280c9d' "${test_file}" || fail "test pin changed or missing"
grep -q '9134f808337b401e8e53c73734c81fab04280c9d' "${runner}" || fail "runner pin changed or missing"
grep -q 'I2PR_M6_JAVA_SOURCE_ROOT' "${test_file}" || fail "source helper does not require configured root"
if grep -q 'target.*interop.*m6-java-sources' "${test_file}"; then
  fail "test file still guesses the checkout under target/"
fi
if grep -q 'streaming_through_java' "${runner}"; then
  fail "runner includes the live Java Streaming test"
fi
if grep -q 'I2PR_M6_JAVA_SOURCE_ROOT' "${workflow}"; then
  fail "ordinary CI injects a Java source path"
fi
actual_tests=()
for test_name in "${expected[@]}"; do
  grep -Fq "#[ignore = \"requires exact-pinned Java I2P 2.13.0 source tree\"]" "${test_file}" || fail "ignore reason missing"
  grep -Fq "fn ${test_name}()" "${test_file}" || fail "missing gated assertion ${test_name}"
  grep -Fq "  ${test_name}" "${runner}" || fail "runner omits ${test_name}"
  actual_tests+=("${test_name}")
done
for test_name in p246_default_ack_delay_is_500_on_frozen_helper p246_packet_handler_sets_deadline_before_event p246_received_reschedule_calls_connection_timer p246_transition_add_event_uses_fresh_wrapper p246_timer_wrapper_delegates_to_connection_event p246_connection_event_reenters_scheduler_chooser p246_scheduler_precedence_is_source_locked p247_no_ack_delay_override; do
  count="$(grep -Fc "  ${test_name}" "${runner}")"
  [[ "${count}" == 1 ]] || fail "runner test list has duplicate/missing ${test_name}"
done
for assertion in 'DEFAULT_INITIAL_ACK_DELAY = 500' 'setNextSendTime(delay + context.clock().now())' 'con.scheduleConnectionEvent(msToWait)' 'new TimedEvent(this, timeoutMs)' 'event.timeReached()' '_chooser.getScheduler(this)' 'SchedulerHardDisconnected' 'SchedulerDead'; do
  grep -Fq "${assertion}" "${test_file}" || fail "source-lock assertion removed: ${assertion}"
done
echo "Java source-lock gating check passed (8 exact-pinned tests, ordinary CI remains source-free)"
