// Plan 076 i2pd direct NTCP2 driver.
//
// The driver is the test-only, source-locked i2pd 2.60.0 NTCP2
// reference helper. It links against the unmodified pinned i2pd
// libraries built from
// `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`,
// initializes the full pinned i2pd context in the source-verified
// order from the Plan 062 / Plan 064 source-verification record, and
// uses the real NTCP2 transport. Inspect mode produces a real
// signed RouterInfo, listen mode binds a real NTCP2 listener on
// the configured endpoint, and dial mode submits one real
// DeliveryStatus I2NP message through
// `i2p::transport::Transports::SendMessage`.
//
// Build artefacts:
//
// * instrumented binary: built with
//   `-DI2PD_INTEROP_OBSERVER=1` and the observer patch applied to
//   the pinned `libi2pd/NTCP2.cpp`;
// * control binary: built without the macro and without the observer
//   patch so no observer call site is reachable.
//
// Behavioural constraints (Plan 064, corrected by Plan 076):
//
//   * the driver links against the unmodified pinned i2pd libraries;
//     Plan 046 host may not have those libraries available, in which
//     case `I2PD_PLAN076_LINKED` is not defined and the driver fails
//     closed with exit 66;
//   * no cryptography, handshake, frame-encoding, or transport
//     patches — the observer patch observes only after AEAD
//     verification, block bounds validation, and FromNTCP2
//     conversion;
//   * exactly one outbound dial or inbound listener per invocation
//     (one-shot contract);
//   * bounded monotonic timeout — no retries, no wall-clock sleeps,
//     no DNS, no public network egress, no SAM/I2CP/HTTP, no reseed,
//     no floodfill, no support router;
//   * typed outcomes only — every rejected or blocked outcome exits
//     non-zero and writes the typed event to the disposable reference
//     event stream;
//   * shutdown is mandatory — the driver stops the i2pd singletons
//     in strict reverse ownership order so no helper-owned state
//     survives the process boundary;
//   * the passive observer is compile-time gated; the uninstrumented
//     control build omits every observer call site.

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <optional>
#include <sstream>
#include <string>
#include <string_view>
#include <vector>

#include <openssl/sha.h>

#include "interop_observer.h"

// i2pd library headers — the driver is linked against the unmodified
// pinned i2pd 2.60.0 libraries. The constants below are governed by
// the build script and verifier, not by include guards.
#include "Config.h"
#include "Crypto.h"
#include "FS.h"
#include "I2NPProtocol.h"
#include "Identity.h"
#include "Log.h"
#include "NTCP2.h"
#include "NetDb.hpp"
#include "RouterContext.h"
#include "RouterInfo.h"
#include "Transports.h"
#include "version.h"

namespace {

constexpr std::string_view kEventSchema = "i2pr-reference-event-v1";
constexpr std::int32_t kEventSchemaVersion = 1;

constexpr std::string_view kReference = "i2pd";
constexpr std::string_view kReferenceVersion = "2.60.0";
constexpr std::string_view kReferenceRevision =
    "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e";

constexpr std::string_view kImplName = "i2pd-direct-driver";
constexpr std::string_view kImplRevision =
    "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e";

constexpr std::size_t kSha256HexLength = 64;

constexpr const char* kConfigSchema = "i2pr-i2pd-direct-driver-config-v1";
constexpr std::int32_t kConfigSchemaVersion = 1;

std::string hex_lower(const std::uint8_t* data, std::size_t length) {
    static constexpr char kAlphabet[] = "0123456789abcdef";
    std::string out;
    out.resize(length * 2);
    for (std::size_t i = 0; i < length; ++i) {
        out[2 * i] = kAlphabet[data[i] >> 4];
        out[2 * i + 1] = kAlphabet[data[i] & 0x0F];
    }
    return out;
}

std::string sha256_hex(const std::vector<std::uint8_t>& bytes) {
    std::uint8_t digest[32];
    SHA256(bytes.data(), bytes.size(), digest);
    return hex_lower(digest, sizeof(digest));
}

std::string sha256_hex(const std::string& text) {
    return sha256_hex(std::vector<std::uint8_t>(text.begin(), text.end()));
}

std::uint64_t monotonic_millis() {
    return static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now().time_since_epoch())
            .count());
}

bool looks_like_hex64(std::string_view text) {
    if (text.size() != kSha256HexLength) {
        return false;
    }
    for (char c : text) {
        const bool ok = (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f');
        if (!ok) {
            return false;
        }
    }
    return true;
}

std::optional<std::string> read_file(const std::filesystem::path& path,
                                     std::size_t max_bytes) {
    std::ifstream handle(path, std::ios::binary);
    if (!handle) {
        return std::nullopt;
    }
    std::ostringstream buffer;
    buffer << handle.rdbuf();
    auto text = buffer.str();
    if (text.size() > max_bytes) {
        return std::nullopt;
    }
    return text;
}

std::string json_escape(std::string_view text) {
    std::string out;
    out.reserve(text.size() + 2);
    for (char c : text) {
        switch (c) {
            case '"':
                out += "\\\"";
                break;
            case '\\':
                out += "\\\\";
                break;
            case '\n':
                out += "\\n";
                break;
            case '\r':
                out += "\\r";
                break;
            case '\t':
                out += "\\t";
                break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buffer[8];
                    std::snprintf(buffer, sizeof(buffer), "\\u%04x",
                                  static_cast<unsigned char>(c));
                    out += buffer;
                } else {
                    out.push_back(c);
                }
        }
    }
    return out;
}

struct DriverConfig {
    std::string run_id;
    std::string scenario_id;
    std::string direction;
    std::string mode;
    std::filesystem::path data_dir;
    std::filesystem::path output_dir;
    std::string local_address;
    std::uint16_t local_port{0};
    std::int32_t network_id{0};
    std::filesystem::path peer_router_info_path;
    std::string expected_local_router_hash_sha256;
    std::string expected_peer_router_hash_sha256;
    std::string expected_peer_address;
    std::uint16_t expected_peer_port{0};
    std::uint32_t delivery_status_message_id{0};
    std::uint32_t startup_timeout_ms{0};
    std::uint32_t handshake_timeout_ms{0};
    std::uint32_t data_phase_timeout_ms{0};
    std::uint32_t shutdown_timeout_ms{0};
    std::string reference_revision;
    std::string reference_tree_sha256;
    std::string driver_source_sha256;
    std::string driver_binary_sha256;
    std::string build_manifest_sha256;
    std::string observer_patch_sha256;
    std::string run_identity_sha256;
    std::string topology_kind;
};

