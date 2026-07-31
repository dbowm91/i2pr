// Plan 064 i2pd passive observer seam.
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

#ifdef I2PD_INTEROP_OBSERVER

// Compile-time gated observer call sites. The instrumented build emits
// structured metadata to the owned bounded sink. The uninstrumented
// build resolves to an empty inline that the compiler is allowed to
// elide.
void ObserveReceivedI2NP(const ObserverMetadata& metadata) noexcept;
void ObserveSentI2NP(const ObserverMetadata& metadata) noexcept;

// Sink management. The driver owns the sink; the observer never blocks.
void ResetObserverSink() noexcept;
std::uint64_t ObserverDropCount() noexcept;
std::uint64_t ObserverObservationCount() noexcept;

#else

inline void ObserveReceivedI2NP(const ObserverMetadata& /*metadata*/) noexcept {}
inline void ObserveSentI2NP(const ObserverMetadata& /*metadata*/) noexcept {}
inline void ResetObserverSink() noexcept {}
inline std::uint64_t ObserverDropCount() noexcept { return 0; }
inline std::uint64_t ObserverObservationCount() noexcept { return 0; }

#endif

}  // namespace i2pdinterop
}  // namespace i2pr

#endif  // I2PR_I2PD_INTEROP_OBSERVER_H