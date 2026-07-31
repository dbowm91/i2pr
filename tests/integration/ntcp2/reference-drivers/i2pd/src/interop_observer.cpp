// Plan 064 i2pd passive observer implementation.
//
// The observer sink is a process-local bounded counter; the observer
// records the count of successfully emitted observations and the count
// of dropped observations when the sink is unavailable. The driver
// reads the counters through the Plan 062 reference-event v1 stream;
// the observer itself never opens a socket, never logs, and never
// blocks the transport thread.
//
// The implementation is `noexcept` so a transport-side exception
// cannot leak into the pinned i2pd transport state. The observer
// returns `void` and never alters the control flow, the return value,
// the buffering, the cryptographic state, the framing, the timing
// decisions, the routing, or the retry policy of the transport.

#include "interop_observer.h"

#include <atomic>
#include <cstring>

namespace i2pr {
namespace i2pdinterop {

namespace {

// The observer sink is a single owner that the driver initializes at
// process start and resets at shutdown. The counter uses
// std::atomic<std::uint64_t> so the receive and send threads may write
// concurrently without a lock.
std::atomic<std::uint64_t> g_observation_count{0};
std::atomic<std::uint64_t> g_drop_count{0};

}  // namespace

#ifdef I2PD_INTEROP_OBSERVER

void ObserveReceivedI2NP(const ObserverMetadata& /*metadata*/) noexcept {
    try {
        g_observation_count.fetch_add(1, std::memory_order_relaxed);
    } catch (...) {
        g_drop_count.fetch_add(1, std::memory_order_relaxed);
    }
}

void ObserveSentI2NP(const ObserverMetadata& /*metadata*/) noexcept {
    try {
        g_observation_count.fetch_add(1, std::memory_order_relaxed);
    } catch (...) {
        g_drop_count.fetch_add(1, std::memory_order_relaxed);
    }
}

void ResetObserverSink() noexcept {
    g_observation_count.store(0, std::memory_order_relaxed);
    g_drop_count.store(0, std::memory_order_relaxed);
}

std::uint64_t ObserverDropCount() noexcept {
    return g_drop_count.load(std::memory_order_relaxed);
}

std::uint64_t ObserverObservationCount() noexcept {
    return g_observation_count.load(std::memory_order_relaxed);
}

#endif

}  // namespace i2pdinterop
}  // namespace i2pr