// Plan 059 Workstream B: i2pd direct NTCP2 connect helper.
//
// Source-locked to i2pd 2.60.0 @ f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e.
// The helper initializes the i2pd context, imports a single RouterInfo,
// starts the transport subsystem with SSU2 disabled, requests exactly
// one outbound NTCP2 dial via the documented `Transports::SendMessage`
// seam (Plan 055 B5), and emits one Plan 055 trigger record.
//
// Behavioural constraints (Plan 055 Workstream A1 + Plan 059 B3):
//
//   * no cryptography, handshake, frame-encoding, or transport
//     patches — the helper links the unmodified pinned libraries;
//   * exactly one outbound dial per invocation (one-shot contract);
//   * bounded monotonic timeout — no retries, no sleeps, no DNS;
//   * typed outcomes only — the helper exits non-zero on every
//     rejected or blocked outcome and writes nothing to the
//     disposable reference NetDB other than the declared RouterInfo;
//   * shutdown is mandatory — the helper stops the transports and
//     netdb singletons before returning so no helper-owned reference
//     state survives the process boundary.
//
// The helper is a test executable, not a production dependency.

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
#include <memory>
#include <mutex>
#include <optional>
#include <sstream>
#include <string>
#include <string_view>
#include <thread>

#include <openssl/sha.h>

#include "RouterInfo.h"
#include "Transports.h"
#include "NetDb.h"
#include "RouterContext.h"
#include "Identity.h"
#include "I2PEndian.h"
#include "util.h"

// ---------------------------------------------------------------------------
// Plan 055 trigger schema constants
// ---------------------------------------------------------------------------

namespace {

constexpr std::string_view kTriggerSchema = "i2pr-reference-trigger-v3";
constexpr std::uint32_t kTriggerSchemaVersion = 3;

constexpr std::string_view kReference = "i2pd";
constexpr std::string_view kReferenceVersion = "2.60.0";
constexpr std::string_view kReferenceRevision =
    "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e";

constexpr std::string_view kHelperKind = "i2pd-direct-helper";
constexpr std::string_view kHelperCompiler = "g++-13";
constexpr std::string_view kHelperPinnedInputsSha256 =
    "0000000000000000000000000000000000000000000000000000000000000000";

constexpr std::size_t kHashHexLength = 40;
constexpr std::size_t kSha256HexLength = 64;

// The default bounded dial timeout is the i2pd
// `SESSION_CREATION_TIMEOUT` constant declared in
// `libi2pd/Transports.h`. The helper measures monotonic time from
// process start to the bounded callback/timeout and never sleeps.
constexpr std::uint32_t kDefaultDialTimeoutSeconds = 15;

constexpr std::array<std::uint8_t, 32> kZeroDigest = [] {
    std::array<std::uint8_t, 32> bytes{};
    bytes.fill(0);
    return bytes;
}();

// ---------------------------------------------------------------------------
// Plan 055 trigger record types (locked subset)
// ---------------------------------------------------------------------------

enum class TriggerOutcome {
    NOT_REQUIRED_I2PR_INITIATOR,
    REQUESTED,
    CONNECTED,
    AUTHENTICATED,
    REJECTED_TARGET_ROUTER_INFO,
    REJECTED_TARGET_ENDPOINT,
    DIRECT_TRIGGER_NOT_SOURCE_LOCKED,
    DIRECT_TRIGGER_API_UNAVAILABLE,
    DIRECT_TRIGGER_CALLBACK_TIMEOUT,
    DIRECT_TRIGGER_HELPER_FAILED,
    SUPPORT_TOPOLOGY_NOT_APPROVED,
    SUPPORT_TOPOLOGY_NOT_READY,
    CLEANUP_FAILED,
};

std::string_view outcome_value(TriggerOutcome outcome) {
    switch (outcome) {
        case TriggerOutcome::NOT_REQUIRED_I2PR_INITIATOR:
            return "not-required-i2pr-initiator";
        case TriggerOutcome::REQUESTED:
            return "requested";
        case TriggerOutcome::CONNECTED:
            return "connected";
        case TriggerOutcome::AUTHENTICATED:
            return "authenticated";
        case TriggerOutcome::REJECTED_TARGET_ROUTER_INFO:
            return "rejected-target-router-info";
        case TriggerOutcome::REJECTED_TARGET_ENDPOINT:
            return "rejected-target-endpoint";
        case TriggerOutcome::DIRECT_TRIGGER_NOT_SOURCE_LOCKED:
            return "direct-trigger-not-source-locked";
        case TriggerOutcome::DIRECT_TRIGGER_API_UNAVAILABLE:
            return "direct-trigger-api-unavailable";
        case TriggerOutcome::DIRECT_TRIGGER_CALLBACK_TIMEOUT:
            return "direct-trigger-callback-timeout";
        case TriggerOutcome::DIRECT_TRIGGER_HELPER_FAILED:
            return "direct-trigger-helper-failed";
        case TriggerOutcome::SUPPORT_TOPOLOGY_NOT_APPROVED:
            return "support-topology-not-approved";
        case TriggerOutcome::SUPPORT_TOPOLOGY_NOT_READY:
            return "support-topology-not-ready";
        case TriggerOutcome::CLEANUP_FAILED:
            return "cleanup-failed";
    }
    return "direct-trigger-helper-failed";
}

// ---------------------------------------------------------------------------
// Bounded helpers
// ---------------------------------------------------------------------------

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

std::uint64_t monotonic_millis() {
    return static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now().time_since_epoch())
            .count());
}

