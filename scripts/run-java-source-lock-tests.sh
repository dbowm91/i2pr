#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${I2PR_M6_JAVA_SOURCE_ROOT:-}" ]]; then
  echo "I2PR_M6_JAVA_SOURCE_ROOT must name the exact-pinned Java I2P 2.13.0 source tree" >&2
  exit 2
fi
source_root="${I2PR_M6_JAVA_SOURCE_ROOT}"
if [[ ! -d "${source_root}/.git" && ! -f "${source_root}/.git" ]]; then
  echo "Java source-lock root is not a Git checkout: ${source_root}" >&2
  exit 2
fi
actual_pin="$(git -C "${source_root}" rev-parse HEAD)"
expected_pin="9134f808337b401e8e53c73734c81fab04280c9d"
if [[ "${actual_pin}" != "${expected_pin}" ]]; then
  echo "Java source-lock pin mismatch: expected ${expected_pin}, found ${actual_pin}" >&2
  exit 2
fi
for relative in \
  apps/streaming/java/src/net/i2p/client/streaming/impl/ConnectionOptions.java \
  apps/streaming/java/src/net/i2p/client/streaming/impl/ConnectionPacketHandler.java \
  apps/streaming/java/src/net/i2p/client/streaming/impl/SchedulerImpl.java \
  apps/streaming/java/src/net/i2p/client/streaming/impl/Connection.java \
  apps/streaming/java/src/net/i2p/client/streaming/impl/SchedulerChooser.java \
  core/java/src/net/i2p/util/SimpleTimer2.java; do
  if [[ ! -f "${source_root}/${relative}" ]]; then
    echo "Java source-lock input is missing: ${relative}" >&2
    exit 2
  fi
done

tests=(
  p246_default_ack_delay_is_500_on_frozen_helper
  p246_packet_handler_sets_deadline_before_event
  p246_received_reschedule_calls_connection_timer
  p246_transition_add_event_uses_fresh_wrapper
  p246_timer_wrapper_delegates_to_connection_event
  p246_connection_event_reenters_scheduler_chooser
  p246_scheduler_precedence_is_source_locked
  p247_no_ack_delay_override
)
for test_name in "${tests[@]}"; do
  I2PR_M6_JAVA_SOURCE_ROOT="${source_root}" cargo test --locked -p i2pr-daemon \
    --test java_tunnel_external "${test_name}" -- --ignored --exact --test-threads=1
done