void parse_strict_config(const std::filesystem::path& path,
                         DriverConfig& out,
                         std::vector<std::string>& extra_fields,
                         std::vector<std::string>& missing_fields) {
    auto text = read_file(path, 1u << 16);
    if (!text) {
        throw std::runtime_error("config-not-readable");
    }
    const auto& raw = *text;
    std::size_t idx = 0;
    if (raw.empty() || raw[idx] != '{') {
        throw std::runtime_error("config-not-object");
    }
    ++idx;
    bool first = true;
    while (true) {
        while (idx < raw.size() &&
               (raw[idx] == ' ' || raw[idx] == '\t' || raw[idx] == '\n' ||
                raw[idx] == '\r')) {
            ++idx;
        }
        if (idx >= raw.size()) {
            throw std::runtime_error("config-truncated");
        }
        if (raw[idx] == '}') {
            break;
        }
        if (!first) {
            if (raw[idx] != ',') {
                throw std::runtime_error("config-missing-comma");
            }
            ++idx;
            while (idx < raw.size() &&
                   (raw[idx] == ' ' || raw[idx] == '\t' || raw[idx] == '\n' ||
                    raw[idx] == '\r')) {
                ++idx;
            }
        }
        first = false;
        if (idx >= raw.size() || raw[idx] != '"') {
            throw std::runtime_error("config-key-not-string");
        }
        std::size_t key_start = ++idx;
        while (idx < raw.size() && raw[idx] != '"') {
            if (raw[idx] == '\\') {
                ++idx;
                if (idx >= raw.size()) {
                    throw std::runtime_error("config-truncated-escape");
                }
            }
            ++idx;
        }
        if (idx >= raw.size()) {
            throw std::runtime_error("config-unterminated-key");
        }
        std::string key = raw.substr(key_start, idx - key_start);
        ++idx;
        while (idx < raw.size() &&
               (raw[idx] == ' ' || raw[idx] == '\t' || raw[idx] == '\n' ||
                raw[idx] == '\r')) {
            ++idx;
        }
        if (idx >= raw.size() || raw[idx] != ':') {
            throw std::runtime_error("config-missing-colon");
        }
        ++idx;
        while (idx < raw.size() &&
               (raw[idx] == ' ' || raw[idx] == '\t' || raw[idx] == '\n' ||
                raw[idx] == '\r')) {
            ++idx;
        }
        std::string string_value;
        long long integer_value = 0;
        bool is_integer = false;
        if (idx < raw.size() && raw[idx] == '"') {
            ++idx;
            std::size_t value_start = idx;
            while (idx < raw.size() && raw[idx] != '"') {
                if (raw[idx] == '\\') {
                    ++idx;
                    if (idx >= raw.size()) {
                        throw std::runtime_error("config-truncated-escape");
                    }
                }
                ++idx;
            }
            if (idx >= raw.size()) {
                throw std::runtime_error("config-unterminated-string");
            }
            string_value = raw.substr(value_start, idx - value_start);
            ++idx;
        } else {
            std::size_t start = idx;
            if (idx < raw.size() && raw[idx] == '-') {
                ++idx;
            }
            while (idx < raw.size() && raw[idx] >= '0' && raw[idx] <= '9') {
                ++idx;
            }
            if (idx == start) {
                throw std::runtime_error("config-value-not-primitive");
            }
            integer_value = std::stoll(raw.substr(start, idx - start));
            is_integer = true;
        }
        if (key == "schema") {
            if (string_value != kConfigSchema) {
                throw std::runtime_error("config-schema-mismatch");
            }
        } else if (key == "schema_version") {
            if (!is_integer || integer_value != kConfigSchemaVersion) {
                throw std::runtime_error("config-schema-version-mismatch");
            }
        } else if (key == "run_id") {
            out.run_id = string_value;
        } else if (key == "scenario_id") {
            out.scenario_id = string_value;
        } else if (key == "direction") {
            out.direction = string_value;
        } else if (key == "mode") {
            out.mode = string_value;
        } else if (key == "data_dir") {
            out.data_dir = string_value;
        } else if (key == "output_dir") {
            out.output_dir = string_value;
        } else if (key == "local_address") {
            out.local_address = string_value;
        } else if (key == "local_port") {
            if (!is_integer) {
                throw std::runtime_error("config-local-port-not-integer");
            }
            out.local_port = static_cast<std::uint16_t>(integer_value);
        } else if (key == "network_id") {
            if (!is_integer) {
                throw std::runtime_error("config-network-id-not-integer");
            }
            out.network_id = static_cast<std::int32_t>(integer_value);
        } else if (key == "peer_router_info_path") {
            out.peer_router_info_path = string_value;
        } else if (key == "expected_local_router_hash_sha256") {
            out.expected_local_router_hash_sha256 = string_value;
        } else if (key == "expected_peer_router_hash_sha256") {
            out.expected_peer_router_hash_sha256 = string_value;
        } else if (key == "expected_peer_address") {
            out.expected_peer_address = string_value;
        } else if (key == "expected_peer_port") {
            if (!is_integer) {
                throw std::runtime_error("config-peer-port-not-integer");
            }
            out.expected_peer_port = static_cast<std::uint16_t>(integer_value);
        } else if (key == "delivery_status_message_id") {
            if (!is_integer) {
                throw std::runtime_error("config-message-id-not-integer");
            }
            out.delivery_status_message_id =
                static_cast<std::uint32_t>(integer_value);
        } else if (key == "startup_timeout_ms") {
            out.startup_timeout_ms = static_cast<std::uint32_t>(integer_value);
        } else if (key == "handshake_timeout_ms") {
            out.handshake_timeout_ms =
                static_cast<std::uint32_t>(integer_value);
        } else if (key == "data_phase_timeout_ms") {
            out.data_phase_timeout_ms =
                static_cast<std::uint32_t>(integer_value);
        } else if (key == "shutdown_timeout_ms") {
            out.shutdown_timeout_ms =
                static_cast<std::uint32_t>(integer_value);
        } else if (key == "reference_revision") {
            out.reference_revision = string_value;
        } else if (key == "reference_tree_sha256") {
            out.reference_tree_sha256 = string_value;
        } else if (key == "driver_source_sha256") {
            out.driver_source_sha256 = string_value;
        } else if (key == "driver_binary_sha256") {
            out.driver_binary_sha256 = string_value;
        } else if (key == "build_manifest_sha256") {
            out.build_manifest_sha256 = string_value;
        } else if (key == "observer_patch_sha256") {
            out.observer_patch_sha256 = string_value;
        } else if (key == "run_identity_sha256") {
            out.run_identity_sha256 = string_value;
        } else if (key == "topology_kind") {
            out.topology_kind = string_value;
        } else {
            extra_fields.push_back(key);
        }
    }
    static const char* const kRequired[] = {
        "schema",            "schema_version",
        "run_id",            "scenario_id",
        "direction",         "mode",
        "data_dir",          "output_dir",
        "local_address",     "local_port",
        "network_id",        "peer_router_info_path",
        "expected_local_router_hash_sha256",
        "expected_peer_router_hash_sha256",
        "expected_peer_address", "expected_peer_port",
        "delivery_status_message_id",
        "startup_timeout_ms", "handshake_timeout_ms",
        "data_phase_timeout_ms", "shutdown_timeout_ms",
        "reference_revision", "reference_tree_sha256",
        "driver_source_sha256", "driver_binary_sha256",
        "build_manifest_sha256", "observer_patch_sha256",
        "run_identity_sha256", "topology_kind",
    };
    auto contains = [](const DriverConfig& cfg, const char* name) -> bool {
        if (std::strcmp(name, "schema") == 0) return true;
        if (std::strcmp(name, "schema_version") == 0) return true;
        if (std::strcmp(name, "run_id") == 0) return !cfg.run_id.empty();
        if (std::strcmp(name, "scenario_id") == 0) return !cfg.scenario_id.empty();
        if (std::strcmp(name, "direction") == 0) return !cfg.direction.empty();
        if (std::strcmp(name, "mode") == 0) return !cfg.mode.empty();
        if (std::strcmp(name, "data_dir") == 0) return !cfg.data_dir.empty();
        if (std::strcmp(name, "output_dir") == 0) return !cfg.output_dir.empty();
        if (std::strcmp(name, "local_address") == 0) return !cfg.local_address.empty();
        if (std::strcmp(name, "local_port") == 0) return cfg.local_port != 0;
        if (std::strcmp(name, "network_id") == 0) return cfg.network_id != 0;
        if (std::strcmp(name, "peer_router_info_path") == 0) return !cfg.peer_router_info_path.empty();
        if (std::strcmp(name, "expected_local_router_hash_sha256") == 0) return !cfg.expected_local_router_hash_sha256.empty();
        if (std::strcmp(name, "expected_peer_router_hash_sha256") == 0) return !cfg.expected_peer_router_hash_sha256.empty();
        if (std::strcmp(name, "expected_peer_address") == 0) return !cfg.expected_peer_address.empty();
        if (std::strcmp(name, "expected_peer_port") == 0) return cfg.expected_peer_port != 0;
        if (std::strcmp(name, "delivery_status_message_id") == 0) return cfg.delivery_status_message_id != 0;
        if (std::strcmp(name, "startup_timeout_ms") == 0) return cfg.startup_timeout_ms != 0;
        if (std::strcmp(name, "handshake_timeout_ms") == 0) return cfg.handshake_timeout_ms != 0;
        if (std::strcmp(name, "data_phase_timeout_ms") == 0) return cfg.data_phase_timeout_ms != 0;
        if (std::strcmp(name, "shutdown_timeout_ms") == 0) return cfg.shutdown_timeout_ms != 0;
        if (std::strcmp(name, "reference_revision") == 0) return !cfg.reference_revision.empty();
        if (std::strcmp(name, "reference_tree_sha256") == 0) return !cfg.reference_tree_sha256.empty();
        if (std::strcmp(name, "driver_source_sha256") == 0) return !cfg.driver_source_sha256.empty();
        if (std::strcmp(name, "driver_binary_sha256") == 0) return !cfg.driver_binary_sha256.empty();
        if (std::strcmp(name, "build_manifest_sha256") == 0) return !cfg.build_manifest_sha256.empty();
        if (std::strcmp(name, "observer_patch_sha256") == 0) return !cfg.observer_patch_sha256.empty();
        if (std::strcmp(name, "run_identity_sha256") == 0) return !cfg.run_identity_sha256.empty();
        if (std::strcmp(name, "topology_kind") == 0) return !cfg.topology_kind.empty();
        return false;
    };
    for (const auto* field : kRequired) {
        if (!contains(out, field)) {
            missing_fields.emplace_back(field);
        }
    }
}

