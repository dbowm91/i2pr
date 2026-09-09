//! I2CP SessionConfig option disposition table and bounded projection
//! into `i2pr_client::DestinationConfig`.
//!
//! Plan 165 §6 owns this table. Every supported key has one explicit
//! disposition: applied to the typed [`ProjectedPolicy`], rejected with
//! a typed error, or silently ignored when the specification explicitly
//! defines ignore semantics. Router-wide ceilings remain authoritative:
//! the disposition table never lets an I2CP session bypass a
//! [`DestinationConfig`] ceiling.
//!
//! The parser rejects signed, whitespace-padded, and overflowing values
//! for every numeric option unless the option's syntax explicitly allows
//! them. Unknown keys are recorded but never alter the projected policy.
//!
//! This module owns no sockets, timers, Tokio tasks, or destination
//! secret material. The returned [`ProjectedPolicy`] is typed and
//! transport-safe; the daemon (Plan 167) is responsible for handing it
//! to `i2pr-client`.

use i2pr_client::{DestinationConfig, DestinationConfigError, RegistryConfig};
use i2pr_proto::Mapping;
use i2pr_tunnel::TunnelLifetime;

use super::error::I2cpError;

/// Maximum number of entries an I2CP SessionConfig mapping may carry.
pub const MAX_SESSION_CONFIG_OPTIONS: usize = 64;

/// Maximum UTF-8 byte length of one option key in the SessionConfig.
pub const MAX_SESSION_CONFIG_KEY_BYTES: usize = 96;

/// Maximum UTF-8 byte length of one option value in the SessionConfig.
pub const MAX_SESSION_CONFIG_VALUE_BYTES: usize = 96;

/// The supported LeaseSet type for M9 I2CP sessions (`type 3`).
pub const M9_LEASE_SET_TYPE: u8 = 3;
/// The supported LeaseSet encryption type for M9 I2CP sessions (`type 4`).
pub const M9_LEASE_SET_ENC_TYPE: u8 = 4;

/// The M9 reliability class: best-effort fast receive only.
pub const M9_MESSAGE_RELIABILITY_BEST_EFFORT: &str = "BestEffort";

/// Default I2CP fast-receive posture for M9: only fast receive is
/// delivered by the Plan 168 data plane.
pub const M9_FAST_RECEIVE_DEFAULT: bool = true;

/// Per-option disposition classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionDisposition {
    /// Option was applied to the projected [`ProjectedPolicy`].
    Applied,
    /// Option was rejected; the whole SessionConfig fails.
    Rejected,
    /// Option was accepted and ignored per specification semantics.
    Ignored,
    /// Option key was unknown; recorded but not consumed.
    Unknown,
}

/// One explicit note recorded during projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionNote {
    /// The option key (canonical, sorted).
    pub key: String,
    /// Whether the option was applied, ignored, rejected (recorded as
    /// a pre-rejection reason), or unknown.
    pub disposition: OptionDisposition,
    /// Optional human-readable rationale (redacted; never contains
    /// value bytes for rejected options that may carry credentials).
    pub note: Option<&'static str>,
}

/// Resulting destination policy after projecting SessionConfig options
/// onto [`DestinationConfig`]. The `destination_config` field is the
/// canonical typed output; the `registry_config` field is unchanged for
/// M9 (per-connection session limits live in the session registry,
/// Plan 165 §4).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedPolicy {
    /// The bounded destination configuration honored by `i2pr-client`.
    /// For `LocalZeroHop` mode this carries a valid remote placeholder
    /// (length 2) plus the explicit mode; the daemon creates the real
    /// local routes via the Plan 166 pool seam. The remote minimum-hop
    /// invariant is never lowered to make length 0 pass.
    pub destination_config: DestinationConfig,
    /// Explicit tunnel mode (Plan 172 §8).
    pub tunnel_mode: i2pr_client::DestinationTunnelMode,
    /// Recorder of every option disposition seen during projection.
    pub notes: Vec<OptionNote>,
    /// Selected LeaseSet type (must equal [`M9_LEASE_SET_TYPE`]).
    pub lease_set_type: u8,
    /// Selected LeaseSet encryption type (must equal [`M9_LEASE_SET_ENC_TYPE`]).
    pub lease_set_enc_type: u8,
    /// `i2cp.messageReliability` selection (`BestEffort` for M9).
    pub message_reliability: &'static str,
    /// `i2cp.fastReceive` selection (only `true` is delivered by M9).
    pub fast_receive: bool,
    /// `inbound.allowZeroHop` selection.
    pub inbound_allow_zero_hop: bool,
    /// `outbound.allowZeroHop` selection.
    pub outbound_allow_zero_hop: bool,
    /// `i2cp.dontPublishLeaseSet` selection. `true` installs the
    /// client-signed LS2 locally without public NetDB publication;
    /// local/cross-client routing may still use it (Plan 172 §10).
    pub dont_publish_lease_set: bool,
}

/// Validated SessionConfig ceilings; the same limits apply to every
/// Plan 165 verification pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionConfigLimits {
    /// Maximum SessionConfig entries.
    pub max_options: usize,
    /// Maximum UTF-8 byte length of one option key.
    pub max_key_bytes: usize,
    /// Maximum UTF-8 byte length of one option value.
    pub max_value_bytes: usize,
    /// Hard ceiling on the total SessionConfig body bytes (Destination
    /// + Mapping + creation timestamp + signature). The body ceiling is
    ///
    /// the I2CP frame ceiling.
    pub max_total_bytes: usize,
}