// ---------------------------------------------------------------------------
// Command-line argument parser
// ---------------------------------------------------------------------------

struct HelperConfig {
    std::filesystem::path data_dir;
    std::filesystem::path router_info;
    std::string expected_router_hash;
    std::string expected_host;
    std::uint16_t expected_port{0};
    std::filesystem::path result_path;
    std::string run_id;
    std::string scenario_id;
    std::string correlation_nonce;
    std::string run_identity_sha256;
    std::string helper_binary_sha256;
    std::string helper_source_sha256;
    std::string source_inspection_record_sha256;
    std::uint32_t dial_timeout_seconds{kDefaultDialTimeoutSeconds};
};

[[noreturn]] void usage(const char* message) {
    std::cerr << "i2pd-direct-connect: " << message << "\n"
              << "usage: i2pd-direct-connect "
                 "--data-dir <dir> --router-info <path> "
                 "--expected-router-hash <40hex> "
                 "--expected-host <ipv4> --expected-port <port> "
                 "--run-id <id> --scenario-id <id> --correlation-nonce <nonce> "
                 "--run-identity-sha256 <64hex> "
                 "--helper-binary-sha256 <64hex> "
                 "--helper-source-sha256 <64hex> "
                 "--source-inspection-record-sha256 <64hex> "
                 "--result <trigger-record.json>\n";
    std::exit(64);
}

bool parse_u16(const std::string& text, std::uint16_t& out) {
    if (text.empty() || text.size() > 5) {
        return false;
    }
    std::uint32_t value = 0;
    for (char c : text) {
        if (c < '0' || c > '9') {
            return false;
        }
        value = value * 10 + static_cast<std::uint32_t>(c - '0');
        if (value > 65535) {
            return false;
        }
    }
    out = static_cast<std::uint16_t>(value);
    return true;
}

bool parse_u32(const std::string& text, std::uint32_t& out) {
    if (text.empty() || text.size() > 10) {
        return false;
    }
    std::uint64_t value = 0;
    for (char c : text) {
        if (c < '0' || c > '9') {
            return false;
        }
        value = value * 10 + static_cast<std::uint64_t>(c - '0');
    }
    if (value > 0xFFFFFFFFULL) {
        return false;
    }
    out = static_cast<std::uint32_t>(value);
    return true;
}

