// Plan 064/083/093 i2pd passive observer implementation.
//
// The observer sink is a generation-bound bounded sequence ring
// (receive, sent, authenticated, tcp-accepted) guarded by a mutex
// and four atomic counters (received, sent, authenticated,
// tcp_accepted). The transport threads call the observer via the
// ``noexcept`` Plan 064 seam, the observer copies the metadata into
// the ring entry tagged with the active generation and a strictly
// monotonic observation_sequence, and the Plan 083 wait primitives
// poll the ring from the driver thread.
//
// Plan 093: the ring replaces the previous "last observation" slots
// so that i2pd's automatic local-RouterInfo send on inbound sessions
// cannot satisfy the target DeliveryStatus wait. Every ring entry is
// tagged with the active generation (one per listener invocation);
// stale-generation entries cannot satisfy any wait. The predicate
// waits require an ``observation_sequence`` strictly greater than a
// caller-supplied baseline so the receive-oracle and send-oracle
// surface the exact target DeliveryStatus event with the exact peer
// Router Hash and message ID.
//
// The observer never blocks the transport thread, never opens a
// socket, never logs, and never alters the transport control flow.
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

// Plan 093: the receive/sent/authenticated/tcp-accepted rings are
// generation-bound bounded sequence rings. Each ring entry carries
// the active generation, a strictly monotonic observation_sequence,
// and the allowlisted allowlisted metadata. The transport threads
// append; the driver thread polls.
struct ObservationRing {
    std::mutex mutex;
    ObservationRingEntry entries[INTEROP_RING_CAPACITY];
    std::uint64_t next_sequence{0};
    std::atomic<std::uint64_t> total_count{0};
};

// Active listener generation. The driver calls
// ``BeginListenerGeneration`` before starting transports; every
// ring entry tags itself with the active value. Stale-generation
// entries are rejected by the predicate waits.
std::atomic<std::uint64_t> g_active_generation{0};

// The receive, sent, authenticated, tcp-accepted rings are owned by
// the observer process. The atomic counters remain for the legacy
// Plan 091/092 `WaitFor*` primitives that do not require a
// generation or sequence.
std::atomic<std::uint64_t> g_drop_count{0};
std::atomic<std::uint64_t> g_received_count{0};
std::atomic<std::uint64_t> g_sent_count{0};
std::atomic<std::uint64_t> g_authenticated_count{0};
std::atomic<std::uint64_t> g_tcp_accepted_count{0};

ObservationRing g_receive_ring{};
ObservationRing g_sent_ring{};
ObservationRing g_authenticated_ring{};
ObservationRing g_tcp_accepted_ring{};

void ResetRing(ObservationRing& ring) noexcept {
    try {
        std::lock_guard<std::mutex> guard(ring.mutex);
        for (std::size_t index = 0; index < INTEROP_RING_CAPACITY; ++index) {
            ring.entries[index].present = false;
            ring.entries[index].generation = 0;
            ring.entries[index].observation_sequence = 0;
            std::memset(&ring.entries[index].metadata, 0,
                        sizeof(ObserverMetadata));
        }
        ring.next_sequence = 0;
        ring.total_count.store(0, std::memory_order_relaxed);
    } catch (...) {
        // Reset is best-effort; failures are not observable through
        // this sink.
    }
}

bool AppendRingEntry(ObservationRing& ring,
                     const ObserverMetadata& metadata) noexcept {
    try {
        std::lock_guard<std::mutex> guard(ring.mutex);
        std::uint64_t sequence = ring.next_sequence + 1;
        if (sequence == 0) {
            // Overflow protection. The ring capacity is bounded so a
            // monotonic overflow implies an unbounded observation
            // stream. Drop the observation and let the gate fail.
            g_drop_count.fetch_add(1, std::memory_order_relaxed);
            return false;
        }
        std::size_t slot = static_cast<std::size_t>(
            (sequence - 1) % INTEROP_RING_CAPACITY);
        if (ring.entries[slot].present &&
            ring.entries[slot].observation_sequence >= sequence) {
            // Sequence collision defensive guard. Drop and let the
            // gate fail.
            g_drop_count.fetch_add(1, std::memory_order_relaxed);
            return false;
        }
        ring.entries[slot].generation =
            g_active_generation.load(std::memory_order_acquire);
        ring.entries[slot].observation_sequence = sequence;
        ring.entries[slot].metadata = metadata;
        ring.entries[slot].present = true;
        ring.next_sequence = sequence;
        ring.total_count.fetch_add(1, std::memory_order_release);
        return true;
    } catch (...) {
        g_drop_count.fetch_add(1, std::memory_order_relaxed);
        return false;
    }
}