impl SessionConfigLimits {
    /// Returns the standard M9 limits.
    pub const fn m9() -> Self {
        Self {
            max_options: MAX_SESSION_CONFIG_OPTIONS,
            max_key_bytes: MAX_SESSION_CONFIG_KEY_BYTES,
            max_value_bytes: MAX_SESSION_CONFIG_VALUE_BYTES,
            max_total_bytes: super::frame::MAX_I2CP_BODY_BYTES,
        }
    }
}

/// Bounds-checks the SessionConfig mapping before projection.
pub fn validate_mapping_shape(
    options: &Mapping,
    limits: &SessionConfigLimits,
) -> Result<(), I2cpError> {
    let entries = options.entries();
    if entries.len() > limits.max_options {
        return Err(I2cpError::SessionConfigLimit {
            context: "options count",
            actual: entries.len(),
            maximum: limits.max_options,
        });
    }
    for entry in entries {
        if entry.key().len() > limits.max_key_bytes {
            return Err(I2cpError::SessionConfigLimit {
                context: "option key bytes",
                actual: entry.key().len(),
                maximum: limits.max_key_bytes,
            });
        }
        if entry.value().len() > limits.max_value_bytes {
            return Err(I2cpError::SessionConfigLimit {
                context: "option value bytes",
                actual: entry.value().len(),
                maximum: limits.max_value_bytes,
            });
        }
    }
    Ok(())
}

/// Parses a strictly unsigned decimal `u16` option value, rejecting
/// signed, whitespace, overflow, or non-digit input.
fn parse_u16_strict(key: &str, raw: &str) -> Result<u16, I2cpError> {
    parse_unsigned_integer(key, raw)
}

/// Parses a strictly unsigned decimal `u8` option value.
fn parse_u8_strict(key: &str, raw: &str) -> Result<u8, I2cpError> {
    parse_unsigned_integer(key, raw)
}

fn parse_unsigned_integer<T>(key: &str, raw: &str) -> Result<T, I2cpError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    if raw.is_empty() {
        return Err(I2cpError::OptionParseFailed {
            key: key.to_owned(),
            reason: "empty value",
        });
    }
    let bytes = raw.as_bytes();
    if bytes[0] == b'-' || bytes[0] == b'+' {
        return Err(I2cpError::OptionParseFailed {
            key: key.to_owned(),
            reason: "signed value rejected",
        });
    }
    if bytes.iter().any(|byte| byte.is_ascii_whitespace()) {
        return Err(I2cpError::OptionParseFailed {
            key: key.to_owned(),
            reason: "whitespace rejected",
        });
    }
    raw.parse::<T>().map_err(|_| I2cpError::OptionParseFailed {
        key: key.to_owned(),
        reason: "value out of range or non-digit characters",
    })
}

/// Parses a strictly signed decimal `i16` value (length variance can
/// be negative). Whitespace and unsigned-only prefixes are rejected.
fn parse_i16_strict(key: &str, raw: &str) -> Result<i16, I2cpError> {
    if raw.is_empty() {
        return Err(I2cpError::OptionParseFailed {
            key: key.to_owned(),
            reason: "empty value",
        });
    }
    if raw.as_bytes().iter().any(|byte| byte.is_ascii_whitespace()) {
        return Err(I2cpError::OptionParseFailed {
            key: key.to_owned(),
            reason: "whitespace rejected",
        });
    }
    raw.parse::<i16>()
        .map_err(|_| I2cpError::OptionParseFailed {
            key: key.to_owned(),
            reason: "i16 overflow or invalid digits",
        })
}

/// Parses a boolean option value. The M9 profile accepts the literal
/// strings `true` or `false`; any other value (including mixed-case
/// `True`/`FALSE`) is rejected.
fn parse_bool_strict(key: &str, raw: &str) -> Result<bool, I2cpError> {
    if raw == "true" {
        Ok(true)
    } else if raw == "false" {
        Ok(false)
    } else {
        Err(I2cpError::OptionParseFailed {
            key: key.to_owned(),
            reason: "boolean must be 'true' or 'false'",
        })
    }
}

fn push_note(
    notes: &mut Vec<OptionNote>,
    key: &str,
    disposition: OptionDisposition,
    note: Option<&'static str>,
) {
    notes.push(OptionNote {
        key: key.to_owned(),
        disposition,
        note,
    });
}