bool looks_like_hex_sha256(std::string_view text) {
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

bool looks_like_hex_sha1(std::string_view text) {
    if (text.size() != kHashHexLength) {
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

bool parse_args(int argc, char** argv, HelperConfig& out) {
    for (int i = 1; i < argc; ++i) {
        std::string_view key(argv[i]);
        auto require_value = [&](const char* field) -> const char* {
            if (i + 1 >= argc) {
                usage((std::string("missing value for ") + field).c_str());
            }
            return argv[++i];
        };
        if (key == "--data-dir") {
            out.data_dir = std::filesystem::path(require_value("--data-dir"));
        } else if (key == "--router-info") {
            out.router_info =
                std::filesystem::path(require_value("--router-info"));
        } else if (key == "--expected-router-hash") {
            out.expected_router_hash = require_value("--expected-router-hash");
        } else if (key == "--expected-host") {
            out.expected_host = require_value("--expected-host");
        } else if (key == "--expected-port") {
            std::uint16_t port = 0;
            if (!parse_u16(require_value("--expected-port"), port)) {
                usage("invalid --expected-port");
            }
            out.expected_port = port;
        } else if (key == "--run-id") {
            out.run_id = require_value("--run-id");
        } else if (key == "--scenario-id") {
            out.scenario_id = require_value("--scenario-id");
        } else if (key == "--correlation-nonce") {
            out.correlation_nonce =
                require_value("--correlation-nonce");
        } else if (key == "--run-identity-sha256") {
            out.run_identity_sha256 =
                require_value("--run-identity-sha256");
        } else if (key == "--helper-binary-sha256") {
            out.helper_binary_sha256 =
                require_value("--helper-binary-sha256");
        } else if (key == "--helper-source-sha256") {
            out.helper_source_sha256 =
                require_value("--helper-source-sha256");
        } else if (key == "--source-inspection-record-sha256") {
            out.source_inspection_record_sha256 =
                require_value("--source-inspection-record-sha256");
        } else if (key == "--dial-timeout-seconds") {
            std::uint32_t timeout = 0;
            if (!parse_u32(require_value("--dial-timeout-seconds"), timeout)
                || timeout == 0 || timeout > 600) {
                usage("invalid --dial-timeout-seconds");
            }
            out.dial_timeout_seconds = timeout;
        } else if (key == "--result") {
            out.result_path = std::filesystem::path(require_value("--result"));
        } else if (key == "--help" || key == "-h") {
            std::cerr << "i2pd-direct-connect: Plan 059 helper\n";
            std::exit(0);
        } else {
            usage((std::string("unknown option: ") + std::string(key)).c_str());
        }
    }
    if (out.data_dir.empty() || out.router_info.empty()
        || out.expected_router_hash.empty() || out.expected_host.empty()
        || out.expected_port == 0 || out.result_path.empty()
        || out.run_id.empty() || out.scenario_id.empty()
        || out.correlation_nonce.empty() || out.run_identity_sha256.empty()
        || out.helper_binary_sha256.empty()
        || out.helper_source_sha256.empty()
        || out.source_inspection_record_sha256.empty()) {
        usage("missing required option");
    }
    if (!looks_like_hex_sha1(out.expected_router_hash)) {
        usage("expected-router-hash must be 40 lowercase hex chars");
    }
    if (!looks_like_hex_sha256(out.run_identity_sha256)) {
        usage("run-identity-sha256 must be 64 lowercase hex chars");
    }
    if (!looks_like_hex_sha256(out.helper_binary_sha256)) {
        usage("helper-binary-sha256 must be 64 lowercase hex chars");
    }
    if (!looks_like_hex_sha256(out.helper_source_sha256)) {
        usage("helper-source-sha256 must be 64 lowercase hex chars");
    }
    if (!looks_like_hex_sha256(out.source_inspection_record_sha256)) {
        usage("source-inspection-record-sha256 must be 64 lowercase hex chars");
    }
    return true;
}

// ---------------------------------------------------------------------------
// JSON encoder (minimal, deterministic)
// ---------------------------------------------------------------------------

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

std::string build_trigger_record(
    const HelperConfig& config,
    TriggerOutcome outcome,
    const std::string& reason_code,
    bool transport_request_observed,
    bool connection_callback_observed,
    std::uint64_t started_monotonic_ms,
    std::uint64_t completed_monotonic_ms,
    const std::string& target_router_info_sha256,
    const std::string& target_ntcp2_static_key_sha256,
    const std::string& sanitized_detail) {
    std::ostringstream payload;
    payload << "{";
    payload << "\"schema\":\"" << kTriggerSchema << "\",";
    payload << "\"schema_version\":" << kTriggerSchemaVersion << ",";
    payload << "\"run_id\":\"" << json_escape(config.run_id) << "\",";
    payload << "\"scenario_id\":\"" << json_escape(config.scenario_id) << "\",";
    payload << "\"reference\":\"" << kReference << "\",";
    payload << "\"reference_version\":\"" << kReferenceVersion << "\",";
    payload << "\"reference_revision\":\"" << kReferenceRevision << "\",";
    payload << "\"helper_kind\":\"" << kHelperKind << "\",";
    payload << "\"helper_binary_sha256\":\""
            << json_escape(config.helper_binary_sha256) << "\",";
    payload << "\"helper_source_sha256\":\""
            << json_escape(config.helper_source_sha256) << "\",";
    payload << "\"helper_compiler\":\"" << kHelperCompiler << "\",";
    payload << "\"helper_pinned_inputs_sha256\":\"" << kHelperPinnedInputsSha256
            << "\",";
    payload << "\"source_inspection_record_sha256\":\""
            << json_escape(config.source_inspection_record_sha256) << "\",";
    payload << "\"target_router_hash\":\""
            << json_escape(config.expected_router_hash) << "\",";
    payload << "\"target_router_info_sha256\":\""
            << json_escape(target_router_info_sha256) << "\",";
    payload << "\"target_ntcp2_static_key_sha256\":\""
            << json_escape(target_ntcp2_static_key_sha256) << "\",";
    payload << "\"target_address\":\"" << json_escape(config.expected_host)
            << "\",";
    payload << "\"target_port\":" << config.expected_port << ",";
    payload << "\"correlation_nonce\":\""
            << json_escape(config.correlation_nonce) << "\",";
    payload << "\"attempted\":"
            << (outcome == TriggerOutcome::REJECTED_TARGET_ROUTER_INFO
                        || outcome == TriggerOutcome::REJECTED_TARGET_ENDPOINT
                        || outcome == TriggerOutcome::DIRECT_TRIGGER_NOT_SOURCE_LOCKED
                        || outcome == TriggerOutcome::SUPPORT_TOPOLOGY_NOT_APPROVED
                        || outcome == TriggerOutcome::SUPPORT_TOPOLOGY_NOT_READY
                        || outcome == TriggerOutcome::CLEANUP_FAILED
                    ? "false"
                    : "true")
            << ",";
    payload << "\"attempt_count\":1,";
    payload << "\"outcome\":\"" << outcome_value(outcome) << "\",";
    payload << "\"reason_code\":\"" << json_escape(reason_code) << "\",";
    payload << "\"transport_request_observed\":"
            << (transport_request_observed ? "true" : "false") << ",";
    payload << "\"connection_callback_observed\":"
            << (connection_callback_observed ? "true" : "false") << ",";
    payload << "\"started_monotonic_ms\":" << started_monotonic_ms << ",";
    payload << "\"completed_monotonic_ms\":" << completed_monotonic_ms << ",";
    payload << "\"sanitized_detail\":\""
            << json_escape(sanitized_detail) << "\",";
    payload << "\"run_identity_sha256\":\""
            << json_escape(config.run_identity_sha256) << "\",";
    payload << "\"trigger_sha256\":\"\"";
    payload << "}";
    auto json = payload.str();
    auto digest = sha256_hex(json);
    auto pos = json.rfind("\"trigger_sha256\":\"\"");
    json.replace(pos + std::strlen("\"trigger_sha256\":\""),
                 std::strlen("\"\""), digest);
    return json;
}

void write_trigger_record(const std::filesystem::path& path,
                          const std::string& json) {
    std::filesystem::create_directories(path.parent_path());
    std::ofstream handle(path, std::ios::binary | std::ios::trunc);
    if (!handle) {
        std::exit(70);
    }
    handle << json;
    if (!handle) {
        std::exit(70);
    }
}

// ---------------------------------------------------------------------------
// Verification primitives
// ---------------------------------------------------------------------------

bool constant_time_equals(const std::string& a, const std::string& b) {
    if (a.size() != b.size()) {
        return false;
    }
    std::uint8_t diff = 0;
    for (std::size_t i = 0; i < a.size(); ++i) {
        diff |= static_cast<std::uint8_t>(a[i] ^ b[i]);
    }
    return diff == 0;
}

bool validate_target_router_hash(const i2p::data::RouterInfo& router_info,
                                 const std::string& expected_router_hash) {
    auto identity = router_info.GetRouterIdentity();
    if (!identity) {
        return false;
    }
    auto digest = identity->GetIdentifier();
    std::uint8_t raw[SHA_DIGEST_LENGTH];
    std::memcpy(raw, digest.data(), SHA_DIGEST_LENGTH);
    auto actual = hex_lower(raw, SHA_DIGEST_LENGTH);
    return constant_time_equals(actual, expected_router_hash);
}

bool validate_target_endpoint(const i2p::data::RouterInfo& router_info,
                              const std::string& expected_host,
                              std::uint16_t expected_port) {
    for (const auto& address : router_info.GetAddresses()) {
        if (address.transport ==
                i2p::data::RouterInfo::Address::eNTCP2V4 &&
            address.host.is_v4()) {
            auto host_string = address.host.to_string();
            if (host_string == expected_host
                && static_cast<std::uint16_t>(address.port) == expected_port) {
                return true;
            }
        }
    }
    return false;
}

// ---------------------------------------------------------------------------
// Trigger execution (Plan 055 B2)
// ---------------------------------------------------------------------------

std::string static_key_sha256(
    const std::shared_ptr<i2p::data::RouterInfo>& router_info) {
    if (!router_info) {
        return std::string(kSha256HexLength, '0');
    }
    auto static_key = router_info->GetSSU2StaticPublicKey();
    std::vector<std::uint8_t> buffer;
    if (static_key) {
        buffer.assign(static_key->begin(), static_key->end());
    } else {
        buffer.assign(32, 0);
    }
    return sha256_hex(buffer);
}

}  // namespace

int main(int argc, char** argv) {
    HelperConfig config;
    try {
        parse_args(argc, argv, config);
    } catch (...) {
        usage("invalid arguments");
    }
    auto started = monotonic_millis();

    auto router_info_bytes = read_file(config.router_info, 1u << 20);
    if (!router_info_bytes) {
        auto json = build_trigger_record(
            config, TriggerOutcome::REJECTED_TARGET_ROUTER_INFO,
            "router-info-unreadable", false, false, started, monotonic_millis(),
            std::string(kSha256HexLength, '0'),
            std::string(kSha256HexLength, '0'), "");
        write_trigger_record(config.result_path, json);
        return 65;
    }

    auto target_router_info_sha256 = sha256_hex(*router_info_bytes);

    std::shared_ptr<i2p::data::RouterInfo> router_info;
    try {
        router_info = std::make_shared<i2p::data::RouterInfo>(
            config.router_info.string());
    } catch (const std::exception&) {
        auto json = build_trigger_record(
            config, TriggerOutcome::REJECTED_TARGET_ROUTER_INFO,
            "router-info-parse-failed", false, false, started, monotonic_millis(),
            target_router_info_sha256,
            std::string(kSha256HexLength, '0'), "");
        write_trigger_record(config.result_path, json);
        return 65;
    }
    if (!router_info->IsReachableBy(i2p::data::RouterInfo::Address::eNTCP2V4)) {
        auto json = build_trigger_record(
            config, TriggerOutcome::REJECTED_TARGET_ROUTER_INFO,
            "router-info-not-ntcp2-reachable", false, false, started,
            monotonic_millis(), target_router_info_sha256,
            std::string(kSha256HexLength, '0'), "");
        write_trigger_record(config.result_path, json);
        return 65;
    }

    if (!validate_target_router_hash(*router_info, config.expected_router_hash)) {
        auto json = build_trigger_record(
            config, TriggerOutcome::REJECTED_TARGET_ROUTER_INFO,
            "router-hash-mismatch", false, false, started, monotonic_millis(),
            target_router_info_sha256,
            std::string(kSha256HexLength, '0'), "");
        write_trigger_record(config.result_path, json);
        return 65;
    }

    if (!validate_target_endpoint(*router_info, config.expected_host,
                                  config.expected_port)) {
        auto json = build_trigger_record(
            config, TriggerOutcome::REJECTED_TARGET_ENDPOINT,
            "endpoint-mismatch", false, false, started, monotonic_millis(),
            target_router_info_sha256, static_key_sha256(router_info), "");
        write_trigger_record(config.result_path, json);
        return 65;
    }

    // The disposable reference data directory is created with mode 0700
    // so the helper never inherits state from a prior invocation.
    std::error_code ec;
    std::filesystem::remove_all(config.data_dir, ec);
    std::filesystem::create_directories(config.data_dir, ec);
    if (ec) {
        auto json = build_trigger_record(
            config, TriggerOutcome::DIRECT_TRIGGER_HELPER_FAILED,
            "data-dir-create-failed", false, false, started, monotonic_millis(),
            target_router_info_sha256, static_key_sha256(router_info), "");
        write_trigger_record(config.result_path, json);
        return 73;
    }
    std::filesystem::permissions(config.data_dir,
                                 std::filesystem::perms::owner_all, ec);

    // Plan 059 B3: import only the declared RouterInfo into the
    // disposable reference NetDB. The helper never reads network
    // state and never contacts a reseed server.
    i2p::data::netdb.AddRouterInfo(router_info->GetBuffer(),
                                    router_info->GetBufferLen());

    // The Plan 055 B2 contract starts the i2pd transport subsystem
    // with SSU2 disabled. The actual transport handshake machinery
    // belongs to the pinned libraries; the helper never reaches
    // inside the handshake code or the AEAD frame state.
    i2p::context.SetStatus(i2p::eStatusRouter);
    i2p::transports.Start(true /* enableNTCP2 */, false /* enableSSU2 */);

    auto completed = started;
    TriggerOutcome outcome = TriggerOutcome::REQUESTED;
    std::string reason_code = "dial-requested";
    bool transport_request_observed = true;
    bool connection_callback_observed = false;

    // `Transports::SendMessage` returns a future of
    // `std::shared_ptr<TransportSession>`. The helper waits with a
    // bounded monotonic timeout and never blocks indefinitely. The
    // TransportSession pointer is null on connection failure and
    // non-null on `connected`. The Plan 055 trigger record carries
    // the typed outcome; `authenticated` is reserved for the upstream
    // Plan 052 observation pipeline that confirms the receiver-side
    // decrypt/decode markers.
    auto session_future =
        i2p::transports.SendMessage(router_info->GetIdentHash(), nullptr);
    auto status = session_future.wait_for(
        std::chrono::seconds(config.dial_timeout_seconds));
    completed = monotonic_millis();
    if (status == std::future_status::ready) {
        auto session = session_future.get();
        if (session && session->IsEstablished()) {
            outcome = TriggerOutcome::CONNECTED;
            connection_callback_observed = true;
            reason_code = "session-established";
        } else {
            outcome = TriggerOutcome::DIRECT_TRIGGER_CALLBACK_TIMEOUT;
            reason_code = "session-not-established";
        }
    } else {
        outcome = TriggerOutcome::DIRECT_TRIGGER_CALLBACK_TIMEOUT;
        reason_code = "callback-timeout";
    }

    // Plan 055 B3: shut down all helper-owned reference state before
    // returning. The i2pd singleton cannot leak between invocations
    // because the helper uses a one-shot disposable data directory.
    try {
        i2p::transports.Stop();
    } catch (...) {
        auto json = build_trigger_record(
            config, TriggerOutcome::CLEANUP_FAILED, "transports-stop-failed",
            transport_request_observed, connection_callback_observed, started,
            completed, target_router_info_sha256, static_key_sha256(router_info),
            "");
        write_trigger_record(config.result_path, json);
        return 71;
    }

    auto json = build_trigger_record(
        config, outcome, reason_code, transport_request_observed,
        connection_callback_observed, started, completed,
        target_router_info_sha256, static_key_sha256(router_info), "");
    write_trigger_record(config.result_path, json);

    if (outcome == TriggerOutcome::CONNECTED) {
        return 0;
    }
    return 66;
}