void validate_config(const DriverConfig& cfg) {
    static const std::vector<std::string> kAllowedModes = {
        "listen", "dial", "inspect",
    };
    static const std::vector<std::string> kAllowedDirections = {
        "i2pr-to-java-ipv4",
        "java-to-i2pr-ipv4",
        "i2pr-to-i2pd-ipv4",
        "i2pd-to-i2pr-ipv4",
    };
    static const std::vector<std::string> kSyntheticTargets = {
        "192.0.2.1", "192.0.2.2",
    };
    static const std::vector<std::string> kLoopbackTargets = {
        "127.0.0.1",
    };
    static const std::string kLoopbackTopologyKind = "host-loopback-development";

    const bool is_loopback_topology = (cfg.topology_kind == kLoopbackTopologyKind);
    if (!is_loopback_topology &&
        std::find(kSyntheticTargets.begin(), kSyntheticTargets.end(),
                  cfg.local_address) == kSyntheticTargets.end()) {
        throw std::runtime_error("config-local-address-not-synthetic");
    }
    if (is_loopback_topology &&
        std::find(kLoopbackTargets.begin(), kLoopbackTargets.end(),
                  cfg.local_address) == kLoopbackTargets.end()) {
        throw std::runtime_error("config-local-address-not-loopback");
    }

    if (cfg.reference_revision != kReferenceRevision) {
        throw std::runtime_error("config-reference-revision-mismatch");
    }
    if (cfg.mode == "listen" || cfg.mode == "dial") {
        if (std::find(kAllowedDirections.begin(), kAllowedDirections.end(),
                      cfg.direction) == kAllowedDirections.end()) {
            throw std::runtime_error("config-direction-not-allowlisted");
        }
    }
    if (std::find(kAllowedModes.begin(), kAllowedModes.end(), cfg.mode) ==
        kAllowedModes.end()) {
        throw std::runtime_error("config-mode-not-allowlisted");
    }
    if (cfg.local_port == 0) {
        throw std::runtime_error("config-local-port-out-of-range");
    }
    if (cfg.network_id != 99) {
        throw std::runtime_error("config-network-id-not-99");
    }
    if (!looks_like_hex64(cfg.expected_local_router_hash_sha256)) {
        throw std::runtime_error(
            "config-local-router-hash-not-64-hex");
    }
    if (!looks_like_hex64(cfg.expected_peer_router_hash_sha256)) {
        throw std::runtime_error(
            "config-peer-router-hash-not-64-hex");
    }
    if (cfg.expected_local_router_hash_sha256 ==
            std::string(kSha256HexLength, '0') ||
        cfg.expected_peer_router_hash_sha256 ==
            std::string(kSha256HexLength, '0')) {
        throw std::runtime_error("config-router-hash-zero-provenance");
    }
    for (const auto& field : {
             cfg.reference_tree_sha256, cfg.driver_source_sha256,
             cfg.driver_binary_sha256, cfg.build_manifest_sha256,
             cfg.observer_patch_sha256, cfg.run_identity_sha256}) {
        if (!looks_like_hex64(field)) {
            throw std::runtime_error("config-digest-not-64-hex");
        }
        if (field == std::string(kSha256HexLength, '0')) {
            throw std::runtime_error("config-zero-provenance-digest");
        }
    }
    if (cfg.delivery_status_message_id < 1 ||
        cfg.delivery_status_message_id > 0xFFFFFFFFu) {
        throw std::runtime_error(
            "config-delivery-status-message-id-out-of-range");
    }
    if (cfg.expected_peer_port == 0) {
        throw std::runtime_error("config-peer-port-out-of-range");
    }
    if (!is_loopback_topology &&
        std::find(kSyntheticTargets.begin(), kSyntheticTargets.end(),
                  cfg.expected_peer_address) == kSyntheticTargets.end()) {
        throw std::runtime_error("config-peer-address-not-synthetic");
    }
    if (is_loopback_topology &&
        std::find(kLoopbackTargets.begin(), kLoopbackTargets.end(),
                  cfg.expected_peer_address) == kLoopbackTargets.end()) {
        throw std::runtime_error("config-peer-address-not-loopback");
    }
    if (cfg.handshake_timeout_ms == 0 ||
        cfg.handshake_timeout_ms > 600000) {
        throw std::runtime_error("config-handshake-timeout-out-of-range");
    }
    if (cfg.shutdown_timeout_ms == 0 ||
        cfg.shutdown_timeout_ms > 60000) {
        throw std::runtime_error("config-shutdown-timeout-out-of-range");
    }
}