/// Projects the SessionConfig options onto a [`ProjectedPolicy`].
///
/// `defaults` provides every destination field the client did not
/// supply. Router-wide ceilings are enforced by [`DestinationConfig`]:
/// a client-supplied value that exceeds a ceiling is rejected, not
/// silently clamped.
pub fn project_options(
    options: &Mapping,
    limits: &SessionConfigLimits,
    defaults: DestinationConfig,
) -> Result<ProjectedPolicy, I2cpError> {
    validate_mapping_shape(options, limits)?;

    let mut inbound_target: Option<u16> = None;
    let mut outbound_target: Option<u16> = None;
    let mut inbound_length: Option<u8> = None;
    let mut outbound_length: Option<u8> = None;
    let mut inbound_backup: Option<u16> = None;
    let mut outbound_backup: Option<u16> = None;
    let mut inbound_allow_zero_hop = false;
    let mut outbound_allow_zero_hop = false;
    let mut dont_publish_lease_set: Option<bool> = None;
    let mut message_reliability: Option<&'static str> = None;
    let mut fast_receive: Option<bool> = None;
    let mut lease_set_type: Option<u8> = None;
    let mut lease_set_enc_type: Option<u8> = None;

    let mut notes: Vec<OptionNote> = Vec::with_capacity(options.entries().len());

    for entry in options.entries() {
        let key = entry.key();
        let value = entry.value();
        match key {
            "inbound.length" => {
                let parsed = parse_u8_strict(key, value)?;
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                inbound_length = Some(parsed);
            }
            "outbound.length" => {
                let parsed = parse_u8_strict(key, value)?;
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                outbound_length = Some(parsed);
            }
            "inbound.quantity" => {
                let parsed = parse_u16_strict(key, value)?;
                if parsed == 0 {
                    return Err(I2cpError::OptionRejected {
                        key: key.to_owned(),
                        reason: "inbound.quantity must be nonzero",
                    });
                }
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                inbound_target = Some(parsed);
            }
            "outbound.quantity" => {
                let parsed = parse_u16_strict(key, value)?;
                if parsed == 0 {
                    return Err(I2cpError::OptionRejected {
                        key: key.to_owned(),
                        reason: "outbound.quantity must be nonzero",
                    });
                }
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                outbound_target = Some(parsed);
            }
            "inbound.backupQuantity" => {
                let parsed = parse_u16_strict(key, value)?;
                push_note(
                    &mut notes,
                    key,
                    OptionDisposition::Applied,
                    Some("backup quantity recorded; target drives pool sizing"),
                );
                inbound_backup = Some(parsed);
            }
            "outbound.backupQuantity" => {
                let parsed = parse_u16_strict(key, value)?;
                push_note(
                    &mut notes,
                    key,
                    OptionDisposition::Applied,
                    Some("backup quantity recorded; target drives pool sizing"),
                );
                outbound_backup = Some(parsed);
            }
            "inbound.lengthVariance" => {
                let parsed = parse_i16_strict(key, value)?;
                push_note(
                    &mut notes,
                    key,
                    OptionDisposition::Applied,
                    Some("length variance recorded; target length drives pool sizing"),
                );
                let _ = parsed;
            }
            "outbound.lengthVariance" => {
                let parsed = parse_i16_strict(key, value)?;
                push_note(
                    &mut notes,
                    key,
                    OptionDisposition::Applied,
                    Some("length variance recorded; target length drives pool sizing"),
                );
                let _ = parsed;
            }
            "inbound.allowZeroHop" => {
                let parsed = parse_bool_strict(key, value)?;
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                inbound_allow_zero_hop = parsed;
            }
            "outbound.allowZeroHop" => {
                let parsed = parse_bool_strict(key, value)?;
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                outbound_allow_zero_hop = parsed;
            }
            "i2cp.dontPublishLeaseSet" => {
                let parsed = parse_bool_strict(key, value)?;
                push_note(
                    &mut notes,
                    key,
                    OptionDisposition::Applied,
                    Some("unpublished LS2 still installed locally; publication suppressed"),
                );
                dont_publish_lease_set = Some(parsed);
            }
            "i2cp.messageReliability" => {
                if value.eq_ignore_ascii_case("besteffort") {
                    push_note(&mut notes, key, OptionDisposition::Applied, None);
                    message_reliability = Some(M9_MESSAGE_RELIABILITY_BEST_EFFORT);
                } else if value.eq_ignore_ascii_case("guaranteed") {
                    return Err(I2cpError::OptionRejected {
                        key: key.to_owned(),
                        reason: "guaranteed reliability not delivered by M9",
                    });
                } else if value.eq_ignore_ascii_case("none") {
                    // The Java I2P 2.13.0 reference and the go-i2cp
                    // reference both ship "none" by default (see
                    // I2PSessionImpl.java + client_connect.go).
                    // Mapping it to the M9 best-effort posture keeps
                    // the negotiation fail-open for every unmodified
                    // client while still recording the documented
                    // ignored note.
                    push_note(
                        &mut notes,
                        key,
                        OptionDisposition::Ignored,
                        Some("treated as best-effort"),
                    );
                    message_reliability = Some(M9_MESSAGE_RELIABILITY_BEST_EFFORT);
                } else {
                    return Err(I2cpError::OptionRejected {
                        key: key.to_owned(),
                        reason: "unknown messageReliability value",
                    });
                }
            }
            "i2cp.fastReceive" => {
                let parsed = parse_bool_strict(key, value)?;
                if !parsed {
                    return Err(I2cpError::OptionRejected {
                        key: key.to_owned(),
                        reason: "non-fast receive is not implemented in M9",
                    });
                }
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                fast_receive = Some(parsed);
            }
            "i2cp.leaseSetType" => {
                let parsed = parse_u8_strict(key, value)?;
                if parsed != M9_LEASE_SET_TYPE {
                    return Err(I2cpError::OptionRejected {
                        key: key.to_owned(),
                        reason: "M9 supports Standard LeaseSet2 only (type 3)",
                    });
                }
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                lease_set_type = Some(parsed);
            }
            "i2cp.leaseSetEncType" => {
                let parsed = parse_u8_strict(key, value)?;
                if parsed != M9_LEASE_SET_ENC_TYPE {
                    return Err(I2cpError::OptionRejected {
                        key: key.to_owned(),
                        reason: "M9 supports X25519 lease-set encryption (type 4) only",
                    });
                }
                push_note(&mut notes, key, OptionDisposition::Applied, None);
                lease_set_enc_type = Some(parsed);
            }
            "i2cp.outbound.tunnel.switch" => {
                push_note(
                    &mut notes,
                    key,
                    OptionDisposition::Ignored,
                    Some("Proposal 171 draft; spec-defined ignore"),
                );
            }
            _ => {
                push_note(
                    &mut notes,
                    key,
                    OptionDisposition::Unknown,
                    Some("unknown option key"),
                );
            }
        }
    }

    // Plan 172 §8: explicit local zero-hop profile. Remote
    // one-plus-hop behavior is unchanged; the remote minimum is never
    // lowered to make length 0 pass as remote.
    let inbound_is_zero = inbound_length == Some(0);
    let outbound_is_zero = outbound_length == Some(0);
    if inbound_is_zero || outbound_is_zero {
        // Mixed local/remote modes are unsupported: reject explicitly
        // rather than silently rewriting.
        if inbound_is_zero != outbound_is_zero {
            return Err(I2cpError::OptionRejected {
                key: "inbound.length".to_owned(),
                reason: "mixed zero-hop/remote modes not supported in M9",
            });
        }
        // allowZeroHop=false + length 0 must reject.
        if !inbound_allow_zero_hop || !outbound_allow_zero_hop {
            return Err(I2cpError::OptionRejected {
                key: "inbound.allowZeroHop".to_owned(),
                reason: "length 0 requires inbound.allowZeroHop=true and outbound.allowZeroHop=true",
            });
        }
        // Quantity must be exactly 1 when explicitly supplied.
        if let Some(value) = inbound_target
            && value != 1
        {
            return Err(I2cpError::OptionRejected {
                key: "inbound.quantity".to_owned(),
                reason: "zero-hop profile requires inbound.quantity=1",
            });
        }
        if let Some(value) = outbound_target
            && value != 1
        {
            return Err(I2cpError::OptionRejected {
                key: "outbound.quantity".to_owned(),
                reason: "zero-hop profile requires outbound.quantity=1",
            });
        }
        // Backup quantity must be 0 when explicitly supplied.
        if let Some(value) = inbound_backup
            && value != 0
        {
            return Err(I2cpError::OptionRejected {
                key: "inbound.backupQuantity".to_owned(),
                reason: "zero-hop profile requires inbound.backupQuantity=0",
            });
        }
        if let Some(value) = outbound_backup
            && value != 0
        {
            return Err(I2cpError::OptionRejected {
                key: "outbound.backupQuantity".to_owned(),
                reason: "zero-hop profile requires outbound.backupQuantity=0",
            });
        }
        // dontPublishLeaseSet=false would require public NetDB
        // publication, which the M9 localhost profile does not do.
        if dont_publish_lease_set == Some(false) {
            return Err(I2cpError::OptionRejected {
                key: "i2cp.dontPublishLeaseSet".to_owned(),
                reason: "M9 localhost profile requires i2cp.dontPublishLeaseSet=true",
            });
        }
        let destination_config = DestinationConfig::try_new(
            1,
            1,
            1,
            defaults.length_hops(),
            defaults.tunnel_lifetime_seconds(),
            defaults.build_concurrency(),
            defaults.failure_threshold(),
            defaults.max_pending_messages(),
            defaults.max_pending_bytes(),
            defaults.lease_publication_margin_seconds(),
            defaults.lease_rotation_margin_seconds(),
        )
        .map_err(projection_error_to_i2cp)?;
        return Ok(ProjectedPolicy {
            destination_config,
            tunnel_mode: i2pr_client::DestinationTunnelMode::LocalZeroHop,
            notes,
            lease_set_type: lease_set_type.unwrap_or(M9_LEASE_SET_TYPE),
            lease_set_enc_type: lease_set_enc_type.unwrap_or(M9_LEASE_SET_ENC_TYPE),
            message_reliability: message_reliability.unwrap_or(M9_MESSAGE_RELIABILITY_BEST_EFFORT),
            fast_receive: fast_receive.unwrap_or(M9_FAST_RECEIVE_DEFAULT),
            inbound_allow_zero_hop: true,
            outbound_allow_zero_hop: true,
            dont_publish_lease_set: dont_publish_lease_set.unwrap_or(true),
        });
    }
    // Remote path: any explicit allowZeroHop=true without length 0 is
    // an unsupported fallback request; reject explicitly rather than
    // silently treating it as remote.
    if inbound_allow_zero_hop || outbound_allow_zero_hop {
        return Err(I2cpError::OptionRejected {
            key: "inbound.allowZeroHop".to_owned(),
            reason: "allowZeroHop=true requires inbound.length=0 and outbound.length=0 in M9",
        });
    }
    // Non-zero lengths flow through the remote policy. Length 0 was
    // handled above; here length is either absent or >= 1.
    if let Some(value) = inbound_length
        && value == 0
    {
        return Err(I2cpError::OptionRejected {
            key: "inbound.length".to_owned(),
            reason: "length 0 requires allowZeroHop",
        });
    }
    if let Some(value) = outbound_length
        && value == 0
    {
        return Err(I2cpError::OptionRejected {
            key: "outbound.length".to_owned(),
            reason: "length 0 requires allowZeroHop",
        });
    }

    // Backup quantity/variance may not silently replace target quantity/
    // length; only the explicit `quantity`/`length` keys drive pool
    // sizing. Backup/variance are recorded as notes and dropped here.
    let _ = (inbound_backup, outbound_backup);

    let inbound_target_value = inbound_target.unwrap_or_else(|| defaults.inbound_target());
    let outbound_target_value = outbound_target.unwrap_or_else(|| defaults.outbound_target());
    let minimum_usable_inbound = inbound_target_value.min(defaults.minimum_usable_inbound());
    let length_hops = inbound_length
        .or(outbound_length)
        .unwrap_or_else(|| defaults.length_hops());
    let lifetime_seconds = defaults.tunnel_lifetime_seconds();
    let build_concurrency = defaults.build_concurrency();
    let failure_threshold = defaults.failure_threshold();
    let max_pending_messages = defaults.max_pending_messages();
    let max_pending_bytes = defaults.max_pending_bytes();
    let lease_publication_margin_seconds = defaults.lease_publication_margin_seconds();
    let lease_rotation_margin_seconds = defaults.lease_rotation_margin_seconds();

    let destination_config = DestinationConfig::try_new(
        inbound_target_value,
        outbound_target_value,
        minimum_usable_inbound,
        length_hops,
        lifetime_seconds,
        build_concurrency,
        failure_threshold,
        max_pending_messages,
        max_pending_bytes,
        lease_publication_margin_seconds,
        lease_rotation_margin_seconds,
    )
    .map_err(projection_error_to_i2cp)?;

    let _ = message_reliability;
    let _ = fast_receive;
    let _ = lease_set_type;
    let _ = lease_set_enc_type;

    Ok(ProjectedPolicy {
        destination_config,
        tunnel_mode: i2pr_client::DestinationTunnelMode::Remote { length_hops },
        notes,
        lease_set_type: lease_set_type.unwrap_or(M9_LEASE_SET_TYPE),
        lease_set_enc_type: lease_set_enc_type.unwrap_or(M9_LEASE_SET_ENC_TYPE),
        message_reliability: message_reliability.unwrap_or(M9_MESSAGE_RELIABILITY_BEST_EFFORT),
        fast_receive: fast_receive.unwrap_or(M9_FAST_RECEIVE_DEFAULT),
        inbound_allow_zero_hop: false,
        outbound_allow_zero_hop: false,
        dont_publish_lease_set: dont_publish_lease_set.unwrap_or(false),
    })
}

