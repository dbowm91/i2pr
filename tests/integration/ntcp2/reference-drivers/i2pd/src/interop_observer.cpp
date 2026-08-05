// Plan 064/083 i2pd passive observer implementation.
//
// The observer sink is a process-local bounded set of "last observation"
// slots guarded by a mutex and three atomic counters (received, sent,
// authenticated). The transport threads call the observer via the
// ``noexcept`` Plan 064 seam, the observer copies the metadata into the
// slot, and the Plan 083 wait primitives poll the counters from the
// driver thread. The observer never blocks the transport thread, never
// opens a socket, never logs, and never alters the transport control
// flow.
//
// The implementation is `noexcept` so a transport-side exception cannot
// leak into the pinned i2pd transport state.

#include "interop_observer.h"

#include <atomic>
#include <chrono>
#include <cstring>
#include <mutex>
#include <thread>

namespace i2pr {
namespace i2pdinterop {

namespace {

// The observer sink is a single owner that the driver initializes at
// process start and resets at shutdown. The counters use
// std::atomic<std::uint64_t> so the receive and send threads may write
// concurrently without a lock. The metadata slots are protected by a
// mutex so the wait primitive can read them coherently.
std::atomic<std::uint64_t> g_received_count{0};
std::atomic<std::uint64_t> g_sent_count{0};
std::atomic<std::uint64_t> g_authenticated_count{0};
std::atomic<std::uint64_t> g_tcp_accepted_count{0};
std::atomic<std::uint64_t> g_drop_count{0};

std::mutex g_slot_mutex;
ObserverMetadata g_last_received{};
ObserverMetadata g_last_sent{};
ObserverMetadata g_last_authenticated{};
ObserverMetadata g_last_tcp_accepted{};

void RecordObservation(ObserverMetadata& slot,
                       const ObserverMetadata& metadata,
                       std::atomic<std::uint64_t>& counter) noexcept {
    try {
        std::lock_guard<std::mutex> guard(g_slot_mutex);
        std::memcpy(&slot, &metadata, sizeof(ObserverMetadata));
        counter.fetch_add(1, std::memory_order_release);
    } catch (...) {
        g_drop_count.fetch_add(1, std::memory_order_relaxed);
    }
}

bool WaitForCounter(std::atomic<std::uint64_t>& counter,
                    std::uint32_t timeout_ms) noexcept {
    const auto deadline =
        std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);
    while (std::chrono::steady_clock::now() < deadline) {
        if (counter.load(std::memory_order_acquire) > 0) {
            return true;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    return false;
}

}  // namespace

#ifdef I2PD_INTEROP_OBSERVER

void ObserveReceivedI2NP(const ObserverMetadata& metadata) noexcept {
    RecordObservation(g_last_received, metadata, g_received_count);
}

void ObserveSentI2NP(const ObserverMetadata& metadata) noexcept {
    RecordObservation(g_last_sent, metadata, g_sent_count);
}

void ObserveAuthenticated(const ObserverMetadata& metadata) noexcept {
    RecordObservation(g_last_authenticated, metadata, g_authenticated_count);
}

void ObserveTcpAccepted(const ObserverMetadata& metadata) noexcept {
    RecordObservation(g_last_tcp_accepted, metadata, g_tcp_accepted_count);
}

void ResetObserverSink() noexcept {
    std::lock_guard<std::mutex> guard(g_slot_mutex);
    g_received_count.store(0, std::memory_order_relaxed);
    g_sent_count.store(0, std::memory_order_relaxed);
    g_authenticated_count.store(0, std::memory_order_relaxed);
    g_tcp_accepted_count.store(0, std::memory_order_relaxed);
    g_drop_count.store(0, std::memory_order_relaxed);
    std::memset(&g_last_received, 0, sizeof(ObserverMetadata));
    std::memset(&g_last_sent, 0, sizeof(ObserverMetadata));
    std::memset(&g_last_authenticated, 0, sizeof(ObserverMetadata));
    std::memset(&g_last_tcp_accepted, 0, sizeof(ObserverMetadata));
}

std::uint64_t ObserverDropCount() noexcept {
    return g_drop_count.load(std::memory_order_relaxed);
}

std::uint64_t ObserverObservationCount() noexcept {
    return g_received_count.load(std::memory_order_relaxed) +
           g_sent_count.load(std::memory_order_relaxed) +
           g_authenticated_count.load(std::memory_order_relaxed) +
           g_tcp_accepted_count.load(std::memory_order_relaxed);
}

bool WaitForAuthenticated(ObserverMetadata& metadata,
                          std::uint32_t timeout_ms) noexcept {
    if (!WaitForCounter(g_authenticated_count, timeout_ms)) {
        return false;
    }
    try {
        std::lock_guard<std::mutex> guard(g_slot_mutex);
        std::memcpy(&metadata, &g_last_authenticated, sizeof(ObserverMetadata));
        return true;
    } catch (...) {
        return false;
    }
}

bool WaitForReceivedI2NP(ObserverMetadata& metadata,
                         std::uint32_t timeout_ms) noexcept {
    if (!WaitForCounter(g_received_count, timeout_ms)) {
        return false;
    }
    try {
        std::lock_guard<std::mutex> guard(g_slot_mutex);
        std::memcpy(&metadata, &g_last_received, sizeof(ObserverMetadata));
        return true;
    } catch (...) {
        return false;
    }
}

bool WaitForSentI2NP(ObserverMetadata& metadata,
                     std::uint32_t timeout_ms) noexcept {
    if (!WaitForCounter(g_sent_count, timeout_ms)) {
        return false;
    }
    try {
        std::lock_guard<std::mutex> guard(g_slot_mutex);
        std::memcpy(&metadata, &g_last_sent, sizeof(ObserverMetadata));
        return true;
    } catch (...) {
        return false;
    }
}

bool WaitForTcpAccepted(ObserverMetadata& metadata,
                        std::uint32_t timeout_ms) noexcept {
    if (!WaitForCounter(g_tcp_accepted_count, timeout_ms)) {
        return false;
    }
    try {
        std::lock_guard<std::mutex> guard(g_slot_mutex);
        std::memcpy(&metadata, &g_last_tcp_accepted, sizeof(ObserverMetadata));
        return true;
    } catch (...) {
        return false;
    }
}

#endif

}  // namespace i2pdinterop
}  // namespace i2pr