bool path_is_owned(const std::filesystem::path& path) {
    auto raw = path.string();
    if (raw.empty()) {
        return false;
    }
    if (raw.find("..") != std::string::npos) {
        return false;
    }
    if (raw.compare(0, 5, "/proc") == 0 || raw.compare(0, 4, "/sys") == 0 ||
        raw.compare(0, 4, "/dev") == 0) {
        return false;
    }
    return true;
}

struct EventRecord {
    std::string run_id;
    std::string scenario_id;
    std::string direction;
    std::string implementation;
    std::string implementation_revision;
    std::string driver_binary_sha256;
    std::string local_router_hash_sha256;
    std::string peer_router_hash_sha256;
    std::uint64_t monotonic_ms;
    std::string event_kind;
    std::int64_t event_sequence;
    std::optional<std::uint32_t> delivery_status_message_id;
    std::optional<std::uint32_t> i2np_type;
    std::optional<std::uint64_t> frame_sequence;
    std::optional<std::string> reason_code;
    std::optional<std::string> detail;
};

class EventWriter {
   public:
    explicit EventWriter(std::filesystem::path path)
        : path_(std::move(path)) {}

    void Emit(const EventRecord& record) {
        std::ostringstream payload;
        payload << "{";
        payload << "\"schema\":\"" << json_escape(kEventSchema) << "\",";
        payload << "\"schema_version\":" << kEventSchemaVersion << ",";
        payload << "\"run_id\":\"" << json_escape(record.run_id) << "\",";
        payload << "\"scenario_id\":\"" << json_escape(record.scenario_id)
                << "\",";
        payload << "\"direction\":\"" << json_escape(record.direction)
                << "\",";
        payload << "\"implementation\":\"" << json_escape(record.implementation)
                << "\",";
        payload << "\"implementation_revision\":\""
                << json_escape(record.implementation_revision) << "\",";
        payload << "\"driver_binary_sha256\":\""
                << json_escape(record.driver_binary_sha256) << "\",";
        payload << "\"local_router_hash_sha256\":\""
                << json_escape(record.local_router_hash_sha256) << "\",";
        payload << "\"peer_router_hash_sha256\":\""
                << json_escape(record.peer_router_hash_sha256) << "\",";
        payload << "\"monotonic_ms\":" << record.monotonic_ms << ",";
        payload << "\"event_kind\":\"" << json_escape(record.event_kind)
                << "\",";
        payload << "\"event_sequence\":" << record.event_sequence;
        if (record.delivery_status_message_id.has_value()) {
            payload << ",\"delivery_status_message_id\":"
                    << *record.delivery_status_message_id;
        }
        if (record.i2np_type.has_value()) {
            payload << ",\"i2np_type\":" << *record.i2np_type;
        }
        if (record.frame_sequence.has_value()) {
            payload << ",\"frame_sequence\":" << *record.frame_sequence;
        }
        if (record.reason_code.has_value()) {
            payload << ",\"reason_code\":\""
                    << json_escape(*record.reason_code) << "\"";
        }
        if (record.detail.has_value()) {
            payload << ",\"detail\":\""
                    << json_escape(*record.detail) << "\"";
        }
        // The event_sha256 placeholder is rendered as a sentinel
        // marker that the canonical event schema can never produce
        // naturally; we replace it with the digest after computing
        // the hash over the line that contains the marker.
        payload << ",\"event_sha256\":\"##EVENT_SHA256##\"}";
        auto line = payload.str();
        auto digest = sha256_hex(line);
        auto pos = line.find("##EVENT_SHA256##");
        if (pos == std::string::npos) {
            throw std::runtime_error("event-sha256-marker-missing");
        }
        line.replace(pos, std::strlen("##EVENT_SHA256##"), digest);
        std::filesystem::create_directories(path_.parent_path());
        std::ofstream handle(path_, std::ios::binary | std::ios::app);
        if (!handle) {
            throw std::runtime_error("event-write-failed");
        }
        handle << line << "\n";
        if (!handle) {
            throw std::runtime_error("event-write-failed");
        }
        next_sequence_ += 1;
    }

    std::int64_t next_sequence() const { return next_sequence_; }

   private:
    std::filesystem::path path_;
    std::int64_t next_sequence_{0};
};

void emit_event(EventWriter& writer, const DriverConfig& cfg,
                std::string event_kind,
                std::optional<std::uint32_t> message_id = std::nullopt,
                std::optional<std::uint32_t> i2np_type = std::nullopt,
                std::optional<std::uint64_t> frame_sequence = std::nullopt,
                std::optional<std::string> reason_code = std::nullopt,
                std::optional<std::string> detail = std::nullopt) {
    EventRecord record{};
    record.run_id = cfg.run_id;
    record.scenario_id = cfg.scenario_id;
    record.direction = cfg.direction;
    record.implementation = std::string(kImplName);
    record.implementation_revision = std::string(kImplRevision);
    record.driver_binary_sha256 = cfg.driver_binary_sha256;
    record.local_router_hash_sha256 = cfg.expected_local_router_hash_sha256;
    record.peer_router_hash_sha256 = cfg.expected_peer_router_hash_sha256;
    record.monotonic_ms = monotonic_millis();
    record.event_kind = std::move(event_kind);
    record.event_sequence = writer.next_sequence();
    record.delivery_status_message_id = message_id;
    record.i2np_type = i2np_type;
    record.frame_sequence = frame_sequence;
    record.reason_code = std::move(reason_code);
    record.detail = std::move(detail);
    writer.Emit(record);
}

bool pinned_libraries_linked() {
#ifdef I2PD_PLAN076_LINKED
    return true;
#else
    return false;
#endif
}

void set_bool_option(const char* name, bool value) {
    i2p::config::SetOption(name, value);
}

void set_int_option(const char* name, int value) {
    i2p::config::SetOption(name, value);
}

void set_uint16_option(const char* name, std::uint16_t value) {
    i2p::config::SetOption(name, value);
}

void set_int_typed_option(const char* name, int value) {
    i2p::config::SetOption(name, value);
}

void set_string_option(const char* name, const std::string& value) {
    i2p::config::SetOption(name, value);
}

struct OwnedRuntime {
    bool context_initialised{false};
    bool netdb_started{false};
    bool transports_started{false};
    bool context_started{false};
    bool router_info_exported{false};
    bool router_info_written{false};
    std::string local_ident_hash_hex;
    std::string router_info_path;
};

std::string write_local_router_info(const DriverConfig& cfg) {
    if (!path_is_owned(cfg.output_dir)) {
        return "output-dir-not-owned";
    }
    const auto& ri = i2p::context.GetRouterInfo();
    if (!ri.GetBuffer() || ri.GetBufferLen() == 0) {
        return "router-info-buffer-empty";
    }
    auto target = cfg.output_dir / "router.info";
    std::ofstream out(target, std::ios::binary | std::ios::trunc);
    if (!out) {
        return "router-info-write-failed";
    }
    out.write(reinterpret_cast<const char*>(ri.GetBuffer()),
              static_cast<std::streamsize>(ri.GetBufferLen()));
    out.close();
    if (!out.good()) {
        return "router-info-write-failed";
    }
    return target.string();
}