/// Remaps a [`DestinationConfigError`] into an I2CP option-rejection
/// error tagged with the configuration policy that failed.
fn projection_error_to_i2cp(error: DestinationConfigError) -> I2cpError {
    let key = match &error {
        DestinationConfigError::ZeroInboundTarget
        | DestinationConfigError::InboundExceedsMaximum { .. }
        | DestinationConfigError::ZeroMinimumUsableInbound
        | DestinationConfigError::MinimumUsableExceedsTarget { .. } => "inbound.quantity",
        DestinationConfigError::ZeroOutboundTarget
        | DestinationConfigError::OutboundExceedsMaximum { .. } => "outbound.quantity",
        DestinationConfigError::ZeroBuildConcurrency
        | DestinationConfigError::BuildConcurrencyExceedsMaximum { .. } => {
            "router.buildConcurrency"
        }
        DestinationConfigError::ZeroFailureThreshold
        | DestinationConfigError::FailureThresholdExceedsMaximum { .. } => {
            "router.failureThreshold"
        }
        DestinationConfigError::ZeroPendingMessages
        | DestinationConfigError::PendingMessagesExceedsMaximum { .. } => "router.pendingMessages",
        DestinationConfigError::ZeroPendingBytes
        | DestinationConfigError::PendingBytesExceedsMaximum { .. } => "router.pendingBytes",
        DestinationConfigError::PublicationMarginExceedsMaximum { .. }
        | DestinationConfigError::RotationMarginExceedsMaximum { .. }
        | DestinationConfigError::PublicationMarginExceedsLifetime { .. } => "router.leaseMargin",
        DestinationConfigError::ZeroMaxDestinations
        | DestinationConfigError::MaxDestinationsExceedsMaximum { .. }
        | DestinationConfigError::ZeroCommandQueueDepth
        | DestinationConfigError::CommandQueueDepthExceedsMaximum { .. } => {
            "router.registryCapacity"
        }
        _ => "router.policy",
    };
    I2cpError::OptionRejected {
        key: key.to_owned(),
        reason: "destination policy rejected by router-wide ceiling",
    }
}

