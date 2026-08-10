// Plan 064/083/093 i2pd passive observer seam.
//
// The observer is the single owner of the receive-side and send-side
// observation surface for the Plan 064 i2pd direct NTCP2 driver. The
// observer API is `noexcept`, never blocks the transport thread on
// unbounded I/O, writes only to an owned bounded sink, and drops the
// observation with a typed local counter if the sink is unavailable.
//
// Every observer call site is compile-time gated by the
// ``I2PD_INTEROP_OBSERVER`` macro. The uninstrumented control build
// defines neither the macro nor the observer call sites; the observer
// API becomes an empty inline function that returns ``void``. The
// instrumented build defines the macro and the call sites become
// active observability hooks.
//
// Plan 093: the observer sink is a generation-bound bounded sequence
// ring instead of the previous cumulative last-slot. Each emitted
// observation carries a process-generation value (one per listener
// invocation) and an observation_sequence value. Wait primitives
// require exact matches on the ring entries by type, peer Router Hash,
// and message ID, and they require the observation_sequence to be
// strictly greater than a caller-supplied baseline. Stale-generation
// observations cannot satisfy a wait.
//
// The observer exposes only allowlisted metadata. It never exposes raw
// payload bytes, private keys, Noise state, frame keys, IV state, or
// transcripts.

#ifndef I2PR_I2PD_INTEROP_OBSERVER_H
#define I2PR_I2PD_INTEROP_OBSERVER_H

#include <cstdint>
#include <cstddef>