void shutdown_runtime(OwnedRuntime& rt, EventWriter* /*writer*/,
                     DriverConfig* /*cfg*/) {
    // Strict reverse ownership order. Each step is independently
    // idempotent. Any failure during stop is logged; the runtime
    // must be brought to a quiescent state before exit.
    try {
        if (rt.context_started) {
            i2p::context.Stop();
            rt.context_started = false;
        }
    } catch (...) {
    }
    try {
        if (rt.transports_started) {
            i2p::transport::transports.Stop();
            rt.transports_started = false;
        }
    } catch (...) {
    }
    try {
        if (rt.netdb_started) {
            i2p::data::netdb.Stop();
            rt.netdb_started = false;
        }
    } catch (...) {
    }
    try {
        i2p::crypto::TerminateCrypto();
    } catch (...) {
    }
}

bool initialise_i2pd_runtime(const DriverConfig& cfg, EventWriter& writer,
                             OwnedRuntime& rt, std::string& failure_reason) {
    failure_reason.clear();

    // Phase 0: process_started. Already emitted by the caller; nothing
    // to do here.

    try {
        // Step 1: i2p::config::Init()
        i2p::config::Init();
    } catch (const std::exception& exc) {
        failure_reason = std::string("config-init:") + exc.what();
        return false;
    }

    // Step 2: set the data directory explicitly. DetectDataDir sets
    // the global dataDir used by every `i2p::fs::DataDirPath(...)`
    // call, including the NetDb path used by `i2p::data::netdb`.
    try {
        i2p::fs::SetAppName("i2pr-interop-driver");
        i2p::fs::DetectDataDir(cfg.data_dir.string(), false);
    } catch (const std::exception& exc) {
        failure_reason = std::string("fs-detect:") + exc.what();
        return false;
    }

    // Step 2b: populate the i2pd option store before mutating it.
    // i2pd's `boost::program_options` map only materializes the
    // declared defaults into `m_Options` after `store()` runs; the
    // standalone i2pd binary calls `ParseCmdline` (which stores
    // defaults) and `ParseConfig` (which overrides with the file).
    // The driver has neither a command line nor a config file, so
    // we synthesize a one-argument `ParseCmdline` invocation with
    // `ignoreUnknown = true`. This stores every declared default
    // into `m_Options` so each subsequent `SetOption` update lands
    // in place. `Finalize()` then runs external notifications.
    try {
        char* fake_argv[] = {const_cast<char*>("i2pd-direct-driver")};
        i2p::config::ParseCmdline(1, fake_argv, /*ignoreUnknown=*/true);
        i2p::config::Finalize();
    } catch (const std::exception& exc) {
        failure_reason = std::string("config-populate:") + exc.what();
        return false;
    }

    // Step 3: render a minimal configuration suitable for the sealed
    // synthetic namespace. The configuration values are forced through
    // the i2pd configuration subsystem; we never write an i2pd.conf
    // file because the data directory is consumed by context.Init().
    try {
        set_bool_option("precomputation.elgamal", false);
        set_bool_option("ssu", false);
        set_bool_option("upnp.enabled", false);
        set_bool_option("nettime.enabled", false);
        set_bool_option("floodfill", false);
        set_bool_option("notransit", true);
        set_bool_option("nat", true);
        set_bool_option("ipv4", true);
        set_bool_option("ipv6", false);
        set_bool_option("ntcp2.enabled", true);
        // Plan 090 correction: publish the NTCP2 address so the
        // exported RouterInfo carries `host`, `port`, and `i`. With
        // `ntcp2.published = false` the i2pd `NewRouterInfo()` path
        // takes the non-published branch (RouterContext.cpp lines
        // 152-157) and the address is serialized without the fields
        // the i2pr `exact_ntcp2_address` parser requires. The option
        // is registered as `value<bool>()` in i2pd Config.cpp line
        // 330, so the driver must use `set_bool_option` (the
        // `set_int_option` overload would store an `int` and the
        // subsequent `GetOption<bool>` would throw `bad_any_cast`).
        set_bool_option("ntcp2.published", true);
        set_bool_option("ssu2.enabled", false);
        set_bool_option("meshnets.yggdrasil", false);
        // Plan 090 correction: both `port` and `ntcp2.port` are
        // registered as `value<uint16_t>()` in i2pd Config.cpp lines
        // 63 and 331. Storing as `int` (the default `set_int_option`
        // overload) would throw `boost::bad_any_cast` when i2pd
        // extracts the value as `uint16_t`. Use the typed uint16_t
        // overload.
        set_uint16_option("port", cfg.local_port);
        set_uint16_option("ntcp2.port", cfg.local_port);
        set_string_option("address4", cfg.local_address);
        set_string_option("host", cfg.local_address);
        set_string_option("ifname4", "");
        set_string_option("datadir", cfg.data_dir.string());
        set_int_option("netid", cfg.network_id);
        set_bool_option("reservedrange", false);
        set_string_option("log", "file");
        set_string_option("logfile", (cfg.data_dir / "i2pd.log").string());
        set_string_option("loglevel", "debug");
        set_string_option("family", "i2pr-interop-driver");
        set_bool_option("trust.enabled", false);
        set_int_option("share", 100);
        set_int_option("limits.transittunnels", 0);
    } catch (const std::exception& exc) {
        failure_reason = std::string("config-set:") + exc.what();
        return false;
    }

    // Step 4: filesystem ownership. The data directory is created
    // restricted to mode 0700 by the build script; the driver simply
    // refuses to operate outside that directory.
    try {
        std::filesystem::create_directories(cfg.data_dir);
    } catch (const std::exception& exc) {
        failure_reason = std::string("data-dir-mkdir:") + exc.what();
        return false;
    }
    try {
        std::filesystem::permissions(
            cfg.data_dir,
            std::filesystem::perms::owner_all |
                std::filesystem::perms::group_all |
                std::filesystem::perms::others_read,
            std::filesystem::perm_options::replace);
    } catch (...) {
        // The permissions helper is best-effort on hosts that ignore
        // chmod. The runner still owns the directory lifetime.
    }

    // Step 4b: disable reserved-range filtering so loopback
    // addresses (127.0.0.0/8) survive RouterInfo deserialization.
    // i2pd's `Transports::IsInReservedRange` defaults to enabled
    // (Transports.cpp line 156, `m_CheckReserved(true)`), which
    // marks 127.0.0.0/8 addresses as invalid during `ReadFromBuffer`
    // (RouterInfo.cpp lines 256-262) and silently strips the
    // `host` material. The Plan 046 rootless sealed-namespace lane,
    // the Plan 048/049 Multipass recovery lane, and the Plan 086
    // host-loopback-development lane all use loopback or sealed
    // namespaces; reserved-range filtering must be off for the
    // published NTCP2 address to round-trip through the buffer.
    i2p::transport::transports.SetCheckReserved(false);

    // Step 5: crypto::InitCrypto(false)
    try {
        i2p::crypto::InitCrypto(false);
    } catch (const std::exception& exc) {
        failure_reason = std::string("crypto-init:") + exc.what();
        return false;
    }

    // Plan 091: set the network ID on RouterContext. The i2pd
    // standalone daemon reads `netid` and calls `SetNetID` between
    // `InitCrypto` and `context.Init`. The driver must do the same
    // or `RouterContext::GetNetID` returns the default
    // `I2PD_NET_ID` (=2) and the NTCP2 listener rejects the
    // SessionRequest with `networkID 99 mismatch. Expected 2`.
    i2p::context.SetNetID(cfg.network_id);

    // Plan 091: start the logger so i2pd transport warnings
    // (KDF failure, MAC mismatch, etc.) are visible in the driver
    // log file. The driver stops the logger in main() before exit.
    i2p::log::Logger().SendTo((cfg.data_dir / "i2pd.log").string());
    i2p::log::Logger().Start();

    // Step 6: context.Init(). This loads the local identity (or
    // generates fresh keys) and produces a signed local RouterInfo.
    try {
        i2p::context.Init();
        rt.context_initialised = true;
    } catch (const std::exception& exc) {
        failure_reason = std::string("context-init:") + exc.what();
        return false;
    }

    // Step 7: NetDb start. The in-memory NetDB is required before
    // peer RouterInfos may be imported.
    try {
        i2p::data::netdb.Start();
        rt.netdb_started = true;
    } catch (const std::exception& exc) {
        failure_reason = std::string("netdb-start:") + exc.what();
        return false;
    }

    // Step 8: transports.Start(true /*NTCP2*/, false /*SSU2*/). This
    // creates the NTCP2Server and binds the configured IPv4 endpoint
    // when address4 + ntcp2.port are populated. SSU2 is disabled.
    try {
        i2p::transport::transports.Start(true, false);
        rt.transports_started = true;
    } catch (const std::exception& exc) {
        failure_reason = std::string("transports-start:") + exc.what();
        return false;
    }

    // Step 9: context.Start(). Routed services (publish timer etc.)
    // are required when the driver submits an outbound message.
    try {
        i2p::context.Start();
        rt.context_started = true;
    } catch (const std::exception& exc) {
        failure_reason = std::string("context-start:") + exc.what();
        return false;
    }

    // Capture the local Router Hash as measured data. The hash is
    // 32 bytes (Tag<32>) encoded as 64 lowercase hex characters.
    auto local_identity = i2p::context.GetIdentity();
    if (local_identity) {
        const auto& ident = local_identity->GetIdentHash();
        rt.local_ident_hash_hex = hex_lower(ident.data(), 32);
    }
    // Plan 090 fail-closed endpoint verification. The driver must
    // refuse to emit `router_info_exported` when the authoritative
    // RouterInfo does not contain a published NTCP2 address whose
    // endpoint equals the configured listener. The non-published
    // branch (RouterContext.cpp lines 152-157) silently strips
    // host/port/i material, which causes the i2pr
    // `exact_ntcp2_address` parser to reject the peer RouterInfo.
    {
        const auto& ri = i2p::context.GetRouterInfo();
        auto ntcp2_v4 = ri.GetPublishedNTCP2V4Address();
        bool endpoint_ok = false;
        if (ntcp2_v4) {
            auto host_string = ntcp2_v4->host.to_string();
            if (host_string == cfg.local_address &&
                ntcp2_v4->port == cfg.local_port) {
                endpoint_ok = true;
            }
        }
        if (!endpoint_ok) {
            failure_reason = "router-info-endpoint-mismatch";
            return false;
        }
    }
    rt.router_info_exported = true;
    emit_event(writer, cfg, "router_info_exported", std::nullopt,
               std::nullopt, std::nullopt, std::nullopt,
               rt.local_ident_hash_hex);
    std::string written_path = write_local_router_info(cfg);
    if (written_path.find('/') == std::string::npos) {
        failure_reason = "router-info-write-failed";
        return false;
    }
    rt.router_info_written = true;
    rt.router_info_path = written_path;
    return true;
}