/// Reconfiguration classification for the Plan 165 reconfigure model.
///
/// Every known option is tagged with one of the four canonical classes.
/// A reconfigure request must be fully classified and validated before
/// any mutation is applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconfigurationClass {
    /// Mutation that requires rebuilding destination tunnels.
    MutableWithRebuild,
    /// Mutation that takes effect on the next event loop tick.
    MutableImmediate,
    /// Cannot be changed after the session is created.
    ImmutableAfterCreate,
    /// Not supported in M9.
    Unsupported,
}

/// Returns the canonical reconfiguration class for a known option key.
pub fn reconfiguration_class(key: &str) -> ReconfigurationClass {
    match key {
        "inbound.quantity" | "outbound.quantity" | "inbound.length" | "outbound.length" => {
            ReconfigurationClass::MutableWithRebuild
        }
        "inbound.backupQuantity"
        | "outbound.backupQuantity"
        | "inbound.lengthVariance"
        | "outbound.lengthVariance" => ReconfigurationClass::MutableImmediate,
        "i2cp.leaseSetType" | "i2cp.leaseSetEncType" => ReconfigurationClass::ImmutableAfterCreate,
        "i2cp.messageReliability" | "i2cp.fastReceive" => ReconfigurationClass::Unsupported,
        _ => ReconfigurationClass::Unsupported,
    }
}