namespace i2pr {
namespace i2pdinterop {

// Bounded allowlisted metadata. Mirrors the Plan 064 observer API.
struct ObserverMetadata {
    std::uint64_t peer_router_hash_sha256[4]; // 32 bytes (4 x u64)
    char transport[8];                         // "ntcp2\0\0\0"
    char direction[16];                        // "i2pr-to-i2pd-ipv4" or "i2pd-to-i2pr-ipv4"
    std::uint64_t frame_sequence;
    std::uint32_t i2np_type;
    std::uint32_t i2np_envelope_message_id;
    std::uint32_t delivery_status_message_id;
    std::uint64_t bytes_transferred;
    std::uint64_t monotonic_ms;
};

// Plan 093: generation-and-sequence ring entry. The ring stores
// observations tagged with the listener invocation's generation and
// a strictly monotonic observation_sequence. Stale-generation or
// pre-baseline entries are rejected by the predicate waits below.
struct ObservationRingEntry {
    std::uint64_t generation;
    std::uint64_t observation_sequence;
    ObserverMetadata metadata;
    bool present;
};

// Plan 093: the bounded receive/send/authenticated/tcp-accepted
// ring capacity. A passing run must observe only ``capacity`` ring
// entries per category; overflow increments ``ObserverDropCount()``
// and fails the gate. The constant and the ``ObservationRing``
// storage type are reachable from the implementation file in both
// the instrumented and the uninstrumented builds so the file-scope
// ring instances and the empty inline observer API in the control
// build compile cleanly. The active API surface remains gated by
// ``I2PD_INTEROP_OBSERVER`` below.
constexpr std::size_t INTEROP_RING_CAPACITY = 64;

#ifdef I2PD_INTEROP_OBSERVER

// Plan 093: begin a new listener generation. The driver calls this
// before starting transports; the observer resets its counters and
// the tcp-accepted ring to a new generation. The generation is
// recorded on every subsequent ring entry; wait primitives reject
// metadata from a different generation.
void BeginListenerGeneration() noexcept;

// Compile-time gated observer call sites. The instrumented build emits
// structured metadata to the owned bounded sink. The uninstrumented
// build resolves to an empty inline that the compiler is allowed to
// elide.
void ObserveReceivedI2NP(const ObserverMetadata& metadata) noexcept;
void ObserveSentI2NP(const ObserverMetadata& metadata) noexcept;
void ObserveAuthenticated(const ObserverMetadata& metadata) noexcept;
// Plan 091: TCP-stage observer seam. The observer records a
// sanitized marker that the i2pd transport thread emits after the
// real pinned NTCP2 accept succeeds and before any Noise handshake
// byte is processed.
void ObserveTcpAccepted(const ObserverMetadata& metadata) noexcept;

// Sink management. The driver owns the sink; the observer never blocks.
void ResetObserverSink() noexcept;
std::uint64_t ObserverDropCount() noexcept;
std::uint64_t ObserverObservationCount() noexcept;
// Plan 093: total entries recorded since the last reset across all
// categories. Used by the bounded unit tests to verify the ring
// saw only the expected events.
std::uint64_t ObserverRecordedCount() noexcept;
// Plan 093: returns the current listener generation.
std::uint64_t ObserverCurrentGeneration() noexcept;

// Plan 093: the receive ring returns the current receive count for
// the active generation. Predicates require ``observation_sequence``
// strictly greater than this value.
std::uint64_t ObserverReceiveSequence() noexcept;
std::uint64_t ObserverSentSequence() noexcept;

// Bounded wait primitives used by the Plan 083 minimal probe driver.
// The wait primitives spin on a short sleep, never block the transport
// thread, and return ``false`` on timeout. They only ever return
// metadata captured from the observer sink (no fresh data is created).
bool WaitForAuthenticated(ObserverMetadata& metadata,
                          std::uint32_t timeout_ms) noexcept;
bool WaitForReceivedI2NP(ObserverMetadata& metadata,
                         std::uint32_t timeout_ms) noexcept;
bool WaitForSentI2NP(ObserverMetadata& metadata,
                     std::uint32_t timeout_ms) noexcept;
// Plan 091: TCP-stage wait primitive. Polls the observer sink for
// the `tcp_accepted` counter and returns the metadata captured by
// the patched `NTCP2Server::HandleAccept` path.
bool WaitForTcpAccepted(ObserverMetadata& metadata,
                        std::uint32_t timeout_ms) noexcept;

// Plan 093: target predicate waits. The waits block until a ring
// entry in the active generation has type ``DeliveryStatus``, the
// configured peer Router Hash, the configured message ID, and an
// ``observation_sequence`` strictly greater than the supplied
// baseline. The receive variant scans the receive ring; the send
// variant scans the send ring. Returns ``false`` on timeout.
//
// The waits never accept stale-generation entries, generic-phrase
// entries, wrong-router-hash entries, wrong-message-id entries, or
// pre-baseline entries. The wait boundary exposes the resolved ring
// entry via the supplied metadata out parameter.
bool WaitForReceivedDeliveryStatusAfter(
    std::uint64_t generation,
    std::uint64_t baseline_sequence,
    const std::uint64_t expected_peer_router_hash[4],
    std::uint32_t expected_message_id,
    std::uint32_t timeout_ms,
    ObserverMetadata& metadata) noexcept;

bool WaitForSentDeliveryStatusAfter(
    std::uint64_t generation,
    std::uint64_t baseline_sequence,
    const std::uint64_t expected_peer_router_hash[4],
    std::uint32_t expected_message_id,
    std::uint32_t timeout_ms,
    ObserverMetadata& metadata) noexcept;

#else

// Plan 093: under the uninstrumented control build the observer is a
// no-op. The wait primitives never resolve. The plan forbids using
// the control binary for any measure that depends on the observer;
// the only data plane that matters in the control run is the
// external DeliveryStatus reception.
inline void BeginListenerGeneration() noexcept {}

inline void ObserveReceivedI2NP(const ObserverMetadata& /*metadata*/) noexcept {}
inline void ObserveSentI2NP(const ObserverMetadata& /*metadata*/) noexcept {}
inline void ObserveAuthenticated(const ObserverMetadata& /*metadata*/) noexcept {}
inline void ObserveTcpAccepted(const ObserverMetadata& /*metadata*/) noexcept {}

inline void ResetObserverSink() noexcept {}
inline std::uint64_t ObserverDropCount() noexcept { return 0; }
inline std::uint64_t ObserverObservationCount() noexcept { return 0; }
inline std::uint64_t ObserverRecordedCount() noexcept { return 0; }
inline std::uint64_t ObserverCurrentGeneration() noexcept { return 0; }
inline std::uint64_t ObserverReceiveSequence() noexcept { return 0; }
inline std::uint64_t ObserverSentSequence() noexcept { return 0; }

inline bool WaitForAuthenticated(ObserverMetadata& /*metadata*/,
                                 std::uint32_t /*timeout_ms*/) noexcept {
    return false;
}
inline bool WaitForReceivedI2NP(ObserverMetadata& /*metadata*/,
                                std::uint32_t /*timeout_ms*/) noexcept {
    return false;
}
inline bool WaitForSentI2NP(ObserverMetadata& /*metadata*/,
                            std::uint32_t /*timeout_ms*/) noexcept {
    return false;
}
inline bool WaitForTcpAccepted(ObserverMetadata& /*metadata*/,
                               std::uint32_t /*timeout_ms*/) noexcept {
    return false;
}

inline bool WaitForReceivedDeliveryStatusAfter(
    std::uint64_t /*generation*/, std::uint64_t /*baseline_sequence*/,
    const std::uint64_t /*expected_peer_router_hash*/[4],
    std::uint32_t /*expected_message_id*/, std::uint32_t /*timeout_ms*/,
    ObserverMetadata& /*metadata*/) noexcept {
    return false;
}

inline bool WaitForSentDeliveryStatusAfter(
    std::uint64_t /*generation*/, std::uint64_t /*baseline_sequence*/,
    const std::uint64_t /*expected_peer_router_hash*/[4],
    std::uint32_t /*expected_message_id*/, std::uint32_t /*timeout_ms*/,
    ObserverMetadata& /*metadata*/) noexcept {
    return false;
}

#endif

}  // namespace i2pdinterop
}  // namespace i2pr

#endif  // I2PR_I2PD_INTEROP_OBSERVER_H