bool import_peer_router_info(const DriverConfig& cfg, EventWriter& writer,
                             std::string& failure_reason) {
    failure_reason.clear();
    if (!std::filesystem::exists(cfg.peer_router_info_path)) {
        failure_reason = "peer-router-info-missing";
        emit_event(writer, cfg, "peer_router_info_rejected", std::nullopt,
                   std::nullopt, std::nullopt,
                   std::string("peer-router-info-missing"));
        return false;
    }
    std::ifstream handle(cfg.peer_router_info_path, std::ios::binary);
    if (!handle) {
        failure_reason = "peer-router-info-unreadable";
        emit_event(writer, cfg, "peer_router_info_rejected", std::nullopt,
                   std::nullopt, std::nullopt,
                   std::string("peer-router-info-unreadable"));
        return false;
    }
    std::vector<std::uint8_t> bytes((std::istreambuf_iterator<char>(handle)),
                                    std::istreambuf_iterator<char>{});
    i2p::data::RouterInfo peer_info(bytes.data(), bytes.size());
    if (!peer_info.GetIdentity()) {
        failure_reason = "peer-router-info-identity-missing";
        emit_event(writer, cfg, "peer_router_info_rejected", std::nullopt,
                   std::nullopt, std::nullopt, failure_reason);
        return false;
    }
    auto peer_ident_hash = peer_info.GetIdentity()->GetIdentHash();
    auto peer_ident_hash_hex = hex_lower(peer_ident_hash.data(), 32);
    if (peer_ident_hash_hex != cfg.expected_peer_router_hash_sha256) {
        failure_reason = "peer-router-hash-mismatch";
        emit_event(writer, cfg, "peer_router_info_rejected", std::nullopt,
                   std::nullopt, std::nullopt, failure_reason);
        return false;
    }
    try {
        auto imported = i2p::data::netdb.AddRouterInfo(bytes.data(), bytes.size());
        if (!imported) {
            failure_reason = "peer-router-info-import-failed";
            emit_event(writer, cfg, "peer_router_info_rejected", std::nullopt,
                       std::nullopt, std::nullopt, failure_reason);
            return false;
        }
        auto looked_up = i2p::data::netdb.FindRouter(peer_ident_hash);
        if (!looked_up ||
            hex_lower(looked_up->GetIdentity()->GetIdentHash().data(), 32) !=
                cfg.expected_peer_router_hash_sha256) {
            failure_reason = "peer-router-info-lookup-failed";
            emit_event(writer, cfg, "peer_router_info_rejected", std::nullopt,
                       std::nullopt, std::nullopt, failure_reason);
            return false;
        }
    } catch (const std::exception& exc) {
        failure_reason = std::string("peer-router-info-import:") + exc.what();
        emit_event(writer, cfg, "peer_router_info_rejected", std::nullopt,
                   std::nullopt, std::nullopt, failure_reason);
        return false;
    }
    (void)0;
    emit_event(writer, cfg, "peer_router_info_validated");
    return true;
}

bool construct_delivery_status_message(const DriverConfig& cfg, EventWriter& writer,
                                       std::shared_ptr<i2p::I2NPMessage>& out,
                                       std::string& failure_reason) {
    try {
        out = i2p::CreateDeliveryStatusMsg(cfg.delivery_status_message_id);
    } catch (const std::exception& exc) {
        failure_reason = std::string("create-delivery-status:") + exc.what();
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt, failure_reason);
        return false;
    }
    if (!out) {
        failure_reason = "create-delivery-status-null";
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt, std::string("create-delivery-status-null"));
        return false;
    }
    return true;
}