/// Computes the diff between two option sets and classifies each new
/// key for the reconfiguration pass. Removed keys are classified as
/// `ImmutableAfterCreate` (clients cannot silently drop options).
pub fn classify_reconfigure_diff(
    previous: &Mapping,
    next: &Mapping,
) -> Vec<(String, ReconfigurationClass)> {
    let mut classifications = Vec::new();
    let previous_keys: Vec<&str> = previous.entries().iter().map(|entry| entry.key()).collect();
    let next_keys: Vec<&str> = next.entries().iter().map(|entry| entry.key()).collect();

    for key in &next_keys {
        let prev = previous.get(key);
        let curr = next.get(key);
        if prev != curr {
            classifications.push(((*key).to_owned(), reconfiguration_class(key)));
        }
    }
    for key in &previous_keys {
        if next.get(key).is_none() {
            classifications.push((
                (*key).to_owned(),
                ReconfigurationClass::ImmutableAfterCreate,
            ));
        }
    }
    classifications
}

/// Verifies a single proposed reconfigure classification set is
/// all-or-nothing. Any `Unsupported` or `ImmutableAfterCreate`
/// classification aborts the reconfigure.
pub fn validate_reconfigure_classifications(
    classifications: &[(String, ReconfigurationClass)],
) -> Result<(), I2cpError> {
    for (_key, class) in classifications {
        match class {
            ReconfigurationClass::MutableWithRebuild | ReconfigurationClass::MutableImmediate => {}
            ReconfigurationClass::ImmutableAfterCreate => {
                return Err(I2cpError::ReconfigureRejected {
                    context: "immutable option cannot be reconfigured",
                });
            }
            ReconfigurationClass::Unsupported => {
                return Err(I2cpError::ReconfigureRejected {
                    context: "unsupported option in reconfigure request",
                });
            }
        }
    }
    Ok(())
}

/// Returns the router-default [`RegistryConfig`] used when no
/// per-router configuration is provided. Plan 167 may override this
/// from `[i2cp]` configuration.
pub fn default_registry_config() -> RegistryConfig {
    RegistryConfig::default()
}

/// Returns the explorer [`TunnelLifetime`] used as the Plan 165
/// destination lifetime default.
pub fn default_tunnel_lifetime() -> Result<TunnelLifetime, i2pr_tunnel::TunnelLifetimeError> {
    TunnelLifetime::from_seconds(TunnelLifetime::DEFAULT_EXPLORATORY_SECONDS)
}