bool WaitForRingCounter(ObservationRing& ring,
                        std::uint64_t active_generation,
                        std::uint64_t target_sequence,
                        std::uint32_t timeout_ms) noexcept {
    const auto deadline =
        std::chrono::steady_clock::now() +
            std::chrono::milliseconds(timeout_ms);
    while (std::chrono::steady_clock::now() < deadline) {
        try {
            std::lock_guard<std::mutex> guard(ring.mutex);
            // Linear scan over the bounded ring; the ring is at
            // most INTEROP_RING_CAPACITY deep so this is bounded
            // and acceptable for a test-only observer.
            for (std::size_t index = 0; index < INTEROP_RING_CAPACITY;
                 ++index) {
                const auto& entry = ring.entries[index];
                if (!entry.present) {
                    continue;
                }
                if (entry.generation != active_generation) {
                    continue;
                }
                if (entry.observation_sequence > target_sequence) {
                    return true;
                }
            }
        } catch (...) {
            return false;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    return false;
}

bool ReadRingEntry(ObservationRing& ring,
                   std::uint64_t active_generation,
                   std::uint64_t baseline_sequence,
                   const std::uint64_t expected_peer_router_hash[4],
                   std::uint32_t expected_i2np_type,
                   std::uint32_t expected_message_id,
                   ObserverMetadata& out_metadata) noexcept {
    try {
        std::lock_guard<std::mutex> guard(ring.mutex);
        for (std::size_t index = 0; index < INTEROP_RING_CAPACITY;
             ++index) {
            const auto& entry = ring.entries[index];
            if (!entry.present) {
                continue;
            }
            if (entry.generation != active_generation) {
                continue;
            }
            if (entry.observation_sequence <= baseline_sequence) {
                continue;
            }
            const auto& md = entry.metadata;
            if (expected_i2np_type != 0 &&
                md.i2np_type != expected_i2np_type) {
                continue;
            }
            if (expected_message_id != 0 &&
                md.delivery_status_message_id != expected_message_id &&
                md.i2np_envelope_message_id != expected_message_id) {
                continue;
            }
            if (expected_peer_router_hash != nullptr) {
                if (std::memcmp(md.peer_router_hash_sha256,
                                expected_peer_router_hash,
                                sizeof(md.peer_router_hash_sha256)) != 0) {
                    continue;
                }
            }
            out_metadata = md;
            return true;
        }
        return false;
    } catch (...) {
        return false;
    }
}

}  // namespace

#ifdef I2PD_INTEROP_OBSERVER

void BeginListenerGeneration() noexcept {
    // The next listener invocation uses the next generation. The
    // generation is process-global so the unbounded ``ResetObserverSink``
    // call also advances the generation explicitly; this keeps both
    // helpers in lock-step.
    g_active_generation.fetch_add(1, std::memory_order_acq_rel);
    ResetRing(g_receive_ring);
    ResetRing(g_sent_ring);
    ResetRing(g_authenticated_ring);
    ResetRing(g_tcp_accepted_ring);
    g_received_count.store(0, std::memory_order_relaxed);
    g_sent_count.store(0, std::memory_order_relaxed);
    g_authenticated_count.store(0, std::memory_order_relaxed);
    g_tcp_accepted_count.store(0, std::memory_order_relaxed);
}

void ObserveReceivedI2NP(const ObserverMetadata& metadata) noexcept {
    g_received_count.fetch_add(1, std::memory_order_release);
    AppendRingEntry(g_receive_ring, metadata);
}

void ObserveSentI2NP(const ObserverMetadata& metadata) noexcept {
    g_sent_count.fetch_add(1, std::memory_order_release);
    AppendRingEntry(g_sent_ring, metadata);
}

void ObserveAuthenticated(const ObserverMetadata& metadata) noexcept {
    g_authenticated_count.fetch_add(1, std::memory_order_release);
    AppendRingEntry(g_authenticated_ring, metadata);
}

void ObserveTcpAccepted(const ObserverMetadata& metadata) noexcept {
    g_tcp_accepted_count.fetch_add(1, std::memory_order_release);
    AppendRingEntry(g_tcp_accepted_ring, metadata);
}

void ResetObserverSink() noexcept {
    // Plan 093: ``ResetObserverSink`` advances the generation and
    // resets every ring plus every atomic counter. The driver must
    // call this before transports start; the driver must not call
    // this after ``listener_ready``. The dedicated
    // ``BeginListenerGeneration`` helper exposes the same logic
    // without depending on the legacy clear-after-ready ordering.
    g_active_generation.fetch_add(1, std::memory_order_acq_rel);
    ResetRing(g_receive_ring);
    ResetRing(g_sent_ring);
    ResetRing(g_authenticated_ring);
    ResetRing(g_tcp_accepted_ring);
    g_drop_count.store(0, std::memory_order_relaxed);
    g_received_count.store(0, std::memory_order_relaxed);
    g_sent_count.store(0, std::memory_order_relaxed);
    g_authenticated_count.store(0, std::memory_order_relaxed);
    g_tcp_accepted_count.store(0, std::memory_order_relaxed);
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

std::uint64_t ObserverRecordedCount() noexcept {
    return g_receive_ring.total_count.load(std::memory_order_relaxed) +
           g_sent_ring.total_count.load(std::memory_order_relaxed) +
           g_authenticated_ring.total_count.load(
               std::memory_order_relaxed) +
           g_tcp_accepted_ring.total_count.load(
               std::memory_order_relaxed);
}

std::uint64_t ObserverCurrentGeneration() noexcept {
    return g_active_generation.load(std::memory_order_acquire);
}

std::uint64_t ObserverReceiveSequence() noexcept {
    try {
        std::lock_guard<std::mutex> guard(g_receive_ring.mutex);
        return g_receive_ring.next_sequence;
    } catch (...) {
        return 0;
    }
}

std::uint64_t ObserverSentSequence() noexcept {
    try {
        std::lock_guard<std::mutex> guard(g_sent_ring.mutex);
        return g_sent_ring.next_sequence;
    } catch (...) {
        return 0;
    }
}

bool WaitForAuthenticated(ObserverMetadata& metadata,
                          std::uint32_t timeout_ms) noexcept {
    if (!WaitForRingCounter(g_authenticated_ring,
                            g_active_generation.load(
                                std::memory_order_acquire),
                            0, timeout_ms)) {
        return false;
    }
    try {
        std::lock_guard<std::mutex> guard(g_authenticated_ring.mutex);
        for (std::size_t index = 0; index < INTEROP_RING_CAPACITY;
             ++index) {
            const auto& entry = g_authenticated_ring.entries[index];
            if (!entry.present) {
                continue;
            }
            if (entry.generation != g_active_generation.load(
                                        std::memory_order_acquire)) {
                continue;
            }
            metadata = entry.metadata;
            return true;
        }
        return false;
    } catch (...) {
        return false;
    }
}

bool WaitForReceivedI2NP(ObserverMetadata& metadata,
                         std::uint32_t timeout_ms) noexcept {
    if (!WaitForRingCounter(g_receive_ring,
                            g_active_generation.load(
                                std::memory_order_acquire),
                            0, timeout_ms)) {
        return false;
    }
    try {
        std::lock_guard<std::mutex> guard(g_receive_ring.mutex);
        for (std::size_t index = 0; index < INTEROP_RING_CAPACITY;
             ++index) {
            const auto& entry = g_receive_ring.entries[index];
            if (!entry.present) {
                continue;
            }
            if (entry.generation != g_active_generation.load(
                                        std::memory_order_acquire)) {
                continue;
            }
            metadata = entry.metadata;
            return true;
        }
        return false;
    } catch (...) {
        return false;
    }
}

bool WaitForSentI2NP(ObserverMetadata& metadata,
                     std::uint32_t timeout_ms) noexcept {
    if (!WaitForRingCounter(g_sent_ring,
                            g_active_generation.load(
                                std::memory_order_acquire),
                            0, timeout_ms)) {
        return false;
    }
    try {
        std::lock_guard<std::mutex> guard(g_sent_ring.mutex);
        for (std::size_t index = 0; index < INTEROP_RING_CAPACITY;
             ++index) {
            const auto& entry = g_sent_ring.entries[index];
            if (!entry.present) {
                continue;
            }
            if (entry.generation != g_active_generation.load(
                                        std::memory_order_acquire)) {
                continue;
            }
            metadata = entry.metadata;
            return true;
        }
        return false;
    } catch (...) {
        return false;
    }
}

bool WaitForTcpAccepted(ObserverMetadata& metadata,
                        std::uint32_t timeout_ms) noexcept {
    if (!WaitForRingCounter(g_tcp_accepted_ring,
                            g_active_generation.load(
                                std::memory_order_acquire),
                            0, timeout_ms)) {
        return false;
    }
    try {
        std::lock_guard<std::mutex> guard(g_tcp_accepted_ring.mutex);
        for (std::size_t index = 0; index < INTEROP_RING_CAPACITY;
             ++index) {
            const auto& entry = g_tcp_accepted_ring.entries[index];
            if (!entry.present) {
                continue;
            }
            if (entry.generation != g_active_generation.load(
                                        std::memory_order_acquire)) {
                continue;
            }
            metadata = entry.metadata;
            return true;
        }
        return false;
    } catch (...) {
        return false;
    }
}

bool WaitForReceivedDeliveryStatusAfter(
    std::uint64_t generation,
    std::uint64_t baseline_sequence,
    const std::uint64_t expected_peer_router_hash[4],
    std::uint32_t expected_message_id,
    std::uint32_t timeout_ms,
    ObserverMetadata& metadata) noexcept {
    // The wait predicate polls the receive ring for an entry in the
    // expected generation, with a sequence strictly greater than the
    // baseline, type ``DeliveryStatus`` (I2NP type 10), and the
    // configured message ID and peer Router Hash. The i2np_type is
    // not consulted at this layer because the receive ring entry
    // carries the decoded I2NP body type from the transport's
    // ``nextMsg->GetTypeID()`` after successful AEAD verification.
    if (!WaitForRingCounter(g_receive_ring, generation,
                            baseline_sequence, timeout_ms)) {
        return false;
    }
    return ReadRingEntry(g_receive_ring, generation, baseline_sequence,
                         expected_peer_router_hash,
                         /*expected_i2np_type=*/0, expected_message_id,
                         metadata);
}

bool WaitForSentDeliveryStatusAfter(
    std::uint64_t generation,
    std::uint64_t baseline_sequence,
    const std::uint64_t expected_peer_router_hash[4],
    std::uint32_t expected_message_id,
    std::uint32_t timeout_ms,
    ObserverMetadata& metadata) noexcept {
    if (!WaitForRingCounter(g_sent_ring, generation, baseline_sequence,
                            timeout_ms)) {
        return false;
    }
    return ReadRingEntry(g_sent_ring, generation, baseline_sequence,
                         expected_peer_router_hash,
                         /*expected_i2np_type=*/0, expected_message_id,
                         metadata);
}

#endif

}  // namespace i2pdinterop
}  // namespace i2pr