int run_inspect(const DriverConfig& cfg) {
    if (!path_is_owned(cfg.output_dir)) {
        std::cerr << "i2pd-direct-driver: output_dir is not owned: "
                  << cfg.output_dir.string() << std::endl;
        return 65;
    }
    auto events_path = cfg.output_dir / "events.ndjson";
    EventWriter writer(events_path);
    emit_event(writer, cfg, "process_started");
    if (!pinned_libraries_linked()) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("pinned-libraries-not-linked"));
        return 66;
    }

    // listen/dial modes reserve the deeper transport paths. Inspect
    // mode performs real initialization, emits the listener /
    // router_info markers, and shuts down cleanly. No fake wire
    // events are emitted; the inspect mode is intentionally bounded
    // to the local runtime surface.
    OwnedRuntime rt;
    std::string failure_reason;
    if (!initialise_i2pd_runtime(cfg, writer, rt, failure_reason)) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt, failure_reason);
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    if (i2p::transport::transports.IsBoundNTCP2()) {
        emit_event(writer, cfg, "listener_ready");
    }
    i2pr::i2pdinterop::ResetObserverSink();
    shutdown_runtime(rt, nullptr, nullptr);
    emit_event(writer, cfg, "terminal_clean");
    return 0;
}

int run_listen(const DriverConfig& cfg) {
    if (!path_is_owned(cfg.output_dir)) {
        std::cerr << "i2pd-direct-driver: output_dir is not owned: "
                  << cfg.output_dir.string() << std::endl;
        return 65;
    }
    auto events_path = cfg.output_dir / "events.ndjson";
    EventWriter writer(events_path);
    emit_event(writer, cfg, "process_started");
    if (!pinned_libraries_linked()) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("pinned-libraries-not-linked"));
        return 66;
    }

    // Plan 093: the observer generation is advanced and every
    // bounded sequence ring is reset *before* the i2pd transport
    // threads start. ``listener_ready`` (below) does not reset the
    // rings or change the generation. The wait primitives record
    // the active generation before any wait starts; a wait rejects
    // metadata from a different generation. Stale events from an
    // earlier invocation cannot satisfy the target waits.
    i2pr::i2pdinterop::BeginListenerGeneration();
    const std::uint64_t active_generation =
        i2pr::i2pdinterop::ObserverCurrentGeneration();

    // Plan 093: derive the expected peer Router Hash from the
    // imported peer RouterInfo bytes before the listener waits.
    // The receive and send predicate waits both require the entry's
    // peer Router Hash to equal this hash.
    std::uint64_t expected_peer_hash[4] = {0, 0, 0, 0};
    {
        std::ifstream handle(cfg.peer_router_info_path, std::ios::binary);
        std::vector<std::uint8_t> bytes(
            (std::istreambuf_iterator<char>(handle)),
            std::istreambuf_iterator<char>());
        if (handle.good() && !bytes.empty()) {
            i2p::data::RouterInfo peer_info(bytes.data(), bytes.size());
            auto peer_ident_hash = peer_info.GetIdentity()->GetIdentHash();
            std::memcpy(expected_peer_hash, peer_ident_hash.data(),
                        sizeof(expected_peer_hash));
        }
    }

    OwnedRuntime rt;
    std::string failure_reason;
    if (!initialise_i2pd_runtime(cfg, writer, rt, failure_reason)) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt, failure_reason);
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    if (!i2p::transport::transports.IsBoundNTCP2()) {
        failure_reason = "ntcp2-listener-not-bound";
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt, failure_reason);
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    emit_event(writer, cfg, "listener_ready");

    // Plan 091: the listener waits boundedly for the i2pd transport
    // to record that the real pinned NTCP2 transport accepted a TCP
    // connection from the i2pr initiator. The wait primitive only
    // ever reads observer metadata; it never fabricates data.
    i2pr::i2pdinterop::ObserverMetadata tcp_md{};
    const std::uint32_t tcp_accept_timeout_ms = cfg.handshake_timeout_ms;
    if (!i2pr::i2pdinterop::WaitForTcpAccepted(tcp_md, tcp_accept_timeout_ms)) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("listening-tcp-accept-timeout"));
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    emit_event(writer, cfg, "tcp_accepted");

    // Plan 083: the listener waits boundedly for the peer to
    // complete a real NTCP2 handshake and deliver one authenticated
    // I2NP frame. The wait primitives only ever read observer
    // metadata; they never fabricate data.
    i2pr::i2pdinterop::ObserverMetadata auth_md{};
    const std::uint32_t handshake_timeout_ms = cfg.handshake_timeout_ms;
    if (!i2pr::i2pdinterop::WaitForAuthenticated(auth_md, handshake_timeout_ms)) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("listening-handshake-timeout"));
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    emit_event(writer, cfg, "ntcp2_authenticated", std::nullopt, std::nullopt,
               std::nullopt, std::nullopt);

    // Plan 093: the listener waits boundedly for the i2pd transport
    // to deliver an authenticated and decoded I2NP frame with the
    // exact target DeliveryStatus correlation (configured message
    // ID, configured peer Router Hash, active generation,
    // post-baseline observation_sequence). An automatic
    // RouterInfo / DatabaseStore send that i2pd issues on inbound
    // sessions before the target DeliveryStatus cannot satisfy the
    // predicate.
    i2pr::i2pdinterop::ObserverMetadata recv_md{};
    const std::uint64_t receive_baseline =
        i2pr::i2pdinterop::ObserverReceiveSequence();
    const std::uint32_t data_timeout_ms = cfg.data_phase_timeout_ms;
    if (!i2pr::i2pdinterop::WaitForReceivedDeliveryStatusAfter(
            active_generation, receive_baseline,
            expected_peer_hash,
            cfg.delivery_status_message_id, data_timeout_ms, recv_md)) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("listening-data-phase-timeout"));
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    emit_event(writer, cfg, "frame_authenticated_and_decrypted",
               recv_md.delivery_status_message_id,
               recv_md.i2np_type, recv_md.frame_sequence);
    emit_event(writer, cfg, "i2np_message_decoded",
               recv_md.delivery_status_message_id,
               recv_md.i2np_type, recv_md.frame_sequence);

    // Plan 093: the listener composes a DeliveryStatus with the
    // exact correlation message_id and submits it through the real
    // i2pd transport, then waits boundedly for the asynchronous
    // send to complete via the target predicate. The wait observes
    // the active generation and a strictly greater send sequence
    // than the recorded baseline.
    {
        std::ifstream handle(cfg.peer_router_info_path, std::ios::binary);
        std::vector<std::uint8_t> bytes(
            (std::istreambuf_iterator<char>(handle)),
            std::istreambuf_iterator<char>());
        if (handle.good() && !bytes.empty()) {
            i2p::data::RouterInfo peer_info(bytes.data(), bytes.size());
            auto peer_ident_hash = peer_info.GetIdentity()->GetIdentHash();
            auto reply = i2p::CreateDeliveryStatusMsg(
                cfg.delivery_status_message_id);
            try {
                auto future = i2p::transport::transports.SendMessage(
                    peer_ident_hash, reply);
                (void)future.wait_for(std::chrono::milliseconds(0));
            } catch (const std::exception& exc) {
                emit_event(writer, cfg, "terminal_rejected", std::nullopt,
                           std::nullopt, std::nullopt,
                           std::string("listener-send-message:") + exc.what());
                shutdown_runtime(rt, nullptr, nullptr);
                return 66;
            }
            const std::uint64_t send_baseline =
                i2pr::i2pdinterop::ObserverSentSequence();
            i2pr::i2pdinterop::ObserverMetadata sent_md{};
            const std::uint32_t send_timeout_ms = cfg.handshake_timeout_ms;
            if (!i2pr::i2pdinterop::WaitForSentDeliveryStatusAfter(
                    active_generation, send_baseline,
                    expected_peer_hash,
                    cfg.delivery_status_message_id, send_timeout_ms,
                    sent_md)) {
                emit_event(writer, cfg, "terminal_rejected", std::nullopt,
                           std::nullopt, std::nullopt,
                           std::string("listener-send-timeout"));
                shutdown_runtime(rt, nullptr, nullptr);
                return 66;
            }
            emit_event(writer, cfg, "frame_emitted",
                       sent_md.delivery_status_message_id,
                       sent_md.i2np_type, sent_md.frame_sequence);
        }
    }

    shutdown_runtime(rt, nullptr, nullptr);
    emit_event(writer, cfg, "terminal_clean");
    return 0;
}