/// Re-exposes the plan-level ceilings used during the option projection
/// so tests and the Plan 167 daemon can verify the same constants.
pub mod ceilings {
    pub use super::MAX_SESSION_CONFIG_KEY_BYTES;
    pub use super::MAX_SESSION_CONFIG_OPTIONS;
    pub use super::MAX_SESSION_CONFIG_VALUE_BYTES;
    pub use i2pr_client::{
        MAX_DESTINATION_BUILD_CONCURRENCY, MAX_DESTINATION_FAILURE_THRESHOLD,
        MAX_DESTINATION_INBOUND, MAX_DESTINATION_OUTBOUND,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_client::{
        DestinationConfig, MAX_DESTINATION_INBOUND, MAX_PENDING_DESTINATION_BYTES,
        MAX_PENDING_DESTINATION_MESSAGES,
    };

    fn mapping_of(pairs: &[(&str, &str)]) -> Mapping {
        Mapping::from_entries(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        )
        .expect("mapping")
    }

    fn limits() -> SessionConfigLimits {
        SessionConfigLimits::m9()
    }

    #[test]
    fn defaults_remain_when_no_options_supplied() {
        let mapping = Mapping::empty();
        let policy =
            project_options(&mapping, &limits(), DestinationConfig::balanced()).expect("defaults");
        assert_eq!(policy.destination_config, DestinationConfig::balanced());
        assert!(policy.notes.is_empty());
        assert_eq!(policy.lease_set_type, M9_LEASE_SET_TYPE);
        assert_eq!(policy.lease_set_enc_type, M9_LEASE_SET_ENC_TYPE);
        assert_eq!(
            policy.message_reliability,
            M9_MESSAGE_RELIABILITY_BEST_EFFORT
        );
        assert!(policy.fast_receive);
        assert!(!policy.inbound_allow_zero_hop);
        assert!(!policy.outbound_allow_zero_hop);
    }

    #[test]
    fn supported_keys_project_onto_destination_config() {
        let mapping = mapping_of(&[
            ("inbound.length", "3"),
            ("outbound.length", "2"),
            ("inbound.quantity", "3"),
            ("outbound.quantity", "2"),
            ("inbound.backupQuantity", "1"),
            ("outbound.backupQuantity", "1"),
            ("inbound.lengthVariance", "0"),
            ("outbound.lengthVariance", "0"),
            ("i2cp.messageReliability", "BestEffort"),
            ("i2cp.fastReceive", "true"),
            ("i2cp.leaseSetType", "3"),
            ("i2cp.leaseSetEncType", "4"),
        ]);
        let policy =
            project_options(&mapping, &limits(), DestinationConfig::balanced()).expect("project");
        assert_eq!(policy.destination_config.inbound_target(), 3);
        assert_eq!(policy.destination_config.outbound_target(), 2);
        assert_eq!(policy.destination_config.length_hops(), 3);
        assert!(policy.fast_receive);
        assert_eq!(policy.lease_set_type, 3);
        assert_eq!(policy.lease_set_enc_type, 4);
        assert!(policy.notes.iter().any(|n| n.key == "inbound.length"));
        assert!(policy.notes.iter().any(|n| n.key == "i2cp.leaseSetType"));
    }

    #[test]
    fn unknown_keys_recorded_without_altering_policy() {
        let mapping = mapping_of(&[("foo.bar", "baz")]);
        let policy = project_options(&mapping, &limits(), DestinationConfig::balanced())
            .expect("unknown keys do not fail");
        assert_eq!(policy.destination_config, DestinationConfig::balanced());
        assert_eq!(policy.notes.len(), 1);
        assert_eq!(policy.notes[0].key, "foo.bar");
        assert_eq!(policy.notes[0].disposition, OptionDisposition::Unknown);
    }

    #[test]
    fn zero_quantity_rejected_and_zero_hop_rejected() {
        let mapping = mapping_of(&[("inbound.quantity", "0")]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
        let mapping = mapping_of(&[("outbound.quantity", "0")]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
        let mapping = mapping_of(&[("inbound.allowZeroHop", "true")]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
        let mapping = mapping_of(&[("outbound.allowZeroHop", "true")]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
    }

    #[test]
    fn signed_and_overflow_values_rejected() {
        for raw in ["-1", " 1", "+1", "999999", ""] {
            let mapping = mapping_of(&[("inbound.length", raw)]);
            assert!(
                matches!(
                    project_options(&mapping, &limits(), DestinationConfig::balanced()),
                    Err(I2cpError::OptionParseFailed { .. })
                ),
                "value {raw:?}"
            );
        }
    }

    #[test]
    fn unsupported_lease_set_and_pq_encryption_rejected() {
        let mapping = mapping_of(&[("i2cp.leaseSetType", "1")]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
        let mapping = mapping_of(&[("i2cp.leaseSetType", "5")]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
        for enc in ["0", "1", "2", "3", "5", "6", "7"] {
            let mapping = mapping_of(&[("i2cp.leaseSetEncType", enc)]);
            assert!(
                matches!(
                    project_options(&mapping, &limits(), DestinationConfig::balanced()),
                    Err(I2cpError::OptionRejected { .. })
                ),
                "enc {enc}"
            );
        }
    }

    #[test]
    fn guaranteed_reliability_rejected_and_fast_receive_off_rejected() {
        let mapping = mapping_of(&[("i2cp.messageReliability", "Guaranteed")]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
        let mapping = mapping_of(&[("i2cp.fastReceive", "false")]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
    }

    #[test]
    fn ceiling_exceeding_quantity_rejected() {
        let too_many = format!("{}", u32::from(MAX_DESTINATION_INBOUND) + 1);
        let mapping = mapping_of(&[("inbound.quantity", &too_many)]);
        assert!(matches!(
            project_options(&mapping, &limits(), DestinationConfig::balanced()),
            Err(I2cpError::OptionRejected { .. })
        ));
    }

    #[test]
    fn proposal_171_key_is_ignored_not_rejected() {
        let mapping = mapping_of(&[("i2cp.outbound.tunnel.switch", "yes")]);
        let policy =
            project_options(&mapping, &limits(), DestinationConfig::balanced()).expect("ignored");
        assert_eq!(policy.destination_config, DestinationConfig::balanced());
        assert_eq!(policy.notes.len(), 1);
        assert_eq!(policy.notes[0].disposition, OptionDisposition::Ignored);
    }

    #[test]
    fn mapping_shape_rejects_too_many_entries_and_oversize_key() {
        let mut pairs: Vec<(String, String)> = (0..MAX_SESSION_CONFIG_OPTIONS + 1)
            .map(|index| (format!("k{index}"), "1".to_owned()))
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let mapping = Mapping::from_entries(pairs).expect("mapping");
        assert!(matches!(
            validate_mapping_shape(&mapping, &limits()),
            Err(I2cpError::SessionConfigLimit { .. })
        ));

        let long_key = "x".repeat(MAX_SESSION_CONFIG_KEY_BYTES + 1);
        let mapping = mapping_of(&[(&long_key, "1")]);
        assert!(matches!(
            validate_mapping_shape(&mapping, &limits()),
            Err(I2cpError::SessionConfigLimit { .. })
        ));

        let long_value = "x".repeat(MAX_SESSION_CONFIG_VALUE_BYTES + 1);
        let mapping = mapping_of(&[("inbound.length", &long_value)]);
        assert!(matches!(
            validate_mapping_shape(&mapping, &limits()),
            Err(I2cpError::SessionConfigLimit { .. })
        ));
    }

    #[test]
    fn reconfiguration_classes_match_spec() {
        assert_eq!(
            reconfiguration_class("inbound.quantity"),
            ReconfigurationClass::MutableWithRebuild
        );
        assert_eq!(
            reconfiguration_class("inbound.backupQuantity"),
            ReconfigurationClass::MutableImmediate
        );
        assert_eq!(
            reconfiguration_class("i2cp.leaseSetType"),
            ReconfigurationClass::ImmutableAfterCreate
        );
        assert_eq!(
            reconfiguration_class("i2cp.fastReceive"),
            ReconfigurationClass::Unsupported
        );
        assert_eq!(
            reconfiguration_class("not.a.known.key"),
            ReconfigurationClass::Unsupported
        );
    }

    #[test]
    fn reconfigure_diff_classifies_added_changed_removed() {
        let previous = mapping_of(&[("inbound.quantity", "2"), ("inbound.length", "2")]);
        let next = mapping_of(&[("inbound.quantity", "3"), ("i2cp.fastReceive", "true")]);
        let classes = classify_reconfigure_diff(&previous, &next);
        assert!(classes.iter().any(
            |(k, c)| k == "inbound.quantity" && *c == ReconfigurationClass::MutableWithRebuild
        ));
        assert!(classes.iter().any(|(k, c)| {
            k == "inbound.length" && *c == ReconfigurationClass::ImmutableAfterCreate
        }));
        assert!(
            classes.iter().any(|(k, c)| {
                k == "i2cp.fastReceive" && *c == ReconfigurationClass::Unsupported
            })
        );
    }

    #[test]
    fn reconfigure_all_or_nothing_rejects_unsupported_and_immutable() {
        let unsupported = vec![("foo".to_owned(), ReconfigurationClass::Unsupported)];
        assert!(validate_reconfigure_classifications(&unsupported).is_err());
        let immutable = vec![(
            "inbound.length".to_owned(),
            ReconfigurationClass::ImmutableAfterCreate,
        )];
        assert!(validate_reconfigure_classifications(&immutable).is_err());
        let allowed = vec![
            (
                "inbound.quantity".to_owned(),
                ReconfigurationClass::MutableWithRebuild,
            ),
            (
                "inbound.backupQuantity".to_owned(),
                ReconfigurationClass::MutableImmediate,
            ),
        ];
        assert!(validate_reconfigure_classifications(&allowed).is_ok());
    }

    #[test]
    fn default_tunnel_lifetime_is_within_ceiling() {
        let lifetime = default_tunnel_lifetime().expect("default lifetime is valid");
        assert_eq!(
            lifetime.seconds(),
            TunnelLifetime::DEFAULT_EXPLORATORY_SECONDS
        );
    }

    #[test]
    fn default_registry_config_is_within_ceiling() {
        let registry = default_registry_config();
        assert!(registry.max_destinations() > 0);
        assert!(registry.max_aggregate_command_queue_depth() > 0);
    }

    #[test]
    fn plan_constants_are_documented() {
        const _: () = {
            assert!(MAX_DESTINATION_INBOUND >= 1);
            assert!(MAX_PENDING_DESTINATION_MESSAGES >= 1);
            assert!(MAX_PENDING_DESTINATION_BYTES >= 1);
        };
    }

    #[test]
    fn low_u8_length_value_is_accepted() {
        let mapping = mapping_of(&[("inbound.length", "1")]);
        let policy =
            project_options(&mapping, &limits(), DestinationConfig::balanced()).expect("length=1");
        assert_eq!(policy.destination_config.length_hops(), 1);
    }

    #[test]
    fn boolean_normalization_is_strict_lowercase() {
        let mapping = mapping_of(&[("i2cp.fastReceive", "true")]);
        let policy =
            project_options(&mapping, &limits(), DestinationConfig::balanced()).expect("true");
        assert!(policy.fast_receive);
        for raw in ["TRUE", "True", "false"] {
            let mapping = mapping_of(&[("i2cp.fastReceive", raw)]);
            assert!(
                project_options(&mapping, &limits(), DestinationConfig::balanced()).is_err(),
                "{raw}"
            );
        }
    }

    #[test]
    fn supported_inbound_quantity_is_capped_by_router_ceiling() {
        // Asking for the maximum acceptable count must succeed.
        let at_cap = format!("{}", MAX_DESTINATION_INBOUND);
        let mapping = mapping_of(&[("inbound.quantity", &at_cap)]);
        let policy = project_options(&mapping, &limits(), DestinationConfig::balanced())
            .expect("at-cap count");
        assert_eq!(
            policy.destination_config.inbound_target(),
            MAX_DESTINATION_INBOUND
        );
    }
}