int run_dial(const DriverConfig& cfg) {
    if (!path_is_owned(cfg.output_dir)) {
        std::cerr << "i2pd-direct-driver: output_dir is not owned: "
                  << cfg.output_dir.string() << std::endl;
        return 65;
    }
    if (!path_is_owned(cfg.data_dir)) {
        std::cerr << "i2pd-direct-driver: data_dir is not owned: "
                  << cfg.data_dir.string() << std::endl;
        return 65;
    }
    if (!std::filesystem::exists(cfg.peer_router_info_path)) {
        std::cerr << "i2pd-direct-driver: peer_router_info_path missing: "
                  << cfg.peer_router_info_path.string() << std::endl;
        return 65;
    }
    auto events_path = cfg.output_dir / "events.ndjson";
    EventWriter writer(events_path);
    emit_event(writer, cfg, "process_started");
    if (!pinned_libraries_linked()) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("pinned-libraries-not-linked"));
        return 66;
    }
    OwnedRuntime rt;
    std::string failure_reason;
    if (!initialise_i2pd_runtime(cfg, writer, rt, failure_reason)) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt, failure_reason);
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    std::string import_failure;
    if (!import_peer_router_info(cfg, writer, import_failure)) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt, import_failure);
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    std::shared_ptr<i2p::I2NPMessage> message;
    std::string create_failure;
    if (!construct_delivery_status_message(cfg, writer, message,
                                            create_failure)) {
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    emit_event(writer, cfg, "frame_emitted", cfg.delivery_status_message_id,
               /*i2np_type=*/10, /*frame_sequence=*/0);

    // Resolve the peer IdentHash from the imported RouterInfo and
    // submit through the real transport.
    i2p::data::IdentHash peer_ident_hash{};
    {
        std::ifstream handle(cfg.peer_router_info_path, std::ios::binary);
        std::vector<std::uint8_t> bytes(
            (std::istreambuf_iterator<char>(handle)),
            std::istreambuf_iterator<char>{});
        i2p::data::RouterInfo peer_info(bytes.data(), bytes.size());
        peer_ident_hash = peer_info.GetIdentity()->GetIdentHash();
    }

    i2pr::i2pdinterop::ResetObserverSink();
    try {
        auto future = i2p::transport::transports.SendMessage(peer_ident_hash,
                                                              message);
        (void)future.wait_for(std::chrono::milliseconds(0));
    } catch (const std::exception& exc) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("send-message:") + exc.what());
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }

    // Plan 083: wait boundedly for the send observer to record that
    // the asynchronous socket write actually delivered the
    // DeliveryStatus frame.
    i2pr::i2pdinterop::ObserverMetadata sent_md{};
    const std::uint32_t handshake_timeout_ms = cfg.handshake_timeout_ms;
    if (!i2pr::i2pdinterop::WaitForSentI2NP(sent_md, handshake_timeout_ms)) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("dialing-send-timeout"));
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    if (sent_md.delivery_status_message_id != cfg.delivery_status_message_id) {
        emit_event(writer, cfg, "terminal_rejected", std::nullopt, std::nullopt,
                   std::nullopt,
                   std::string("dialing-send-message-id-mismatch"));
        shutdown_runtime(rt, nullptr, nullptr);
        return 66;
    }
    emit_event(writer, cfg, "frame_emitted", sent_md.delivery_status_message_id,
               sent_md.i2np_type, sent_md.frame_sequence);

    shutdown_runtime(rt, nullptr, nullptr);
    emit_event(writer, cfg, "terminal_clean");
    return 0;
}

}  // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        std::cerr << "i2pd-direct-driver: usage: --config <path>" << std::endl;
        return 64;
    }
    std::filesystem::path config_path;
    for (int i = 1; i < argc; ++i) {
        std::string_view key(argv[i]);
        if (key == "--config") {
            if (i + 1 >= argc) {
                std::cerr << "i2pd-direct-driver: missing --config value"
                          << std::endl;
                return 64;
            }
            config_path = argv[++i];
        } else if (key == "--help" || key == "-h") {
            std::cerr << "i2pd-direct-driver: Plan 076 i2pd direct NTCP2 "
                         "driver"
                      << std::endl;
            return 0;
        } else {
            std::cerr << "i2pd-direct-driver: unknown option: " << key
                      << std::endl;
            return 64;
        }
    }
    if (config_path.empty()) {
        std::cerr << "i2pd-direct-driver: --config is required" << std::endl;
        return 64;
    }
    DriverConfig cfg;
    std::vector<std::string> extras;
    std::vector<std::string> missing;
    try {
        parse_strict_config(config_path, cfg, extras, missing);
    } catch (const std::exception& exc) {
        std::cerr << "i2pd-direct-driver: " << exc.what() << std::endl;
        return 65;
    }
    if (!extras.empty()) {
        std::cerr << "i2pd-direct-driver: unknown config field: "
                  << extras.front() << std::endl;
        return 65;
    }
    if (!missing.empty()) {
        std::cerr << "i2pd-direct-driver: missing config field: "
                  << missing.front() << std::endl;
        return 65;
    }
    try {
        validate_config(cfg);
    } catch (const std::exception& exc) {
        std::cerr << "i2pd-direct-driver: " << exc.what() << std::endl;
        return 65;
    }
    if (cfg.mode == "inspect") {
        int rc = run_inspect(cfg);
        i2p::log::Logger().Stop();
        return rc;
    }
    if (cfg.mode == "listen") {
        int rc = run_listen(cfg);
        i2p::log::Logger().Stop();
        return rc;
    }
    if (cfg.mode == "dial") {
        int rc = run_dial(cfg);
        i2p::log::Logger().Stop();
        return rc;
    }
    std::cerr << "i2pd-direct-driver: mode-not-allowlisted: " << cfg.mode
              << std::endl;
    return 64;
}
