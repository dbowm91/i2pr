//! Plan 289 TunnelManager control state and existing service-runtime adapter.
//!
//! # Ownership (Plan 337 — one manager, as Plan 289 required)
//!
//! This module owns "the one existing M10 [`ServiceTunnelManager`]" and never
//! creates a second service/destination/tunnel runtime. That claim was false
//! from Plan 289 until Plan 337: `for_config` used to build a **separate**
//! manager over an empty `ServiceTunnelSet` while `ServiceProduct::new` built
//! the one `publish_service_ls2_for_service` is handed, and no production call
//! site installed a router delivery capability on the control-owned manager —
//! so a control-created server published no LeaseSet2 at all. Plan 334 recorded
//! that as doc-versus-source drift (`plans/closure/i2pcontrol-proposal-170/
//! 334-status.md`) rather than rewriting Plan 289's invariant.
//!
//! Plan 337 made the source match. The composition root now builds the manager
//! **once** and injects it into both owners, so a control-created runtime is the
//! same runtime the product layer delivers and publishes through. Two
//! consequences are load-bearing and must not be undone:
//!
//! - `candidate_set` describes the **whole** manager surface, carrying
//!   startup-owned specs through verbatim and adding control-owned running
//!   specs. `reconcile` is a whole-set transactional replace, so a
//!   control-only candidate would silently remove every startup-owned runtime.
//! - `verify_agreement` scopes its "no extra runtime" rule to names this
//!   coordinator does not own, because the manager is now shared.
//!
//! The rest of this module implements durable administrative ownership over a
//! control-owned service runtime. Every lifecycle action validates a candidate
//! control-owned definition, stages a versioned persistence generation,
//! reconciles the shared manager, publishes the durable generation, and returns
//! success only when durable intent and the authoritative runtime generation
//! agree.
//!
//! Ownership model: `StartupOwned` definitions come from daemon TOML and
//! are inspectable but never mutated here; `ControlOwned` definitions
//! originate from TunnelManager and persist beneath the dedicated
//! `i2pcontrol/tunnels/` state root. Name collisions across classes fail
//! closed, and control state never rewrites `router.toml` or ordinary
//! service-tunnel files.
//!
//! Supported 289 option subset (each with a real effect; every other
//! supplied option fails explicitly as unsupported, never accepted
//! inertly). Key spellings are the frozen Proposal universe from
//! `i2pr-i2pcontrol::TUNNEL_OPTIONS`:
//! - `target_destination` — client kinds: remote [`DestinationRef`];
//! - `target_host` + `target_port` — server kinds: loopback `ip:port`
//!   target, both required (no silent default for where tunneled
//!   traffic exits; `unix:` hosts rejected as not-yet-supported);
//! - `listen_host` — client kinds: loopback IP (default `127.0.0.1`);
//! - `listen_port` — client kinds: `0..=65535` (default `0`, ephemeral);
//! - `start_on_load` — `true`/`false` (default `true`);
//! - `max_streams` — mapped onto the per-service connection ceiling
//!   `1..=128` (default `16`).
//!
//! Profile options for `httpclient`/`socks`/`ircclient` use the reviewed
//! M10 defaults; profile tuning belongs to Plans 290/292.
//!
//! Secret-classified options are all unsupported in Plan 289: they are
//! rejected before storage, so no secret ever reaches the generation
//! files, responses, errors, logs, or `Debug` output.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::encrypted_target_credential::{
    EncryptedTargetCredential, EncryptedTargetCredentialError, SealedEncryptedTargetCredential,
};
use i2pr_i2pcontrol::proposal_leaseset_mode::{LeaseSetClientAuthScheme, LeaseSetSecurityPlan};
use i2pr_i2pcontrol::tunnel::validate_tunnel_name;
use i2pr_i2pcontrol::tunnel_matrix::{CellDisposition, disposition_for};
use i2pr_i2pcontrol::{MAX_OPTION_VALUE_LEN, TunnelAction, TunnelManagerRequest, TunnelType};
use i2pr_service_tunnels::outbound_secret::{
    NoOutboundSecrets, OutboundSecret, RouterSecretOwner, validate_els2_credential_form,
};
use i2pr_service_tunnels::{
    DEFAULT_IDLE_TIMEOUT_MS, DestinationGroupId, DestinationPolicy, DestinationRef,
    IdleSweepAction, LocalListenerSpec, MAX_EFFECTIVE_DIRECTION_TUNNELS, MAX_IDLE_TIMEOUT_MS,
    MAX_SERVICE_TUNNELS, MAX_TUNNEL_BACKUP_QUANTITY, MAX_TUNNEL_LENGTH_HOPS,
    MAX_TUNNEL_LENGTH_VARIANCE, MAX_TUNNEL_QUANTITY, MIN_IDLE_TIMEOUT_MS, PROXY_AUTH_REALM_CONNECT,
    PROXY_AUTH_REALM_HTTP, PROXY_AUTH_REALM_SOCKS, PROXY_VERIFIER_MARKER, ProxyCredentials,
    ServerAccessPolicy, ServerTarget, ServiceTunnelId, ServiceTunnelKind, ServiceTunnelSet,
    ServiceTunnelSpec, TunnelShaping,
};

use crate::service_tunnels::ServiceTunnelManager;

/// Schema version of the control generation files.
pub const CONTROL_SCHEMA_VERSION: u8 = 1;

/// State root beneath the router data directory.
pub const CONTROL_STATE_SUBDIR: &str = "i2pcontrol";
/// Tunnel control generations beneath the state root.
pub const CONTROL_TUNNELS_SUBDIR: &str = "tunnels";
/// Pointer file naming the published generation.
pub const CONTROL_POINTER_FILE: &str = "current.json";
/// Maximum bytes of one generation file.
pub const MAX_GENERATION_BYTES: usize = 1_048_576;
/// Maximum UTF-8 bytes in one confined access-filter file.
pub const MAX_FILTER_FILE_BYTES: usize = 4_096;
const FILTER_DIRECTORY: &str = "filters";
/// Bounded drain deadline for control reconciles.
///
/// The control-plane drain policy is immediate release: administrative
/// stop/restart/delete frees listeners at once so ports recycle
/// deterministically (stop-then-start and rename sequences rebind the
/// same socket without waiting out a grace window). In-flight
/// connections observe cancellation promptly. This is the bounded
/// policy the transaction coordinator applies; it differs
/// deliberately from the product default, which favors connection
/// grace over port reuse.
pub const CONTROL_DRAIN_DEADLINE: Duration = Duration::ZERO;
/// Hard deadline for reconcile-back after a publish failure.
pub const CONTROL_ROLLBACK_DEADLINE: Duration = Duration::from_secs(5);
/// Plan 289 supported option subset (real effect each; see module docs).
pub const SUPPORTED_289_OPTIONS: [&str; 7] = [
    "target_destination",
    "target_host",
    "target_port",
    "listen_host",
    "listen_port",
    "start_on_load",
    "max_streams",
];
/// Plan 291 Streamr option keys with real effects. `remote_udp_host`
/// stays unsupported until Plan 292 assigns it meaning (never
/// accepted inertly).
pub const SUPPORTED_291_OPTIONS: [&str; 7] = [
    "local_udp_host",
    "local_udp_port",
    "target_i2p_port",
    "streamr_subscribe_interval",
    "streamr_expiry",
    "streamr_max_subscribers",
    "streamr_payload_limit",
];
/// Plan 292 option keys: shaping, lifecycle, proxy authentication,
/// access policy, presentation policy, and the Streamr sink redirect.
/// Membership admits a key to definitions; each key additionally
/// needs a `build_control_spec` owner arm (the `other` arm still
/// rejects ownerless keys before any allocation).
pub const SUPPORTED_292_OPTIONS: [&str; 21] = [
    "tunnel_length",
    "tunnel_quantity",
    "inbound_length",
    "outbound_length",
    "inbound_quantity",
    "outbound_quantity",
    "profile",
    "interactive",
    "idle_timeout",
    "close_on_idle",
    "new_dest_on_idle",
    "reduce_on_idle",
    "proxy_username",
    "proxy_password",
    "access_list",
    "white_list",
    "black_list",
    "address_helper",
    "jump_list",
    "unique_local_address",
    "remote_udp_host",
];
/// Plan 296 residual option keys with real owners: pool
/// backup/variance shaping, server multihoming target selection,
/// and garlic reply bundling. Membership admits a key to
/// definitions; each key additionally needs a `build_control_spec`
/// owner arm (the `other` arm still rejects ownerless keys before
/// any allocation).
pub const SUPPORTED_296_OPTIONS: [&str; 4] = [
    "tunnel_backup_quantity",
    "tunnel_variance",
    "multihoming",
    "reply_bundling",
];
/// Plan 297 option key with a real owner: server `use_ssl` rides
/// the daemon's explicit TLS identity/trust policy. Membership
/// admits the key to definitions; the `build_control_spec` owner
/// arm below consumes it (the `other` arm still rejects ownerless
/// keys before any allocation).
pub const SUPPORTED_297_OPTIONS: [&str; 1] = ["use_ssl"];
/// Plan 323 TunnelManager metadata and runtime options with typed owners.
pub const SUPPORTED_323_OPTIONS: [&str; 33] = [
    "description",
    "proxy_auth",
    "allow_user_agent",
    "allow_referer",
    "allow_accept",
    "close_time",
    "reduce_time",
    "reduce_count",
    "spoofed_host",
    "block_referers",
    "allow_internal_ssl",
    "block_user_agents",
    "user_agents",
    "block_access_in_proxies",
    "shared",
    "persistent_client_key",
    "access_option",
    "new_dest",
    "connect_delay",
    "delay_open",
    "client_per_minute",
    "client_per_hour",
    "client_per_day",
    "total_in_per_minute",
    "total_in_per_hour",
    "total_in_per_day",
    "post_limit",
    "post_limit_time",
    "per_client_period",
    "total_period",
    "total_ban_time",
    "filter_file_path",
    "priv_key_file",
];
/// Plan 324 exposes only algorithms supported by the current
/// destination-identity and LeaseSet2 encryption owners.
pub const SUPPORTED_324_OPTIONS: [&str; 2] = ["sig_type", "enc_type"];

/// Plan 334: the three LeaseSet security options that now have real owners.
///
/// `encrypt_lease_set` is the Proposal 170 `EncryptLeaseSet` mode,
/// `leaseset_password` is its `OptionalLookup` field, and
/// `leaseset_client_auth` is its `LeaseSetClientAuths` list in the bounded
/// durable encoding.
///
/// `leaseset_blinding_secret` is deliberately **not** here. The ELS2
/// specification defines exactly one lookup secret, and Proposal 170 spells it
/// once, so the duplicate i2pr slot keeps its refusal rather than becoming a
/// second secret an operator could believe is in effect.
pub const SUPPORTED_334_OPTIONS: [&str; 3] = [
    "encrypt_lease_set",
    "leaseset_password",
    "leaseset_client_auth",
];
/// Plan 342: the seven Proposal 170 outproxy fields, admitted as a
/// **complete set or not at all**.
///
/// Membership here only admits the keys to a definition; whether the set is
/// usable is decided by the whole-block rule in
/// [`crate::outproxy_options::parse_outproxy_block`], which refuses a partial
/// block by name. The two are deliberately the same seven keys: a key that
/// could be stored but never block-validated would be exactly the inert
/// acceptance Plan 342 forbids.
pub const SUPPORTED_342_OPTIONS: [&str; 7] = crate::outproxy_options::OUTPROXY_BLOCK_KEYS;

/// Plan 380: the ELS2 **consumer** client credential, admitted here rather
/// than in the frozen Proposal 170 option inventory.
///
/// Proposal 170 has no such field, and adding one would advertise a Proposal
/// option that does not exist. On the publisher side Proposal already spells
/// the authorized clients (`LeaseSetClientAuths`, i.e. `leaseset_client_auth`);
/// a consumer is expected to hold the matching PSK or DH key out of band, so
/// this key is an i2pr-local configuration surface and never reaches a Proposal
/// projection.
///
/// Opt-in and absent by default: a definition that does not set it is byte-for-
/// byte the Plan 351 definition it was before this plan.
pub const SUPPORTED_380_OPTIONS: [&str; 1] =
    [crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION];
/// Default per-service connection ceiling for control-created tunnels.
pub const DEFAULT_CONTROL_MAX_CONNECTIONS: usize = 16;

/// Definition provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelProvenance {
    /// Originated from daemon TOML/static configuration. Inspectable;
    /// I2PControl mutation is rejected.
    StartupOwned,
    /// Originated from TunnelManager; persisted under the control root.
    ControlOwned,
}

impl TunnelProvenance {
    /// Canonical wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StartupOwned => "startup",
            Self::ControlOwned => "control",
        }
    }
}

/// One control-owned tunnel definition (durable intent).
#[derive(Clone, Eq, PartialEq)]
pub struct ControlDefinition {
    /// Tunnel name (unique across both ownership classes).
    pub name: String,
    /// Proposal tunnel type.
    pub tunnel_type: TunnelType,
    /// Normalized runtime options plus bounded TunnelManager metadata.
    pub options: BTreeMap<String, String>,
    /// Persisted start-at-startup intent (distinct from running state).
    pub start_on_load: bool,
}

impl ControlDefinition {
    /// Plan 334: the resolved LeaseSet security posture of this definition.
    ///
    /// Derived from `options` on every call rather than stored alongside them,
    /// so the two cannot drift: there is no state in which a definition's
    /// recorded posture disagrees with the options it was derived from, and a
    /// reloaded generation is re-validated instead of trusted.
    pub fn lease_set_security(&self) -> Result<LeaseSetSecurityPlan, ControlError> {
        lease_set_security_plan(&self.options)
    }
}

/// Resolves the LeaseSet security posture of one options map.
///
/// A map with none of the three Plan 334 keys is the ordinary posture, which is
/// also what `disable` means. The identical rule runs in the request envelope
/// and here, so a merged `create`+stored-`edit` candidate and a reloaded stored
/// definition obey the same law as a fresh request.
///
/// The error names the offending option and the selected mode and never a
/// secret value.
pub fn lease_set_security_plan(
    options: &BTreeMap<String, String>,
) -> Result<LeaseSetSecurityPlan, ControlError> {
    // Plan 351 Gate 2 — the consumer shape is exempt from the *publisher* LeaseSet security
    // block, because a client that consumes a `.b33` publishes nothing.
    //
    // Without this exemption the block validator refuses a client for configuring
    // `leaseset_password` at all, with "OptionalLookup is not used by EncryptLeaseSet
    // 'disable'". That refusal is correct about the publisher reading and wrong about the
    // consumer one: the option is the same secret, but it participates in deriving the *lookup*
    // key this router asks a floodfill for, not the key it publishes under.
    //
    // Narrowly scoped on purpose. The shape is only recognised when the definition supplies
    // exactly the consumer pairing — a lookup secret and an encrypted-service target — and
    // `encrypt_lease_set` is absent, because a definition that publishes is a publisher no matter
    // what it also consumes and must go through the full block rule. A `leaseset_password` on a
    // client with an ordinary target never reaches here: the mask and pairing rules reject it
    // first, in `build_control_spec`.
    if is_encrypted_consumer(options) {
        return Ok(i2pr_i2pcontrol::resolve_lease_set_security(None, None, &[])
            .expect("the ordinary security block is valid by construction"));
    }
    if !options.contains_key("encrypt_lease_set")
        && !options.contains_key("leaseset_password")
        && !options.contains_key("leaseset_client_auth")
    {
        return Ok(i2pr_i2pcontrol::resolve_lease_set_security(None, None, &[])
            .expect("the ordinary security block is valid by construction"));
    }
    i2pr_i2pcontrol::tunnel_request::validate_lease_set_security_block(options).map_err(|error| {
        // `BadValue` carries the rule's own message, which names the mode and
        // never a secret. The envelope variant is unreachable here because the
        // caller already holds decoded option strings.
        let reason = match error {
            i2pr_i2pcontrol::TunnelRequestError::BadValue(message) => message,
            other => format!("{other:?}"),
        };
        ControlError::LeaseSetSecurityRejected(reason)
    })
}

/// Plan 351 Gate 2 — whether one options map is an ELS2 **consumer** shape.
///
/// True only when the map supplies a lookup secret and names an encrypted-service target and
/// does not publish. Deliberately computable from options alone, so `normalize_definition` and
/// the product reconciliation agree without either one consulting the other.
fn is_encrypted_consumer(options: &BTreeMap<String, String>) -> bool {
    if options.contains_key("encrypt_lease_set") || !options.contains_key("leaseset_password") {
        return false;
    }
    options
        .get("target_destination")
        .and_then(|value| DestinationRef::parse(value).ok())
        .is_some_and(|reference| matches!(reference, DestinationRef::EncryptedService { .. }))
}

/// Projects one definition's resolved LeaseSet security posture for a control
/// response.
///
/// Reports *whether* each secret is configured, the lookup secret's length,
/// and the client count — never a byte of either. This is the same rule the
/// ELS2 secret types enforce structurally by having no revealing `Display`,
/// no `Debug` that reveals, and no serializer; the control surface cannot
/// avoid the question, so it answers it without the value.
///
/// The `EncryptLeaseSet` mode itself is `OptionSensitivity::Public` and is
/// reported verbatim, so an operator can read back which of the ten values the
/// stored definition actually recorded.
pub fn lease_set_security_projection(definition: &ControlDefinition) -> serde_json::Value {
    let plan = definition
        .lease_set_security()
        .expect("a stored definition has already passed this rule");
    let flags = plan.address_flags();
    serde_json::json!({
        "encryptLeaseSet": plan.spelling(),
        "behavior": format!("{:?}", plan.behavior()),
        "publishesEncryptedLeaseSet2": plan.publishes_type5(),
        "lookupSecretConfigured": plan.lookup_secret_supplied(),
        // The lookup secret is needed verbatim to derive the daily blinded
        // key, so its length is the only non-secret fact about it worth
        // returning. It is bounded, so it cannot be a covert channel.
        "lookupSecretLength": definition
            .options
            .get("leaseset_password")
            .map_or(0, String::len),
        "clientAuthScheme": plan.client_auth_scheme().map(|scheme| match scheme {
            LeaseSetClientAuthScheme::PreSharedKey => "psk",
            LeaseSetClientAuthScheme::DiffieHellman => "dh",
        }),
        "clientAuthCount": plan.client_count(),
        "requiresBlindingSecret": flags.requires_blinding_secret(),
        "requiresClientKey": flags.requires_client_key(),
        // Plan 380: the **consumer** credential, which is a different value
        // from `clientAuthCount` above — that is how many clients the
        // *publisher* admits, this is whether *this* router holds one of them.
        // A boolean only: the value is raw key material, its length is
        // constant across both schemes, and its scheme is already public in
        // the `.b33` address flags, so there is nothing else worth returning.
        "clientCredentialConfigured": definition
            .options
            .get(crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION)
            .is_some_and(|value| !value.is_empty()),
    })
}

impl core::fmt::Debug for ControlDefinition {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Keys are schema; values are operator data and never printed.
        // Secret-classified keys can never be present (rejected before
        // storage), and this redaction holds regardless.
        formatter
            .debug_struct("ControlDefinition")
            .field("name", &self.name)
            .field("tunnel_type", &self.tunnel_type)
            .field("option_keys", &self.options.keys().collect::<Vec<_>>())
            .field("start_on_load", &self.start_on_load)
            .finish()
    }
}

/// Durable control error. Messages name keys and kinds, never values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControlError {
    /// Tunnel name unknown to either ownership class.
    UnknownTunnel(String),
    /// Name collides across ownership classes or within control state.
    NameCollision(String),
    /// Attempted mutation of a startup-owned definition.
    StartupMutationRejected(String),
    /// Proposal type without a Plan 290 runtime backend.
    UnsupportedType(String),
    /// Option outside the Plan 289 supported subset.
    UnsupportedOption(String),
    /// Supported option with an unparsable value.
    InvalidOption {
        /// Option key.
        option: String,
        /// Static reason (never echoes the value).
        reason: &'static str,
    },
    /// Plan 334: the LeaseSet security block is internally inconsistent — a
    /// mode that needs `OptionalLookup` or `LeaseSetClientAuths` did not get
    /// it, or got one its mode does not consume, or the mode itself is one
    /// i2pr refuses.
    ///
    /// A dedicated variant rather than [`Self::InvalidOption`] because the
    /// reason is owned bounded text: it must name *which* Proposal mode was
    /// selected to be actionable, and `InvalidOption`'s `&'static str` cannot
    /// carry that. The text comes from the frozen mode-mapping rule and names
    /// the mode and the key, never a secret value.
    LeaseSetSecurityRejected(String),
    /// Options contradict the tunnel kind.
    ContradictoryOptions {
        /// Tunnel name.
        name: String,
        /// Static reason.
        reason: &'static str,
    },
    /// Plan 342: the outproxy block is internally inconsistent — a partial
    /// block, a disabled provider declaration, an incomplete credential
    /// triple, or a block on a kind with no clearnet request target.
    ///
    /// A dedicated variant for the same reason
    /// [`Self::LeaseSetSecurityRejected`] has one: the message must name
    /// *which* of the seven fields are missing to be actionable, and
    /// `ContradictoryOptions`' `&'static str` cannot carry that. The text is
    /// built from option key names and frozen literals only — never from a
    /// value, and never from a credential.
    OutproxyBlockRejected(String),
    /// Malformed request envelope detail (static reason only).
    InvalidRequest(&'static str),
    /// Aggregate candidate set violates M10 ceilings.
    AggregateRejected(&'static str),
    /// Persistence failure.
    Store(StoreError),
    /// Runtime staging/reconcile failure (manager untouched on staging
    /// failure; message carries the manager's static reason only).
    Manager(&'static str),
    /// Durable publication failed after a successful runtime transition;
    /// reconcile-back was attempted (see `reconciled_back`).
    PublishFailed {
        /// Static reason.
        reason: &'static str,
        /// Whether reconcile-back to the prior generation succeeded.
        reconciled_back: bool,
    },
    /// Control state is unavailable (service constructed standalone).
    Unavailable,
}

impl core::fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownTunnel(name) => write!(formatter, "unknown tunnel {name}"),
            Self::NameCollision(name) => write!(formatter, "tunnel name collision for {name}"),
            Self::StartupMutationRejected(name) => {
                write!(
                    formatter,
                    "startup-owned tunnel {name} rejects I2PControl mutation"
                )
            }
            Self::UnsupportedType(kind) => {
                write!(
                    formatter,
                    "tunnel type {kind} has no Plan 290 backend (Plan 290)"
                )
            }
            Self::UnsupportedOption(option) => {
                write!(formatter, "option {option} is not supported in Plan 289")
            }
            Self::InvalidOption { option, reason } => {
                write!(formatter, "invalid option {option}: {reason}")
            }
            Self::LeaseSetSecurityRejected(reason) => {
                write!(formatter, "invalid LeaseSet security block: {reason}")
            }
            Self::ContradictoryOptions { name, reason } => {
                write!(formatter, "contradictory options for {name}: {reason}")
            }
            Self::OutproxyBlockRejected(reason) => {
                write!(formatter, "invalid outproxy block: {reason}")
            }
            Self::InvalidRequest(reason) => {
                write!(formatter, "invalid TunnelManager request: {reason}")
            }
            Self::AggregateRejected(reason) => {
                write!(formatter, "candidate aggregate rejected: {reason}")
            }
            Self::Store(error) => write!(formatter, "control store failed: {error}"),
            Self::Manager(reason) => write!(formatter, "runtime transition failed: {reason}"),
            Self::PublishFailed {
                reason,
                reconciled_back,
            } => write!(
                formatter,
                "durable publication failed ({reason}); reconciled back: {reconciled_back} (Plan 289)"
            ),
            Self::Unavailable => write!(
                formatter,
                "TunnelManager control state unavailable (Plan 289)"
            ),
        }
    }
}

impl ControlError {
    /// JSON-RPC code: validation failures are invalid params; store,
    /// runtime, and publication failures are internal errors.
    pub const fn wire_code(&self) -> i64 {
        match self {
            Self::Store(_) | Self::Manager(_) | Self::PublishFailed { .. } | Self::Unavailable => {
                -32603
            }
            _ => -32602,
        }
    }
}

/// Persistence failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError {
    /// Filesystem I/O failure (static reason only).
    Io(&'static str),
    /// Generation file fails schema validation.
    Corrupt(&'static str),
    /// Byte ceiling exceeded.
    OverBound,
    /// Path escape or separator attempt.
    PathEscape,
    /// Symlink or special file where a regular file/dir is required.
    NotRegular,
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(reason) => write!(formatter, "storage I/O failed: {reason}"),
            Self::Corrupt(reason) => write!(formatter, "generation corrupt: {reason}"),
            Self::OverBound => write!(formatter, "generation over byte ceiling"),
            Self::PathEscape => write!(formatter, "path escape rejected"),
            Self::NotRegular => write!(formatter, "symlink or special file rejected"),
        }
    }
}

/// Serializable generation file (deterministic key order).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct GenerationFile {
    schema_version: u8,
    generation_id: u64,
    definitions: Vec<StoredDefinition>,
}

/// Serializable definition row.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct StoredDefinition {
    name: String,
    tunnel_type: String,
    options: BTreeMap<String, String>,
    start_on_load: bool,
}

/// Serializable pointer file.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct PointerFile {
    schema_version: u8,
    current: u64,
}

/// A loaded generation: id plus ordered definitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedGeneration {
    /// Published generation id (`0` when no generation exists yet).
    pub generation_id: u64,
    /// Definitions in name order.
    pub definitions: Vec<ControlDefinition>,
}

/// Versioned, recoverable control generation store.
///
/// Layout beneath `<data_dir>/i2pcontrol/tunnels/`:
/// `staged-<id>.json` files (never loadable), `generation-<id>.json`
/// files (published only), plus a `current.json` pointer. Staging
/// writes the next staged file without touching published state;
/// publication renames the staged file into place and then moves the
/// pointer atomically. Recovery reads the pointer target, falling back
/// to the newest valid published generation file; retention keeps the
/// current plus prior published generation and removes older files.
#[derive(Clone, Debug)]
pub struct ControlStore {
    root: PathBuf,
}

impl ControlStore {
    /// Opens (creating) the control state root with owner-only
    /// permissions. Rejects symlink roots.
    pub fn open(data_dir: &Path) -> Result<Self, StoreError> {
        let root = data_dir
            .join(CONTROL_STATE_SUBDIR)
            .join(CONTROL_TUNNELS_SUBDIR);
        reject_symlink(data_dir)?;
        create_dir_secure(&root)?;
        reject_symlink(&root)?;
        let filter_root = root.join(FILTER_DIRECTORY);
        create_dir_secure(&filter_root)?;
        reject_symlink(&filter_root)?;
        Ok(Self { root })
    }

    /// State root (tests).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Confined root for bounded server peer-filter files.
    pub fn filter_root(&self) -> PathBuf {
        self.root.join(FILTER_DIRECTORY)
    }

    /// Next generation id: one above the highest id present in
    /// published or staged files (monotonic across restarts; gaps from
    /// retention are harmless).
    fn next_id(&self) -> Result<u64, StoreError> {
        let mut highest = 0_u64;
        for id in self.list_published_ids()? {
            highest = highest.max(id);
        }
        for id in self.list_staged_ids()? {
            highest = highest.max(id);
        }
        highest.checked_add(1).ok_or(StoreError::OverBound)
    }

    /// Lists published generation ids present on disk, ascending.
    fn list_published_ids(&self) -> Result<Vec<u64>, StoreError> {
        self.list_ids("generation-", ".json")
    }

    /// Lists staged (never loadable) generation ids, ascending.
    fn list_staged_ids(&self) -> Result<Vec<u64>, StoreError> {
        self.list_ids("staged-", ".json")
    }

    /// Lists ids for one filename shape, sorted ascending.
    fn list_ids(&self, prefix: &str, suffix: &str) -> Result<Vec<u64>, StoreError> {
        let mut ids = Vec::new();
        let entries = std::fs::read_dir(&self.root).map_err(|_| StoreError::Io("read root"))?;
        for entry in entries {
            let entry = entry.map_err(|_| StoreError::Io("read entry"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = parse_suffixed_name(&name, prefix, suffix) {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    /// Loads the published generation with prior-generation recovery.
    pub fn load(&self) -> Result<LoadedGeneration, StoreError> {
        // Prefer the pointer target when it parses and validates.
        if let Ok(pointer_bytes) = read_bounded(&self.pointer_path())
            && let Ok(pointer) = serde_json::from_slice::<PointerFile>(&pointer_bytes)
            && pointer.schema_version == CONTROL_SCHEMA_VERSION
            && let Ok(definitions) = self.load_generation(pointer.current)
        {
            return Ok(LoadedGeneration {
                generation_id: pointer.current,
                definitions,
            });
        }
        // Fall back to the newest valid published generation file.
        // Staged files are invisible here by construction.
        let mut ids = self.list_published_ids()?;
        ids.reverse();
        for id in ids {
            if let Ok(definitions) = self.load_generation(id) {
                return Ok(LoadedGeneration {
                    generation_id: id,
                    definitions,
                });
            }
        }
        Ok(LoadedGeneration {
            generation_id: 0,
            definitions: Vec::new(),
        })
    }

    /// Loads and validates one generation file.
    fn load_generation(&self, id: u64) -> Result<Vec<ControlDefinition>, StoreError> {
        let path = self.generation_path(id);
        reject_symlink(&path)?;
        let bytes = read_bounded(&path)?;
        let file: GenerationFile =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt("unparsable JSON"))?;
        if file.schema_version != CONTROL_SCHEMA_VERSION {
            return Err(StoreError::Corrupt("schema version"));
        }
        if file.generation_id != id {
            return Err(StoreError::Corrupt("id mismatch"));
        }
        if file.definitions.len() > MAX_SERVICE_TUNNELS {
            return Err(StoreError::Corrupt("definition ceiling"));
        }
        let mut definitions = Vec::with_capacity(file.definitions.len());
        let mut names = BTreeSet::new();
        for stored in file.definitions {
            validate_tunnel_name(&stored.name).map_err(|_| StoreError::Corrupt("name"))?;
            if !names.insert(stored.name.clone()) {
                return Err(StoreError::Corrupt("duplicate name"));
            }
            if stored.options.len() > 64 {
                return Err(StoreError::Corrupt("option ceiling"));
            }
            let tunnel_type =
                TunnelType::parse(&stored.tunnel_type).map_err(|_| StoreError::Corrupt("type"))?;
            // Plan 334: the stored LeaseSet security block is re-validated on
            // load, not trusted. A generation written under an older or
            // different rule that no longer satisfies the frozen mapping fails
            // the whole load closed rather than starting a service in a posture
            // the operator never asked for. The reason is deliberately dropped:
            // it can name the mode but must never surface stored bytes.
            let _ = lease_set_security_plan(&stored.options)
                .map_err(|_| StoreError::Corrupt("leaseset security"))?;
            definitions.push(ControlDefinition {
                name: stored.name,
                tunnel_type,
                options: stored.options,
                start_on_load: stored.start_on_load,
            });
        }
        Ok(definitions)
    }

    /// Stages a candidate generation as a never-loadable staged file.
    /// Returns the staged generation id.
    pub fn stage(
        &self,
        definitions: &BTreeMap<String, ControlDefinition>,
    ) -> Result<u64, StoreError> {
        if definitions.len() > MAX_SERVICE_TUNNELS {
            return Err(StoreError::OverBound);
        }
        let id = self.next_id()?;
        let stored: Vec<StoredDefinition> = definitions
            .values()
            .map(|definition| StoredDefinition {
                name: definition.name.clone(),
                tunnel_type: definition.tunnel_type.name().to_owned(),
                options: definition.options.clone(),
                start_on_load: definition.start_on_load,
            })
            .collect();
        let file = GenerationFile {
            schema_version: CONTROL_SCHEMA_VERSION,
            generation_id: id,
            definitions: stored,
        };
        let bytes = serde_json::to_vec(&file).map_err(|_| StoreError::Corrupt("serialization"))?;
        if bytes.len() > MAX_GENERATION_BYTES {
            return Err(StoreError::OverBound);
        }
        write_atomic(&self.staged_path(id), &bytes)?;
        Ok(id)
    }

    /// Publishes a staged generation id: moves the pointer first (so a
    /// crash before the rename falls back to the prior generation),
    /// then renames the staged file into place, then enforces bounded
    /// retention (current plus prior kept).
    pub fn publish(&self, id: u64) -> Result<(), StoreError> {
        let staged = self.staged_path(id);
        reject_symlink(&staged)?;
        read_bounded(&staged)?;
        let pointer = PointerFile {
            schema_version: CONTROL_SCHEMA_VERSION,
            current: id,
        };
        let bytes =
            serde_json::to_vec(&pointer).map_err(|_| StoreError::Corrupt("serialization"))?;
        write_atomic(&self.pointer_path(), &bytes)?;
        // The rename follows the pointer move: a crash between the two
        // loads the prior generation via scan fallback, and the next
        // publish converges (pointer already names this id).
        if std::fs::rename(&staged, self.generation_path(id)).is_err() {
            return Err(StoreError::Io("rename staged"));
        }
        self.enforce_retention(id)?;
        Ok(())
    }

    /// Removes published generation files older than the prior generation.
    fn enforce_retention(&self, current: u64) -> Result<(), StoreError> {
        for id in self.list_published_ids()? {
            if id < current.saturating_sub(1) {
                let _ = std::fs::remove_file(self.generation_path(id));
            }
        }
        Ok(())
    }

    /// Removes every staged (never-loadable) file plus, defensively,
    /// any published file newer than the pointer. Called at startup
    /// before loading; the load path itself never deletes.
    pub fn cleanup_orphans(&self) {
        let Ok(staged) = self.list_staged_ids() else {
            return;
        };
        for id in staged {
            let _ = std::fs::remove_file(self.staged_path(id));
        }
        let current = self.pointer_current().unwrap_or(0);
        let Ok(published) = self.list_published_ids() else {
            return;
        };
        for id in published {
            if id > current {
                let _ = std::fs::remove_file(self.generation_path(id));
            }
        }
    }

    /// Reads the pointer's current id without validation fallback.
    fn pointer_current(&self) -> Result<u64, StoreError> {
        let bytes = read_bounded(&self.pointer_path())?;
        let pointer: PointerFile =
            serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt("pointer"))?;
        if pointer.schema_version != CONTROL_SCHEMA_VERSION {
            return Err(StoreError::Corrupt("pointer schema"));
        }
        Ok(pointer.current)
    }

    /// Current published generation id (`0` when nothing published yet).
    pub fn current_id(&self) -> u64 {
        self.load().map(|loaded| loaded.generation_id).unwrap_or(0)
    }

    /// Generation file path (no existence check).
    fn generation_path(&self, id: u64) -> PathBuf {
        self.root.join(format!("generation-{id}.json"))
    }

    /// Staged file path (never loadable; no existence check).
    fn staged_path(&self, id: u64) -> PathBuf {
        self.root.join(format!("staged-{id}.json"))
    }

    /// Pointer file path.
    fn pointer_path(&self) -> PathBuf {
        self.root.join(CONTROL_POINTER_FILE)
    }
}

/// Parses `generation-<id>.json` / `staged-<id>.json` file names.
fn parse_suffixed_name(name: &str, prefix: &str, suffix: &str) -> Option<u64> {
    name.strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse::<u64>()
        .ok()
}

/// Reads a file with the generation byte ceiling enforced.
fn read_bounded(path: &Path) -> Result<Vec<u8>, StoreError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| StoreError::Io("stat"))?;
    if metadata.file_type().is_symlink() {
        return Err(StoreError::NotRegular);
    }
    if !metadata.is_file() {
        return Err(StoreError::NotRegular);
    }
    if metadata.len() > MAX_GENERATION_BYTES as u64 {
        return Err(StoreError::OverBound);
    }
    std::fs::read(path).map_err(|_| StoreError::Io("read"))
}

/// Rejects symlink roots/files (no TOCTOU claim across the check/use
/// boundary; generation files are content-validated on load regardless).
fn reject_symlink(path: &Path) -> Result<(), StoreError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StoreError::NotRegular),
        Ok(_) => Ok(()),
        // Absent paths are created fresh below; nothing to reject.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(StoreError::Io("stat")),
    }
}

/// Creates a directory (and parents) with owner-only permissions on
/// unix. Non-unix platforms keep default permissions.
fn create_dir_secure(path: &Path) -> Result<(), StoreError> {
    std::fs::create_dir_all(path).map_err(|_| StoreError::Io("create dir"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| StoreError::Io("permissions"))?;
    }
    Ok(())
}

/// Writes bytes atomically: temp file + sync + rename + directory sync.
/// Temp files carry a hidden prefix so generation scans ignore them.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().ok_or(StoreError::PathEscape)?;
    let temp = parent.join(format!(".tmp-{}-{}", std::process::id(), temp_counter(),));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp).map_err(|_| StoreError::Io("create temp"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| StoreError::Io("permissions"))?;
        }
        file.write_all(bytes)
            .map_err(|_| StoreError::Io("write temp"))?;
        file.sync_all().map_err(|_| StoreError::Io("sync temp"))?;
    }
    std::fs::rename(&temp, path).map_err(|_| {
        let _ = std::fs::remove_file(&temp);
        StoreError::Io("rename")
    })?;
    sync_dir(parent)
}

/// Best-effort directory sync (durability of the rename itself).
fn sync_dir(dir: &Path) -> Result<(), StoreError> {
    let file = std::fs::File::open(dir).map_err(|_| StoreError::Io("open dir"))?;
    file.sync_all().map_err(|_| StoreError::Io("sync dir"))?;
    Ok(())
}

/// Process-wide temp-file counter (disambiguates concurrent writers).
fn temp_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Maps a Proposal type onto the existing M10 runtime family.
/// Plan 290 added the four composed families over the existing
/// manager and shared primitives; Plan 291 adds the two Streamr
/// families over the repliable-datagram substrate. All twelve
/// Proposal types map; unknown spellings never reach this point
/// (the contract parser rejects them).
pub fn map_tunnel_type(tunnel_type: TunnelType) -> Result<ServiceTunnelKind, ControlError> {
    match tunnel_type {
        TunnelType::Client => Ok(ServiceTunnelKind::GenericClient),
        TunnelType::Server => Ok(ServiceTunnelKind::GenericServer),
        TunnelType::HttpClient => Ok(ServiceTunnelKind::HttpClient),
        TunnelType::Socks => Ok(ServiceTunnelKind::Socks5Client),
        TunnelType::IrcClient => Ok(ServiceTunnelKind::IrcClient),
        TunnelType::IrcServer => Ok(ServiceTunnelKind::IrcServer),
        // Plan 290: composed families over the existing M10
        // manager and shared primitives.
        TunnelType::ConnectClient => Ok(ServiceTunnelKind::ConnectClient),
        TunnelType::SocksIrc => Ok(ServiceTunnelKind::SocksIrc),
        TunnelType::HttpServer => Ok(ServiceTunnelKind::HttpServer),
        TunnelType::HttpBidirServer => Ok(ServiceTunnelKind::HttpBidirServer),
        // Plan 291: Streamr families over the repliable-datagram
        // substrate. The match is exhaustive over the frozen
        // twelve-type inventory, so a thirteenth type fails at
        // compile time (fail-closed); runtime rejection of
        // unmapped types stays in `normalize_definition` and the
        // supervisor gates through `has_plan291_backend`.
        TunnelType::StreamrClient => Ok(ServiceTunnelKind::StreamrClient),
        TunnelType::StreamrServer => Ok(ServiceTunnelKind::StreamrServer),
    }
}

/// Parses one `true`/`false` option value.
fn parse_bool_option(option: &str, value: &str) -> Result<bool, ControlError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "must be true or false",
        }),
    }
}

fn parse_rate_option(option: &str, value: &str) -> Result<u32, ControlError> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "connection rate must be an integer within 0..=100000",
        })?;
    if parsed > 100_000 {
        return Err(ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "connection rate must be an integer within 0..=100000",
        });
    }
    Ok(parsed)
}

/// Opens a bounded regular filter file through a no-follow directory
/// walk rooted at the control-owned filter directory. The stored path
/// is a logical relative reference; absolute paths, traversal, symlinks,
/// special files, and permissive Unix file modes are rejected.
fn read_confined_filter_file(root: &Path, relative: &str) -> Result<String, &'static str> {
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::io::Read;
    use std::os::fd::OwnedFd;

    let components: Vec<&std::ffi::OsStr> = Path::new(relative)
        .components()
        .map(|component| match component {
            std::path::Component::Normal(name) => Ok(name),
            _ => Err("FilterFilePath must stay inside the owned filters directory"),
        })
        .collect::<Result<_, _>>()?;
    if components.is_empty() {
        return Err("FilterFilePath must name a file inside the owned filters directory");
    }

    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    let mut directory: OwnedFd = open(root, directory_flags, Mode::empty())
        .map_err(|_| "FilterFilePath owned root is unavailable")?;
    for component in &components[..components.len() - 1] {
        directory = openat(&directory, *component, directory_flags, Mode::empty())
            .map_err(|_| "FilterFilePath contains a missing or unsafe directory")?;
    }

    let file_flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
    let file = openat(
        &directory,
        components[components.len() - 1],
        file_flags,
        Mode::empty(),
    )
    .map_err(|_| "FilterFilePath file is missing or unsafe")?;
    let file = std::fs::File::from(file);
    let metadata = file
        .metadata()
        .map_err(|_| "FilterFilePath file metadata is unavailable")?;
    if !metadata.is_file() {
        return Err("FilterFilePath must name a regular file");
    }
    if metadata.len() > MAX_FILTER_FILE_BYTES as u64 {
        return Err("FilterFilePath exceeds the 4096-byte ceiling");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("FilterFilePath file must not be accessible by group or others");
        }
    }
    let mut contents = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_FILTER_FILE_BYTES + 1) as u64)
        .read_to_end(&mut contents)
        .map_err(|_| "FilterFilePath file could not be read")?;
    if contents.len() > MAX_FILTER_FILE_BYTES {
        return Err("FilterFilePath exceeds the 4096-byte ceiling");
    }
    String::from_utf8(contents).map_err(|_| "FilterFilePath must contain UTF-8 text")
}

/// Parses a Proposal tunnel length (Plan 292 shaping: 1..=3).
/// Length 0 is rejected: service destinations run in Remote tunnel
/// mode and the destination policy does not permit zero-hop pools.
fn parse_shaping_length(option: &str, value: &str) -> Result<u8, ControlError> {
    let length = value
        .parse::<u8>()
        .map_err(|_| ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "tunnel length must be an integer within 1..=3",
        })?;
    if length == 0 || length > MAX_TUNNEL_LENGTH_HOPS {
        return Err(ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "tunnel length must be within 1..=3 (zero-hop is not permitted for service destinations)",
        });
    }
    Ok(length)
}

/// Realm binding proxy credentials to their listener family
/// (Plan 292): verifiers never cross families.
fn proxy_realm_for_kind(kind: ServiceTunnelKind) -> &'static str {
    match kind {
        ServiceTunnelKind::ConnectClient => PROXY_AUTH_REALM_CONNECT,
        ServiceTunnelKind::HttpClient => PROXY_AUTH_REALM_HTTP,
        _ => PROXY_AUTH_REALM_SOCKS,
    }
}

/// Builds listener proxy credentials from the two raw halves (Plan
/// 292). Both halves are required together; a marked password
/// round-trips through the stored verifier so untouched credentials
/// survive edits, while fresh plaintext is hashed (never stored).
fn proxy_credentials_for(
    kind: ServiceTunnelKind,
    username: Option<String>,
    password: Option<String>,
) -> Result<Option<ProxyCredentials>, ControlError> {
    match (username, password) {
        (Some(username), Some(password)) => {
            let realm = proxy_realm_for_kind(kind);
            if password.starts_with(PROXY_VERIFIER_MARKER) {
                ProxyCredentials::from_stored(&username, &password, realm)
                    .map(Some)
                    .map_err(|_| ControlError::InvalidOption {
                        option: "proxy_password".to_owned(),
                        reason: "stored proxy verifier is malformed",
                    })
            } else {
                ProxyCredentials::new(&username, &password, realm)
                    .map(Some)
                    .map_err(|error| ControlError::InvalidOption {
                        option: match error {
                            i2pr_service_tunnels::ServiceTunnelError::ExceedsCeiling {
                                field,
                                ..
                            } => field.to_owned(),
                            _ => "proxy_password".to_owned(),
                        },
                        reason: "proxy credentials must be bounded and framed",
                    })
            }
        }
        (None, None) => Ok(None),
        _ => Err(ControlError::InvalidOption {
            option: "proxy_password".to_owned(),
            reason: "proxy_username and proxy_password are both required",
        }),
    }
}

/// Parses a Proposal tunnel quantity (Plan 292 shaping: 1..=6).
fn parse_shaping_quantity(option: &str, value: &str) -> Result<u8, ControlError> {
    let quantity = value
        .parse::<u8>()
        .map_err(|_| ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "tunnel quantity must be an integer within 1..=6",
        })?;
    if quantity == 0 || quantity > MAX_TUNNEL_QUANTITY {
        return Err(ControlError::InvalidOption {
            option: option.to_owned(),
            reason: "tunnel quantity must be within 1..=6",
        });
    }
    Ok(quantity)
}

/// Builds a validated [`ServiceTunnelSpec`] from a control definition.
///
/// Only options with a real owner are accepted (Plans 289, 291, the
/// Plan 292 option slice, the Plan 296 residual slice, the Plan 297
/// TLS slice, and implemented Plan 323 fields); every other supplied option fails as unsupported
/// (never accepted inertly). Secret-classified options are rejected
/// even though the subset contains none, so a future subset
/// extension cannot silently persist secrets.
pub fn build_control_spec(
    definition: &ControlDefinition,
) -> Result<ServiceTunnelSpec, ControlError> {
    build_control_spec_with_filter_root(definition, None)
}

/// Builds a control spec with access-filter files confined beneath
/// `filter_root`. The test/simple wrapper deliberately cannot open a
/// file because it has no owned filesystem root.
pub fn build_control_spec_with_filter_root(
    definition: &ControlDefinition,
    filter_root: Option<&Path>,
) -> Result<ServiceTunnelSpec, ControlError> {
    let kind = map_tunnel_type(definition.tunnel_type)?;
    let id = ServiceTunnelId::parse(&definition.name)
        .map_err(|_| ControlError::InvalidRequest("stored name fails service id validation"))?;
    let mut destination: Option<DestinationRef> = None;
    let mut target_host: Option<std::net::IpAddr> = None;
    let mut target_port: Option<u16> = None;
    let mut listen_host: Option<std::net::IpAddr> = None;
    let mut listen_port: u16 = 0;
    let mut max_connections = DEFAULT_CONTROL_MAX_CONNECTIONS;
    // Plan 292 shaping inputs (symmetric defaults plus per-direction
    // overrides; ranges enforced per key at parse time).
    let mut symmetric_length: Option<u8> = None;
    let mut symmetric_quantity: Option<u8> = None;
    let mut inbound_length: Option<u8> = None;
    let mut outbound_length: Option<u8> = None;
    let mut inbound_quantity: Option<u8> = None;
    let mut outbound_quantity: Option<u8> = None;
    // Plan 296 standby/variance inputs (ranges enforced per key at
    // parse time; the per-direction pool-maximum sum check runs at
    // spec validation through `TunnelShaping::try_new`).
    let mut backup_quantity: Option<u8> = None;
    let mut length_variance: Option<i8> = None;
    // Plan 292 profile selection (OR-combined effective bit).
    let mut profile_interactive = false;
    // Plan 292 idle inputs (pairing enforced at build time).
    let mut idle_timeout_ms: Option<u64> = None;
    let mut idle_close = false;
    let mut idle_new_dest = false;
    let mut rotate_destination_on_idle = false;
    let mut idle_reduce = false;
    let mut idle_close_timeout_ms: Option<u64> = None;
    let mut idle_reduce_timeout_ms: Option<u64> = None;
    let mut idle_reduce_count: Option<u8> = None;
    // Plan 292 proxy authentication inputs (both halves required;
    // plaintext is scrubbed to the marked verifier in normalize).
    let mut proxy_username: Option<String> = None;
    let mut proxy_password: Option<String> = None;
    let mut proxy_auth_declared: Option<bool> = None;
    // Proposal `Shared` places client tunnels in one explicit
    // control-owned Destination group. The shared group is an
    // intentional linkability domain, matching I2PTunnel's
    // sharedClient setting.
    let mut shared_client = false;
    let mut persistent_client_key = false;
    let mut persistent_client_key_declared: Option<bool> = None;
    let mut priv_key_file: Option<i2pr_service_tunnels::ServiceKeyReference> = None;
    let mut proposal_new_dest: Option<u8> = None;
    let mut connect_delay = false;
    let mut delay_open = false;
    let mut sig_type_selected = false;
    let mut enc_type_selected = false;
    let mut allow_user_agent: Option<bool> = None;
    let mut allow_referer: Option<bool> = None;
    let mut allow_accept: Option<bool> = None;
    let mut allow_internal_ssl: Option<bool> = None;
    // Plan 292 access inputs (raw values per source key so failures
    // name the offending key, never the value).
    let mut access_allow_sources: Vec<(String, String)> = Vec::new();
    let mut access_deny_sources: Vec<(String, String)> = Vec::new();
    let mut proposal_access_list: Option<String> = None;
    let mut proposal_access_option: Option<String> = None;
    let mut filter_file_path: Option<String> = None;
    let mut connection_rates = i2pr_service_tunnels::ServerConnectionRateLimits::default();
    // Plan 292 server dial/presentation inputs (kind-gated at
    // parse; defaults preserve the historical behavior).
    let mut unique_local_address = false;
    // Plan 296 delivery inputs: multihoming is kind-gated at parse
    // like the other server dial options; reply bundling applies to
    // every kind through the destination delivery path. Defaults preserve the historical behavior.
    let mut multihoming = false;
    let mut reply_bundling = false;
    // Plan 297 server TLS input (kind-gated at parse like the other
    // server dial options; the default preserves plaintext).
    let mut use_ssl = false;
    let mut http_policy = i2pr_service_tunnels::HttpServerPolicy::default();
    // Plan 291 Streamr inputs (validated per kind below; ranges
    // enforced by `StreamrOptions::validate` through the final
    // spec validation).
    let mut local_udp_host: Option<std::net::IpAddr> = None;
    let mut local_udp_port: Option<u16> = None;
    // Plan 292 subscriber media-sink redirect (streamr-client
    // only): the host media is sent to; the port comes from
    // local_udp_port, exactly as the matrix owner states.
    let mut remote_udp_host: Option<std::net::IpAddr> = None;
    let mut target_i2p_port: u16 = 0;
    let mut subscribe_interval_ms: Option<u64> = None;
    let mut subscription_expiry_ms: Option<u64> = None;
    let mut max_subscribers: Option<usize> = None;
    let mut payload_limit_bytes: Option<usize> = None;
    for (key, value) in &definition.options {
        match key.as_str() {
            "sig_type" => {
                if !matches!(value.as_str(), "7" | "EDDSA_SHA512_ED25519") {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "SigType supports only EdDSA_SHA512_ED25519 (type 7)",
                    });
                }
                sig_type_selected = true;
            }
            "enc_type" => {
                if value != "4" {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "EncType supports only LeaseSet2 X25519 (type 4)",
                    });
                }
                enc_type_selected = true;
            }
            "shared" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericClient
                        | ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::IrcClient
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "Shared applies to client tunnels with a local Destination",
                    });
                }
                shared_client = parse_bool_option(key, value)?;
            }
            "persistent_client_key" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericClient
                        | ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::IrcClient
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                        | ServiceTunnelKind::StreamrClient
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "PersistentClientKey applies to client tunnels only",
                    });
                }
                let enabled = parse_bool_option(key, value)?;
                persistent_client_key = enabled;
                persistent_client_key_declared = Some(enabled);
            }
            "priv_key_file" => {
                priv_key_file = Some(
                    i2pr_service_tunnels::ServiceKeyReference::parse(value).map_err(|_| {
                        ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "PrivKeyFile must be a logical lowercase key reference, not a path",
                        }
                    })?,
                );
            }
            "new_dest" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericClient
                        | ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::IrcClient
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                        | ServiceTunnelKind::StreamrClient
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "NewDest applies to client tunnels only",
                    });
                }
                let mode = value
                    .parse::<u8>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "NewDest must be 0, 1, or 2",
                    })?;
                if mode > 2 {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "NewDest must be 0, 1, or 2",
                    });
                }
                proposal_new_dest = Some(mode);
            }
            "connect_delay" => {
                if kind != ServiceTunnelKind::GenericClient {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "ConnectDelay applies to generic client tunnels only",
                    });
                }
                connect_delay = parse_bool_option(key, value)?;
            }
            "delay_open" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericClient
                        | ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::IrcClient
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "DelayOpen applies to client tunnels only",
                    });
                }
                delay_open = parse_bool_option(key, value)?;
            }
            "description" => {
                // Description is control-plane metadata: its authoritative
                // owner is the durable definition and canonical TunnelManager
                // Get projection, not the M10 traffic specification.
                if value.len() > MAX_OPTION_VALUE_LEN {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "description exceeds the Proposal string ceiling",
                    });
                }
            }
            "allow_user_agent" | "allow_referer" | "allow_accept" | "allow_internal_ssl" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpClient | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "HTTP request filters apply to HTTP client families only",
                    });
                }
                let enabled = value
                    .parse::<bool>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "HTTP filter must be a boolean",
                    })?;
                match key.as_str() {
                    "allow_user_agent" => allow_user_agent = Some(enabled),
                    "allow_referer" => allow_referer = Some(enabled),
                    "allow_accept" => allow_accept = Some(enabled),
                    "allow_internal_ssl" => allow_internal_ssl = Some(enabled),
                    _ => unreachable!(),
                }
            }
            "block_user_agents" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "BlockUserAgents applies to HTTP server kinds only",
                    });
                }
                http_policy.block_user_agents = parse_bool_option(key, value)?;
            }
            "block_access_in_proxies" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "BlockAccessInProxies applies to HTTP server kinds only",
                    });
                }
                http_policy.block_access_in_proxies = parse_bool_option(key, value)?;
            }
            "user_agents" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "UserAgents applies to HTTP server kinds only",
                    });
                }
                let rules: Vec<String> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|rule| !rule.is_empty())
                    .map(str::to_owned)
                    .collect();
                if !i2pr_service_tunnels::http::valid_user_agent_rules(&rules) {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "UserAgents exceeds the bounded substring rule format",
                    });
                }
                http_policy.user_agents = rules;
            }
            "post_limit" | "post_limit_time" | "per_client_period" | "total_period"
            | "total_ban_time" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "POST limits apply to HTTP server tunnels only",
                    });
                }
                let value = parse_rate_option(key, value)?;
                match key.as_str() {
                    "post_limit" => http_policy.post_limits.window_seconds = value,
                    "post_limit_time" => http_policy.post_limits.client_ban_seconds = value,
                    "per_client_period" => http_policy.post_limits.client_max = value,
                    "total_period" => http_policy.post_limits.total_max = value,
                    "total_ban_time" => http_policy.post_limits.total_ban_seconds = value,
                    _ => unreachable!(),
                }
            }
            "target_destination" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericClient
                        | ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::IrcClient
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                        | ServiceTunnelKind::StreamrClient
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "target_destination applies to client kinds only",
                    });
                }
                destination = Some(DestinationRef::parse(value).map_err(|_| {
                    ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "destination reference malformed",
                    }
                })?);
            }
            "target_host" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "target_host applies to server kinds only",
                    });
                }
                if value.starts_with("unix:") {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "unix targets are not yet supported",
                    });
                }
                let address: std::net::IpAddr =
                    value.parse().map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "target_host must be an IP literal",
                    })?;
                if !address.is_loopback() {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "target_host must be loopback",
                    });
                }
                target_host = Some(address);
            }
            "target_port" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "target_port applies to server kinds only",
                    });
                }
                target_port =
                    Some(
                        value
                            .parse::<u16>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "target_port must be 0..=65535",
                            })?,
                    );
            }
            "listen_host" => {
                if matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::StreamrClient
                        | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "listen_host applies to client kinds only",
                    });
                }
                let address: std::net::IpAddr =
                    value.parse().map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "listen_host must be an IP literal",
                    })?;
                if !address.is_loopback() {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "listen_host must be loopback",
                    });
                }
                listen_host = Some(address);
            }
            "listen_port" => {
                if matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::StreamrClient
                        | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "listen_port applies to client kinds only",
                    });
                }
                listen_port = value
                    .parse::<u16>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "listen_port must be 0..=65535",
                    })?;
            }
            "start_on_load" => {
                // Validated here so malformed values fail before any
                // side effect; the coordinator owns the persisted
                // intent on the definition, not the spec.
                let _ = parse_bool_option(key, value)?;
            }
            "max_streams" => {
                max_connections =
                    value
                        .parse::<usize>()
                        .map_err(|_| ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "max_streams must be a positive integer",
                        })?;
                if max_connections == 0 || max_connections > 128 {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "max_streams must be within 1..=128",
                    });
                }
            }
            "client_per_minute"
            | "client_per_hour"
            | "client_per_day"
            | "total_in_per_minute"
            | "total_in_per_hour"
            | "total_in_per_day" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "connection-rate controls require a server or Streamr publisher",
                    });
                }
                let rate = parse_rate_option(key, value)?;
                match key.as_str() {
                    "client_per_minute" => connection_rates.client_per_minute = rate,
                    "client_per_hour" => connection_rates.client_per_hour = rate,
                    "client_per_day" => connection_rates.client_per_day = rate,
                    "total_in_per_minute" => connection_rates.total_per_minute = rate,
                    "total_in_per_hour" => connection_rates.total_per_hour = rate,
                    "total_in_per_day" => connection_rates.total_per_day = rate,
                    _ => unreachable!(),
                }
            }
            // Plan 292: pool shaping. `tunnel_length` and
            // `tunnel_quantity` are symmetric defaults; the
            // per-direction keys override. Proposal bounds bind
            // (length 1..=3, quantity 1..=6); length 0 is rejected
            // because service destinations run in Remote tunnel mode.
            "tunnel_length" => {
                symmetric_length = Some(parse_shaping_length(key, value)?);
            }
            "inbound_length" => {
                inbound_length = Some(parse_shaping_length(key, value)?);
            }
            "outbound_length" => {
                outbound_length = Some(parse_shaping_length(key, value)?);
            }
            "tunnel_quantity" => {
                symmetric_quantity = Some(parse_shaping_quantity(key, value)?);
            }
            "inbound_quantity" => {
                inbound_quantity = Some(parse_shaping_quantity(key, value)?);
            }
            "outbound_quantity" => {
                outbound_quantity = Some(parse_shaping_quantity(key, value)?);
            }
            // Plan 296: standby quantity (Proposal bound 0..=3) and
            // per-build length variance (Proposal bound -2..=+2).
            "tunnel_backup_quantity" => {
                let backup = value
                    .parse::<u8>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "tunnel backup quantity must be an integer within 0..=3",
                    })?;
                if backup > MAX_TUNNEL_BACKUP_QUANTITY {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "tunnel backup quantity must be within 0..=3",
                    });
                }
                backup_quantity = Some(backup);
            }
            "tunnel_variance" => {
                let variance = value
                    .parse::<i8>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "tunnel variance must be an integer within -2..=+2",
                    })?;
                if !(-MAX_TUNNEL_LENGTH_VARIANCE..=MAX_TUNNEL_LENGTH_VARIANCE).contains(&variance) {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "tunnel variance must be within -2..=+2",
                    });
                }
                length_variance = Some(variance);
            }
            // Plan 292: streaming profile. Only "interactive" is
            // special (PR6 reference: it constrains the streaming
            // window); any other value must be the explicit "bulk"
            // default. The boolean is OR-combined: either knob
            // selects the interactive windows.
            "profile" => {
                if matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "profile applies to streaming kinds only",
                    });
                }
                match value.as_str() {
                    "interactive" => profile_interactive = true,
                    "bulk" => {}
                    _ => {
                        return Err(ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "profile must be \"bulk\" or \"interactive\"",
                        });
                    }
                }
            }
            "interactive" => {
                if matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "interactive applies to streaming kinds only",
                    });
                }
                if parse_bool_option(key, value)? {
                    profile_interactive = true;
                }
            }
            // Plan 292: idle policy. The deadline is milliseconds;
            // flags without a deadline take the documented default
            // at build time, while a deadline without flags is
            // rejected by spec validation (inert configuration).
            "idle_timeout" => {
                let timeout = value
                    .parse::<u64>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "idle_timeout must be an integer in milliseconds",
                    })?;
                if !(MIN_IDLE_TIMEOUT_MS..=MAX_IDLE_TIMEOUT_MS).contains(&timeout) {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "idle_timeout must be within 1000..=86400000 milliseconds",
                    });
                }
                idle_timeout_ms = Some(timeout);
            }
            "close_on_idle" => {
                if parse_bool_option(key, value)? {
                    idle_close = true;
                }
            }
            "new_dest_on_idle" => {
                if parse_bool_option(key, value)? {
                    idle_new_dest = true;
                }
            }
            "reduce_on_idle" => {
                if parse_bool_option(key, value)? {
                    idle_reduce = true;
                }
            }
            "close_time" => {
                let minutes = value
                    .parse::<u64>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "CloseTime must be an integer number of minutes",
                    })?;
                let timeout =
                    minutes
                        .checked_mul(60_000)
                        .ok_or_else(|| ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "CloseTime exceeds the bounded idle deadline",
                        })?;
                if minutes > 9_999 {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "CloseTime must be within 0..=9999 minutes",
                    });
                }
                idle_close_timeout_ms = Some(timeout);
            }
            "reduce_time" => {
                let minutes = value
                    .parse::<u64>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "ReduceTime must be an integer number of minutes",
                    })?;
                let timeout =
                    minutes
                        .checked_mul(60_000)
                        .ok_or_else(|| ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "ReduceTime exceeds the bounded idle deadline",
                        })?;
                if minutes > 9_999 {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "ReduceTime must be within 0..=9999 minutes",
                    });
                }
                idle_reduce_timeout_ms = Some(timeout);
            }
            "reduce_count" => {
                let count = value
                    .parse::<u8>()
                    .map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "ReduceCount must be within 0..=9",
                    })?;
                if count > 9 {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "ReduceCount must be within 0..=9",
                    });
                }
                idle_reduce_count = Some(count);
            }
            // Plan 292: listener proxy authentication (proxy client
            // kinds only; both halves required together, PR6 rule).
            "proxy_username" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "proxy_username applies to proxy client kinds only",
                    });
                }
                proxy_username = Some(value.clone());
            }
            "proxy_password" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "proxy_password applies to proxy client kinds only",
                    });
                }
                proxy_password = Some(value.clone());
            }
            "proxy_auth" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpClient
                        | ServiceTunnelKind::Socks5Client
                        | ServiceTunnelKind::ConnectClient
                        | ServiceTunnelKind::SocksIrc
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "proxy_auth applies to proxy client kinds only",
                    });
                }
                proxy_auth_declared = Some(parse_bool_option(key, value)?);
            }
            "access_option" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "AccessOption applies to server tunnels only",
                    });
                }
                if value != "allow" && value != "deny" {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "AccessOption must be allow or deny",
                    });
                }
                proposal_access_option = Some(value.clone());
            }
            "filter_file_path" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "FilterFilePath applies to server tunnels only",
                    });
                }
                if value.is_empty() || value.len() > 128 {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "FilterFilePath must be a bounded relative path",
                    });
                }
                filter_file_path = Some(value.clone());
            }
            // Plan 292: inbound peer policy (server kinds only;
            // access_list unions white_list, black_list denies).
            "access_list" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "access lists apply to server kinds only",
                    });
                }
                proposal_access_list = Some(value.clone());
            }
            "white_list" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "access lists apply to server kinds only",
                    });
                }
                access_allow_sources.push((key.clone(), value.clone()));
            }
            "black_list" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "access lists apply to server kinds only",
                    });
                }
                access_deny_sources.push((key.clone(), value.clone()));
            }
            // Plan 291: Streamr UDP endpoints and cadence policy.
            // The loopback shape is enforced here; numeric ranges
            // are enforced by `StreamrOptions::validate` through
            // the final spec validation.
            "local_udp_host" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "local_udp_host applies to Streamr kinds only",
                    });
                }
                let address: std::net::IpAddr =
                    value.parse().map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "local_udp_host must be an IP literal",
                    })?;
                if !address.is_loopback() {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "local_udp_host must be loopback",
                    });
                }
                local_udp_host = Some(address);
            }
            "local_udp_port" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "local_udp_port applies to Streamr kinds only",
                    });
                }
                local_udp_port =
                    Some(
                        value
                            .parse::<u16>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "local_udp_port must be 0..=65535",
                            })?,
                    );
            }
            "remote_udp_host" => {
                if !matches!(kind, ServiceTunnelKind::StreamrClient) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "remote_udp_host applies to streamr-client only",
                    });
                }
                let address: std::net::IpAddr =
                    value.parse().map_err(|_| ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "remote_udp_host must be an IP literal",
                    })?;
                if !address.is_loopback() {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "remote_udp_host must be loopback",
                    });
                }
                remote_udp_host = Some(address);
            }
            "target_i2p_port" => {
                if !matches!(kind, ServiceTunnelKind::StreamrClient) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "target_i2p_port applies to streamr-client only",
                    });
                }
                target_i2p_port =
                    value
                        .parse::<u16>()
                        .map_err(|_| ControlError::InvalidOption {
                            option: key.clone(),
                            reason: "target_i2p_port must be 0..=65535",
                        })?;
            }
            "streamr_subscribe_interval" => {
                if !matches!(kind, ServiceTunnelKind::StreamrClient) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "streamr_subscribe_interval applies to streamr-client only",
                    });
                }
                subscribe_interval_ms =
                    Some(
                        value
                            .parse::<u64>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "streamr_subscribe_interval must be an integer",
                            })?,
                    );
            }
            "streamr_expiry" => {
                if !matches!(kind, ServiceTunnelKind::StreamrServer) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "streamr_expiry applies to streamr-server only",
                    });
                }
                subscription_expiry_ms =
                    Some(
                        value
                            .parse::<u64>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "streamr_expiry must be an integer",
                            })?,
                    );
            }
            "streamr_max_subscribers" => {
                if !matches!(kind, ServiceTunnelKind::StreamrServer) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "streamr_max_subscribers applies to streamr-server only",
                    });
                }
                max_subscribers =
                    Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "streamr_max_subscribers must be an integer",
                            })?,
                    );
            }
            "streamr_payload_limit" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "streamr_payload_limit applies to Streamr kinds only",
                    });
                }
                payload_limit_bytes =
                    Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| ControlError::InvalidOption {
                                option: key.clone(),
                                reason: "streamr_payload_limit must be an integer",
                            })?,
                    );
            }
            "unique_local_address" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "unique_local_address applies to server kinds only",
                    });
                }
                unique_local_address = parse_bool_option("unique_local_address", value)?;
            }
            // Plan 296: multihoming consumes the server dial target
            // list (server kinds only); reply bundling applies to
            // every kind through the destination delivery path.
            "multihoming" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "multihoming applies to server kinds only",
                    });
                }
                multihoming = parse_bool_option("multihoming", value)?;
            }
            "reply_bundling" => {
                reply_bundling = parse_bool_option("reply_bundling", value)?;
            }
            // Plan 297: server TLS to the loopback target (server
            // kinds only; the daemon TLS policy owns identity and
            // trust, so Unix targets cannot apply here — the
            // control surface only builds loopback-TCP targets).
            "use_ssl" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "use_ssl applies to server kinds only",
                    });
                }
                use_ssl = parse_bool_option("use_ssl", value)?;
            }
            "address_helper" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "address_helper applies to HTTP server kinds only",
                    });
                }
                http_policy.address_helper = parse_bool_option("address_helper", value)?;
            }
            "jump_list" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "jump_list applies to HTTP server kinds only",
                    });
                }
                http_policy.jump_list = parse_bool_option("jump_list", value)?;
            }
            "spoofed_host" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "spoofed_host applies to HTTP server kinds only",
                    });
                }
                if !i2pr_service_tunnels::http::valid_spoofed_host(value) {
                    return Err(ControlError::InvalidOption {
                        option: key.clone(),
                        reason: "must be a bounded ASCII DNS hostname",
                    });
                }
                http_policy.spoofed_host = Some(value.clone());
            }
            "block_referers" => {
                if !matches!(
                    kind,
                    ServiceTunnelKind::HttpServer | ServiceTunnelKind::HttpBidirServer
                ) {
                    return Err(ControlError::ContradictoryOptions {
                        name: definition.name.clone(),
                        reason: "block_referers applies to HTTP server kinds only",
                    });
                }
                http_policy.block_referers = parse_bool_option(key, value)?;
            }
            // Plan 334: the three LeaseSet security options are consumed by
            // the ELS2 publication path, not by the traffic specification, so
            // the M10 spec has nothing to project from them. The block's
            // cross-field rule is enforced once, as a whole, in
            // `normalize_definition` after this mask check; re-deriving it
            // here per key would reject a valid combination one key at a time.
            //
            // The mask check below is what scopes the rule to kinds that
            // publish a LeaseSet at all, so it must run first: "this kind has
            // no LeaseSet" is a more fundamental objection than "this mode
            // does not use this secret".
            "encrypt_lease_set" | "leaseset_password" | "leaseset_client_auth" => {
                let publisher_kind = matches!(
                    kind,
                    ServiceTunnelKind::GenericServer
                        | ServiceTunnelKind::IrcServer
                        | ServiceTunnelKind::HttpServer
                        | ServiceTunnelKind::HttpBidirServer
                        | ServiceTunnelKind::StreamrServer
                );
                // Plan 351 Gate 2: a client may also need the lookup secret,
                // but as a *consumer* rather than a publisher — it is what
                // derives the blinded storage key it must look up. That is the
                // same secret and the same option name, so this is symmetric
                // with the server rule above rather than a new option.
                //
                // The other two keys stay refused on clients: `encrypt_lease_set`
                // describes what to publish and `leaseset_client_auth` describes
                // which per-client credentials *this* service will accept. Both
                // are publisher-side, and Plan 351 defers consumer credentials
                // (PSK/DH) entirely, so allowing them here would accept a
                // configuration whose effect is a silent no-op.
                //
                // The pairing rule — this key requires an encrypted target and a
                // delay-open client — needs `destination`, which is parsed later
                // in this loop (`target_destination` sorts after
                // `leaseset_password`). It is enforced after the loop, alongside
                // the `timeouts.delay_open` assignment, so the error names the
                // real rule instead of the mask.
                let consumer_kind = key == "leaseset_password"
                    && matches!(
                        kind,
                        ServiceTunnelKind::GenericClient
                            | ServiceTunnelKind::HttpClient
                            | ServiceTunnelKind::Socks5Client
                            | ServiceTunnelKind::IrcClient
                            | ServiceTunnelKind::ConnectClient
                            | ServiceTunnelKind::SocksIrc
                    );
                if !publisher_kind && !consumer_kind {
                    return Err(ControlError::UnsupportedOption(key.clone()));
                }
            }
            // Plan 342: the seven outproxy fields are consumed as a whole,
            // not one key at a time. They carry no per-key `match` arm on
            // purpose: a key that parsed on its own is exactly the inert
            // acceptance the whole-block rule forbids, so the block is
            // resolved once after the loop and any stray key that reached
            // here has already been refused by `parse_outproxy_block`.
            //
            // Falling through rather than binding a variable is deliberate:
            // these arms bind nothing, so the block parser below re-reads the
            // values from `definition.options` in one place.
            key if SUPPORTED_342_OPTIONS.contains(&key) => {}
            // Plan 380: the consumer client credential is consumed whole, by
            // the sync pass and then by the resolve path, and the M10 traffic
            // spec has nothing to project from it. Like the outproxy block it
            // binds no variable here; its cross-field rule is enforced once,
            // after this loop, because `target_destination` sorts after it.
            key if SUPPORTED_380_OPTIONS.contains(&key) => {}
            other => {
                // Secret-classified keys are rejected here even though
                // they never reach storage: belt and suspenders against
                // secret persistence. Blocked and corrective-pending
                // keys name their owning plan.
                return Err(ControlError::UnsupportedOption(rejected_option_reason(
                    definition.tunnel_type,
                    other,
                )));
            }
        }
    }
    if let Some(mode) = proposal_new_dest {
        let persistent_from_mode = mode == 2;
        if persistent_client_key_declared.is_some_and(|declared| declared != persistent_from_mode) {
            return Err(ControlError::ContradictoryOptions {
                name: definition.name.clone(),
                reason: "NewDest and PersistentClientKey request conflicting identity policies",
            });
        }
        if mode == 1 && shared_client {
            return Err(ControlError::ContradictoryOptions {
                name: definition.name.clone(),
                reason: "NewDest=1 cannot rotate a shared Destination group independently",
            });
        }
        if mode == 1 {
            if !idle_close {
                return Err(ControlError::ContradictoryOptions {
                    name: definition.name.clone(),
                    reason: "NewDest=1 requires Close:true to resume with a fresh Destination",
                });
            }
            rotate_destination_on_idle = true;
        }
        persistent_client_key = persistent_from_mode;
    }
    if priv_key_file.is_some() {
        if shared_client {
            return Err(ControlError::ContradictoryOptions {
                name: definition.name.clone(),
                reason: "PrivKeyFile selects a per-service key reference and cannot combine with Shared",
            });
        }
        if persistent_client_key_declared == Some(false)
            || proposal_new_dest.is_some_and(|mode| mode != 2)
        {
            return Err(ControlError::ContradictoryOptions {
                name: definition.name.clone(),
                reason: "PrivKeyFile requires persistent identity semantics (PersistentClientKey:true or NewDest:2)",
            });
        }
    }
    if let Some(mode) = proposal_access_option.as_deref() {
        if let Some(value) = proposal_access_list {
            if mode == "allow" {
                access_allow_sources.push(("access_list".to_owned(), value));
            } else {
                access_deny_sources.push(("access_list".to_owned(), value));
            }
        } else if filter_file_path.is_none() {
            return Err(ControlError::ContradictoryOptions {
                name: definition.name.clone(),
                reason: "AccessOption requires AccessList",
            });
        }
    } else if let Some(value) = proposal_access_list {
        access_allow_sources.push(("access_list".to_owned(), value));
    }
    if let Some(path) = filter_file_path.as_deref() {
        let root = filter_root.ok_or(ControlError::InvalidOption {
            option: "filter_file_path".to_owned(),
            reason: "FilterFilePath requires the owned control filter directory",
        })?;
        let contents = read_confined_filter_file(root, path).map_err(|reason| {
            ControlError::InvalidOption {
                option: "filter_file_path".to_owned(),
                reason,
            }
        })?;
        let entries = contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join(",");
        if entries.is_empty() {
            return Err(ControlError::InvalidOption {
                option: "filter_file_path".to_owned(),
                reason: "FilterFilePath must contain at least one peer hash",
            });
        }
        let source = ("filter_file_path".to_owned(), entries);
        if proposal_access_option.as_deref() == Some("deny") {
            access_deny_sources.push(source);
        } else {
            access_allow_sources.push(source);
        }
    }
    if idle_close_timeout_ms.is_some() && !idle_close {
        return Err(ControlError::ContradictoryOptions {
            name: definition.name.clone(),
            reason: "CloseTime requires Close:true",
        });
    }
    if http_policy.block_user_agents && http_policy.user_agents.is_empty() {
        return Err(ControlError::InvalidOption {
            option: "block_user_agents".to_owned(),
            reason: "BlockUserAgents requires at least one UserAgents rule",
        });
    }
    if (idle_reduce_timeout_ms.is_some() || idle_reduce_count.is_some()) && !idle_reduce {
        return Err(ControlError::ContradictoryOptions {
            name: definition.name.clone(),
            reason: "ReduceTime and ReduceCount require Reduce:true",
        });
    }
    // Per-kind required-field completion with real defaults.
    // Server targets require both halves explicitly: no silent default
    // for where tunneled traffic exits.
    // Plan 292: resolve shaping. Per-direction lengths must agree:
    // the pool uses a single hop length and the control plane must
    // not silently drop a requested value. Per-key ranges were
    // enforced at parse time, so the literal below is in-bounds
    // (re-checked by the final spec validation).
    let inbound_length = inbound_length.or(symmetric_length);
    let outbound_length = outbound_length.or(symmetric_length);
    if let (Some(inbound), Some(outbound)) = (inbound_length, outbound_length)
        && inbound != outbound
    {
        return Err(ControlError::ContradictoryOptions {
            name: definition.name.clone(),
            reason: "inbound_length and outbound_length differ; the pool uses a single hop length",
        });
    }
    let shaping = TunnelShaping {
        inbound_quantity: inbound_quantity.or(symmetric_quantity).unwrap_or(2),
        outbound_quantity: outbound_quantity.or(symmetric_quantity).unwrap_or(2),
        length_hops: inbound_length.or(outbound_length).unwrap_or(2),
        backup_quantity: backup_quantity.unwrap_or(0),
        length_variance: length_variance.unwrap_or(0),
    };
    // Plan 296: each direction's quantity plus backup must fit the
    // pool directional maximum (checked again by spec validation;
    // named here so the failure cites the option pair).
    if shaping
        .inbound_quantity
        .saturating_add(shaping.backup_quantity)
        > MAX_EFFECTIVE_DIRECTION_TUNNELS
        || shaping
            .outbound_quantity
            .saturating_add(shaping.backup_quantity)
            > MAX_EFFECTIVE_DIRECTION_TUNNELS
    {
        return Err(ControlError::ContradictoryOptions {
            name: definition.name.clone(),
            reason: "quantity plus tunnel_backup_quantity exceeds the pool directional maximum of 8",
        });
    }
    // Plan 292: resolve the idle policy. Flags without a deadline
    // take the documented default; a deadline without flags flows
    // to spec validation, which rejects it as inert with the
    // service id. Ranges were enforced per key at parse time.
    let has_proposal_idle_time =
        idle_close_timeout_ms.is_some() || idle_reduce_timeout_ms.is_some();
    let idle_timeout_ms = match (
        idle_timeout_ms,
        idle_close || idle_new_dest || idle_reduce,
        has_proposal_idle_time,
    ) {
        (None, true, false) => Some(DEFAULT_IDLE_TIMEOUT_MS),
        (timeout, _, _) => timeout,
    };
    let idle = i2pr_service_tunnels::IdlePolicy {
        timeout_ms: idle_timeout_ms,
        close_on_idle: idle_close,
        new_dest_on_idle: idle_new_dest,
        rotate_destination_on_idle,
        reduce_on_idle: idle_reduce,
        close_timeout_ms: idle_close_timeout_ms,
        reduce_timeout_ms: idle_reduce_timeout_ms,
        reduce_count: idle_reduce_count,
    };
    // Plan 292: resolve proxy credentials and the access policy.
    // Failures name the offending key, never the value.
    if let Some(enabled) = proxy_auth_declared
        && enabled != (proxy_username.is_some() && proxy_password.is_some())
    {
        return Err(ControlError::ContradictoryOptions {
            name: definition.name.clone(),
            reason: "ProxyAuth must match the complete ProxyUsername/ProxyPassword pair",
        });
    }
    let proxy_auth = proxy_credentials_for(kind, proxy_username, proxy_password)?;
    let mut allow = Vec::new();
    for (key, value) in &access_allow_sources {
        let parsed = ServerAccessPolicy::parse(&[value.as_str()], &[]).map_err(|_| {
            ControlError::InvalidOption {
                option: key.clone(),
                reason: "access entries must be canonical base32 destination hashes",
            }
        })?;
        allow.extend(parsed.allow);
    }
    let mut deny = Vec::new();
    for (key, value) in &access_deny_sources {
        let parsed = ServerAccessPolicy::parse(&[], &[value.as_str()]).map_err(|_| {
            ControlError::InvalidOption {
                option: key.clone(),
                reason: "access entries must be canonical base32 destination hashes",
            }
        })?;
        deny.extend(parsed.deny);
    }
    let access = ServerAccessPolicy {
        allow,
        deny,
        connection_rates: connection_rates
            .validate()
            .map_err(|_| ControlError::InvalidOption {
                option: "ClientPerMinute".to_owned(),
                reason: "server connection rate is outside the bounded range",
            })?,
    };
    // Plan 296: multihoming needs two configured targets, but the
    // control surface carries a singular target (no multi-target
    // wire key exists and none may be added here). The key parses
    // through its real owner arm and kind gate above; the
    // two-target minimum rejects it here with a named reason while
    // the dial-selection owner serves multi-target specs built
    // through other surfaces.
    if multihoming {
        return Err(ControlError::ContradictoryOptions {
            name: definition.name.clone(),
            reason: "multihoming requires at least two configured targets",
        });
    }
    let target = match kind {
        ServiceTunnelKind::GenericServer
        | ServiceTunnelKind::IrcServer
        | ServiceTunnelKind::HttpServer
        | ServiceTunnelKind::HttpBidirServer => Some(match (target_host, target_port) {
            (Some(host), Some(port)) => {
                ServerTarget::LoopbackTcp(std::net::SocketAddr::new(host, port))
            }
            _ => {
                return Err(ControlError::InvalidRequest(
                    "server kinds require target_host and target_port",
                ));
            }
        }),
        _ => None,
    };
    let listener = match kind {
        ServiceTunnelKind::GenericClient
        | ServiceTunnelKind::HttpClient
        | ServiceTunnelKind::Socks5Client
        | ServiceTunnelKind::IrcClient
        | ServiceTunnelKind::ConnectClient
        | ServiceTunnelKind::SocksIrc
        | ServiceTunnelKind::HttpBidirServer => Some(
            LocalListenerSpec::parse(
                listen_host.unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
                listen_port,
            )
            .map_err(|_| ControlError::InvalidOption {
                option: "listen_host".to_owned(),
                reason: "listener must be loopback",
            })?,
        ),
        ServiceTunnelKind::GenericServer
        | ServiceTunnelKind::IrcServer
        | ServiceTunnelKind::HttpServer
        | ServiceTunnelKind::StreamrClient
        | ServiceTunnelKind::StreamrServer => None,
    };
    if listener.is_some() && destination.is_none() {
        // The bidirectional profile is the exception: its client
        // half resolves per-request destinations like http-client
        // and only the server half publishes, so no remote
        // destination reference is carried.
        if !matches!(kind, ServiceTunnelKind::HttpBidirServer) {
            return Err(ControlError::InvalidRequest(
                "client kinds require the target_destination option",
            ));
        }
    }
    // Plan 342: the route policy rides on the spec so the runtime request
    // paths can reach the provider. Re-derived from `definition.options`
    // rather than carried from normalize, which is why the same parse is
    // safe to run twice: on the second pass `outproxy_password` holds the
    // sealed stored form, and the parser reads only its presence.
    //
    // The parsed credential is dropped immediately. The policy is all a
    // spec needs, and a `ServiceTunnelSpec` is `Debug` and `Clone`.
    let outproxy = crate::outproxy_options::parse_outproxy_block(
        &definition.name,
        definition.tunnel_type.name(),
        &definition.options,
    )?
    .map(|block| block.config);

    let (http_options, socks5_options, irc_options, connect_options, streamr_options) = match kind {
        ServiceTunnelKind::HttpClient => {
            let mut options = i2pr_service_tunnels::HttpClientOptions::defaults();
            options.proxy_auth = proxy_auth;
            options.outproxy = outproxy.clone();
            apply_proposal_http_filters(
                &mut options,
                allow_user_agent,
                allow_referer,
                allow_accept,
                allow_internal_ssl,
            );
            (Some(options), None, None, None, None)
        }
        ServiceTunnelKind::HttpBidirServer => {
            let mut options = i2pr_service_tunnels::HttpClientOptions::defaults();
            apply_proposal_http_filters(
                &mut options,
                allow_user_agent,
                allow_referer,
                allow_accept,
                allow_internal_ssl,
            );
            (Some(options), None, None, None, None)
        }
        ServiceTunnelKind::Socks5Client => {
            let mut options = i2pr_service_tunnels::Socks5ClientOptions::defaults();
            options.proxy_auth = proxy_auth;
            options.outproxy = outproxy.clone();
            (None, Some(options), None, None, None)
        }
        ServiceTunnelKind::SocksIrc => {
            let mut socks = i2pr_service_tunnels::Socks5ClientOptions::defaults();
            socks.proxy_auth = proxy_auth;
            socks.outproxy = outproxy.clone();
            (
                None,
                Some(socks),
                Some(i2pr_service_tunnels::IrcClientOptions::defaults()),
                None,
                None,
            )
        }
        ServiceTunnelKind::IrcClient => (
            None,
            None,
            Some(i2pr_service_tunnels::IrcClientOptions::defaults()),
            None,
            None,
        ),
        ServiceTunnelKind::ConnectClient => {
            let mut options = i2pr_service_tunnels::ConnectClientOptions::defaults();
            options.proxy_auth = proxy_auth;
            options.outproxy = outproxy.clone();
            (None, None, None, Some(options), None)
        }
        // Plan 291: Streamr endpoints are explicit (no silent
        // default for where media enters or exits); cadence
        // policy defaults to the freeze and honors supplied
        // overrides through spec validation.
        ServiceTunnelKind::StreamrClient | ServiceTunnelKind::StreamrServer => {
            let local_udp = match (local_udp_host, local_udp_port) {
                (Some(host), Some(port)) => Some(std::net::SocketAddr::new(host, port)),
                _ => {
                    return Err(ControlError::InvalidRequest(
                        "streamr kinds require local_udp_host and local_udp_port",
                    ));
                }
            };
            let options = i2pr_service_tunnels::StreamrOptions {
                local_udp,
                // Plan 292: the redirect host pairs with the
                // local media port; the loopback shape was
                // enforced at parse time and re-checked by
                // spec validation.
                remote_sink: match (kind, remote_udp_host, local_udp_port) {
                    (ServiceTunnelKind::StreamrClient, Some(host), Some(port)) => {
                        Some(std::net::SocketAddr::new(host, port))
                    }
                    _ => None,
                },
                target_i2p_port,
                subscribe_interval_ms: subscribe_interval_ms
                    .unwrap_or(i2pr_service_tunnels::DEFAULT_SUBSCRIBE_INTERVAL_MS),
                subscription_expiry_ms: subscription_expiry_ms
                    .unwrap_or(i2pr_service_tunnels::DEFAULT_SUBSCRIPTION_EXPIRY_MS),
                max_subscribers: max_subscribers
                    .unwrap_or(i2pr_service_tunnels::DEFAULT_MAX_SUBSCRIBERS),
                payload_limit_bytes: payload_limit_bytes
                    .unwrap_or(i2pr_service_tunnels::DEFAULT_PAYLOAD_LIMIT_BYTES),
            };
            (None, None, None, None, Some(options))
        }
        _ => (None, None, None, None, None),
    };
    let mut timeouts = i2pr_service_tunnels::ServiceTimeouts::defaults();
    if connect_delay {
        timeouts.streaming_connect_delay_ms =
            Some(i2pr_service_tunnels::DEFAULT_STREAMING_CONNECT_DELAY_MS);
    }
    timeouts.delay_open = delay_open;
    // Plan 351 Gate 2 — the consumer-side counterpart of the mask rule that
    // let `leaseset_password` through for a client kind. Both cross-field
    // conditions are checked here rather than inside the option loop because
    // `target_destination` sorts after `leaseset_password`, so `destination` is
    // only known once the loop has finished.
    //
    // `ServiceTunnelSpec::validate` enforces the same pairing. This duplicate
    // exists so the operator gets a named rule instead of a generic spec
    // error; both must agree, and the guard script
    // `scripts/check-encrypted-service-consumer-caller.sh` fails if either is
    // removed without the other.
    if definition.options.contains_key("leaseset_password")
        && matches!(
            kind,
            ServiceTunnelKind::GenericClient
                | ServiceTunnelKind::HttpClient
                | ServiceTunnelKind::Socks5Client
                | ServiceTunnelKind::IrcClient
                | ServiceTunnelKind::ConnectClient
                | ServiceTunnelKind::SocksIrc
        )
        && !matches!(destination, Some(DestinationRef::EncryptedService { .. }))
    {
        return Err(ControlError::ContradictoryOptions {
            name: definition.name.clone(),
            reason: "leaseset_password on a client tunnel requires target_destination to be an encrypted-service address (.b33.i2p)",
        });
    }
    // Plan 380 — the consumer credential's own pairing rule, and a *stricter*
    // one than the lookup secret's above.
    //
    // A `leaseset_password` with an ordinary target is refused too, but the
    // objection there is that the secret is useless. A
    // `leaseset_client_credential` with an ordinary target is worse than
    // useless: it would be sealed, stored, restarted, and then never read,
    // because no `.b32` target can ever present a layer-1 authorization block
    // to refuse. So it is refused outright rather than parked.
    //
    // The rule is also unconditional on tunnel kind, unlike the lookup secret's
    // rule which is scoped to client kinds. A credential is only ever consumed
    // by the encrypted-service consumer path, and that path requires
    // `delay_open` (Gate 1), so a credential on any other kind is a
    // configuration that cannot be honoured.
    if definition
        .options
        .contains_key(crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION)
        && !matches!(destination, Some(DestinationRef::EncryptedService { .. }))
    {
        return Err(ControlError::ContradictoryOptions {
            name: definition.name.clone(),
            reason: "leaseset_client_credential requires target_destination to be an encrypted-service address (.b33.i2p)",
        });
    }
    let mut destination_policy = if let Some(key_reference) = priv_key_file {
        DestinationPolicy::KeyReference(key_reference)
    } else {
        match (shared_client, persistent_client_key) {
            (true, true) => DestinationPolicy::PersistentSharedClientGroup(
                DestinationGroupId::parse("i2pcontrol-shared-client")
                    .map_err(|_| ControlError::InvalidRequest("shared group id is invalid"))?,
            ),
            (true, false) => DestinationPolicy::SharedClientGroup(
                DestinationGroupId::parse("i2pcontrol-shared-client")
                    .map_err(|_| ControlError::InvalidRequest("shared group id is invalid"))?,
            ),
            (false, true) => DestinationPolicy::PersistentClient,
            (false, false) => DestinationPolicy::Dedicated,
        }
    };
    if sig_type_selected || enc_type_selected {
        destination_policy = destination_policy
            .with_crypto(i2pr_service_tunnels::DestinationCryptoPolicy::default());
    }
    let spec = ServiceTunnelSpec {
        id,
        kind,
        enabled: false,
        listener,
        target,
        targets: Vec::new(),
        destination,
        policy: destination_policy,
        inbound_port: None,
        max_connections,
        max_buffered_bytes_per_direction: 65_536,
        timeouts,
        shaping,
        streaming_interactive: profile_interactive,
        idle,
        access,
        unique_local_address,
        multihoming,
        reply_bundling,
        use_ssl,
        http_policy,
        http_options,
        socks5_options,
        irc_options,
        connect_options,
        streamr_options,
    };
    spec.validate()
        .map_err(|error| ControlError::InvalidRequest(static_spec_reason(error)))?;
    Ok(spec)
}

/// Converts a spec validation failure into a static reason (never
/// echoes spec values).
fn static_spec_reason(error: i2pr_service_tunnels::ServiceTunnelError) -> &'static str {
    use i2pr_service_tunnels::ServiceTunnelError as E;
    match error {
        E::ExceedsCeiling { .. } => "spec exceeds a service ceiling",
        E::ContradictoryOptions { .. } => "spec options contradict the kind",
        E::DuplicateId { .. } => "duplicate service id",
        E::DuplicateListener { .. } => "duplicate listener",
        E::InvalidKind { .. } => "invalid service kind",
        E::InvalidListener { .. } => "invalid listener",
        E::InvalidTarget { .. } => "invalid server target",
        E::InvalidDestinationRef { .. } => "invalid destination reference",
        E::InvalidId { .. } => "invalid service id",
        _ => "spec validation failed",
    }
}

fn apply_proposal_http_filters(
    options: &mut i2pr_service_tunnels::HttpClientOptions,
    allow_user_agent: Option<bool>,
    allow_referer: Option<bool>,
    allow_accept: Option<bool>,
    allow_internal_ssl: Option<bool>,
) {
    if let Some(allow) = allow_user_agent {
        options.privacy.user_agent = if allow {
            i2pr_service_tunnels::UserAgentPolicy::Keep
        } else {
            i2pr_service_tunnels::UserAgentPolicy::ReplaceStable
        };
    }
    if let Some(allow) = allow_referer {
        options.privacy.strip_referer = !allow;
    }
    if let Some(allow) = allow_accept {
        options.privacy.allow_accept = allow;
    }
    options.allow_internal_ssl = allow_internal_ssl.unwrap_or(false);
}

/// Validates one control definition's options against the 289 subset and
/// builds the normalized definition. Unknown-universe keys cannot arrive
/// here (the envelope rejects them); known-but-unsupported keys fail.
/// Names why an ownerless option key is rejected, pointing at the
/// owning plan for blocked and corrective-pending matrix cells. Only
/// inventory keys reach here (the request envelope rejects unknown
/// keys), so echoing the key name leaks no value.
fn rejected_option_reason(tunnel_type: TunnelType, key: &str) -> String {
    match disposition_for(tunnel_type.canonical_index(), key) {
        Some(CellDisposition::ExplicitIncompatibility { limitation }) => {
            format!(
                "{key} is not supported ({limitation}; Plan 293 determination, carried by Plan 295)"
            )
        }
        Some(CellDisposition::CorrectivePending { plan, reason }) => {
            format!("{key} is not yet supported (Plan {plan} owns {reason})")
        }
        _ => key.to_owned(),
    }
}

/// Normalizes a definition without a secret owner.
///
/// Every caller that can supply an outbound credential uses
/// [`normalize_definition_with_filter_root`], which takes the store. This
/// wrapper passes [`NoOutboundSecrets`], the fail-closed default: a surface
/// with no installed owner refuses a configured credential rather than
/// storing it in the clear. There is no daemon-TOML outproxy block today, so
/// the wrapper's only users are tests and the simple crate-level entry point.
/// Seals a resolved outproxy credential into `persisted`, replacing the
/// plaintext before the definition can reach a generation file.
///
/// Split out so the plaintext's lifetime is one function body rather than a
/// block that could grow. `credential` is consumed; `persisted` is mutated in
/// place only on success, so a failed seal leaves the caller's map untouched
/// and the caller returns an error rather than continuing.
fn seal_outproxy_credential(
    name: &str,
    store: &dyn RouterSecretOwner,
    credential: Option<zeroize::Zeroizing<String>>,
    persisted: &mut BTreeMap<String, String>,
) -> Result<(), ControlError> {
    let Some(credential) = credential else {
        return Ok(());
    };
    // Idempotence, corrected forward by Plan 380.
    //
    // `edit` merges the stored options with the request's, so this function runs again with the
    // value that is already this owner's stored form. Before this arm it re-sealed that ciphertext,
    // so **every edit of an outproxy tunnel carrying an OutproxyPassword silently destroyed the
    // credential**: the stored form became sealed-twice, and opening it once returned the previous
    // stored form *as text*, which the provider then presented to the outproxy as the password. The
    // outproxy answers 407 and there is nothing in any status surface to say why.
    //
    // Plan 342's rows could not see this because they exercised a generation round trip, which
    // never re-runs the seal step, and no Plan 342 or Plan 376 row edited an outproxy tunnel.
    // `plan342_sealed_block_survives_a_generation_round_trip` carried the comment "so an untouched
    // credential survives an edit" — the first clause was true and the second was untested. It is
    // tested now, by `plan380_an_unrelated_edit_leaves_the_sealed_credential_unchanged`.
    //
    // Re-sealing would also be wrong on its own terms: a fresh nonce would change the stored bytes
    // after an edit that changed nothing about the credential.
    if credential.starts_with(i2pr_service_tunnels::outbound_secret::OUTBOUND_SECRET_MARKER) {
        return i2pr_service_tunnels::outbound_secret::validate_stored_form(&credential)
            .map_err(|_| ControlError::OutproxyBlockRejected(format!(
                "{name}: the stored OutproxyPassword form is malformed; the tunnel was not created \
                 and no listener or destination was allocated"
            )));
    }
    // Plan 342 invariant 3, enforced here and nowhere else: a credential that
    // cannot be sealed fails the transaction before any listener or
    // destination is allocated. Falling back to plaintext storage is not an
    // option — it is the specific thing this owner exists to prevent.
    if !store.is_available() {
        return Err(ControlError::OutproxyBlockRejected(format!(
            "{name}: no outbound secret owner is installed, so OutproxyPassword cannot be \
             sealed; the tunnel was not created and no listener or destination was allocated"
        )));
    }
    let secret = i2pr_service_tunnels::outbound_secret::OutboundSecret::new(credential.as_str())
        .map_err(|_| {
            ControlError::OutproxyBlockRejected(format!(
                "{name}: OutproxyPassword exceeds the bounded credential length"
            ))
        })?;
    let sealed = store.seal(&secret).map_err(|_| {
        ControlError::OutproxyBlockRejected(format!(
            "{name}: OutproxyPassword could not be sealed by the outbound secret owner"
        ))
    })?;
    persisted.insert(
        crate::outproxy_options::OUTPROXY_PASSWORD_KEY.to_owned(),
        sealed,
    );
    Ok(())
}

/// Plan 380 — seals an ELS2 consumer client credential into `persisted`,
/// replacing the plaintext before the definition can reach a generation file.
///
/// The grammar is checked here rather than at the sync pass, so a malformed
/// credential is refused by the same transaction that would have stored it —
/// not accepted now and discovered broken on the next restart.
///
/// Only the **stored form** is produced here. The `SealedEncryptedTargetCredential`
/// that owns it — and therefore the `Arc` to the secret owner that can open it
/// — is built later, in the sync pass, where the composition root's `Arc` is
/// in hand. Sealing through a borrowed capability and adopting the stored form
/// through the owned one keeps exactly one place that holds an `Arc`, which is
/// the property Plan 341's "derive once" discipline is actually about.
///
/// Every reason is a literal naming the rule, never the offending value. That
/// matters more here than for the outproxy credential: the value is raw key
/// material, and an error string that echoed it would put a pre-shared key into
/// a client reply and a log line in one step.
fn seal_encrypted_target_credential(
    name: &str,
    plaintext: &str,
    store: &dyn RouterSecretOwner,
    persisted: &mut BTreeMap<String, String>,
) -> Result<(), ControlError> {
    let option = || crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION.to_owned();

    // Idempotence, which a sealing step in a merge-capable normalize path must have.
    //
    // `edit` merges the stored options with the request's, so this function is called again with
    // a value that is *already* this owner's stored form. Without this arm the seal step would try
    // to parse its own ciphertext as a plaintext credential and every edit of a credential-bearing
    // service would fail. Re-sealing instead would be no better: it would churn the nonce, so the
    // stored bytes would differ after an unrelated edit and a generation round trip would stop
    // being byte-stable.
    //
    // The framing is still checked, so a value that merely *begins* like a stored form cannot slip
    // past — it has to be a frame this owner could actually have produced.
    if plaintext.starts_with(i2pr_service_tunnels::outbound_secret::ELS2_CONSUMER_CREDENTIAL_MARKER)
    {
        return validate_els2_credential_form(plaintext).map_err(|_| ControlError::InvalidOption {
            option: option(),
            reason: "client credential stored form is malformed",
        });
    }
    // Parsed for its grammar, then dropped before this function returns. The
    // plaintext's lifetime is one call, not the life of the definition.
    let credential = EncryptedTargetCredential::parse(plaintext).map_err(|error| {
        ControlError::InvalidOption {
            option: option(),
            reason: match error {
                EncryptedTargetCredentialError::OverBound => {
                    "client credential exceeds the bounded length"
                }
                EncryptedTargetCredentialError::UnknownScheme => {
                    "client credential scheme must be psk: or dh:"
                }
                EncryptedTargetCredentialError::MalformedKey => {
                    "client credential key must be exactly 64 lowercase hex characters"
                }
                EncryptedTargetCredentialError::NoSecretOwner
                | EncryptedTargetCredentialError::CannotOpen => {
                    "client credential could not be sealed by the outbound secret owner"
                }
            },
        }
    })?;
    // Sealed from the re-rendered canonical spelling rather than from the
    // client's bytes. `parse` rejects every non-canonical spelling, so the
    // render is the identity — but stating it means a future lenient parser
    // still could not seal a value that would not parse back identically.
    let sealed = store
        .seal_credential(
            &OutboundSecret::new(credential.to_option_value().as_str()).map_err(|_| {
                ControlError::InvalidOption {
                    option: option(),
                    reason: "client credential exceeds the bounded length",
                }
            })?,
        )
        .map_err(|_| ControlError::InvalidOption {
            option: option(),
            reason: "client credential could not be sealed by the outbound secret owner",
        })?;
    // Prove the framing this owner produced before it becomes durable state.
    // A failure here means the owner is not the one that sealed the value, and
    // storing it would defer that to the first resolve.
    if validate_els2_credential_form(&sealed).is_err() {
        return Err(ControlError::InvalidOption {
            option: option(),
            reason: "client credential could not be sealed by the outbound secret owner",
        });
    }
    persisted.insert(option(), sealed);
    let _ = name;
    Ok(())
}

pub fn normalize_definition(
    name: &str,
    tunnel_type: TunnelType,
    options: &BTreeMap<String, String>,
    start_on_load: bool,
) -> Result<ControlDefinition, ControlError> {
    normalize_definition_with_filter_root(
        name,
        tunnel_type,
        options,
        start_on_load,
        None,
        &NoOutboundSecrets,
    )
}

/// Normalizes a definition, validates file-backed options against an
/// explicitly owned filter root, and resolves the Plan 342 outproxy block
/// through `store`.
///
/// `store` is the composition root's single
/// `i2pr_service_tunnels::outbound_secret::OutboundSecretStore`.
/// It is consulted for exactly one thing: sealing `outproxy_password` into
/// its stored form before the definition reaches a generation file. A store
/// that cannot seal makes creation and edit fail here — before any listener
/// or destination is allocated — which is Plan 342 invariant 3's requirement
/// stated as a property of the code path rather than a promise.
pub fn normalize_definition_with_filter_root(
    name: &str,
    tunnel_type: TunnelType,
    options: &BTreeMap<String, String>,
    start_on_load: bool,
    filter_root: Option<&Path>,
    store: &dyn RouterSecretOwner,
) -> Result<ControlDefinition, ControlError> {
    validate_tunnel_name(name).map_err(|_| ControlError::InvalidRequest("invalid tunnel name"))?;
    if !tunnel_type.has_plan291_backend() {
        return Err(ControlError::UnsupportedType(tunnel_type.name().to_owned()));
    }
    for key in options.keys() {
        if !SUPPORTED_289_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_291_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_292_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_296_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_297_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_323_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_324_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_334_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_342_OPTIONS.contains(&key.as_str())
            && !SUPPORTED_380_OPTIONS.contains(&key.as_str())
        {
            return Err(ControlError::UnsupportedOption(rejected_option_reason(
                tunnel_type,
                key,
            )));
        }
    }
    // Plan 292: scrub proxy plaintext before it can reach the
    // definition mirror or the store: the persisted password is
    // always the marked verifier. Both halves are required
    // together; a marked value round-trips through the stored form
    // so untouched credentials survive edits.
    let mut persisted = options.clone();
    match (
        persisted.get("proxy_username"),
        persisted.get("proxy_password"),
    ) {
        (Some(_), Some(_)) => {
            let kind = map_tunnel_type(tunnel_type)?;
            let username = persisted
                .get("proxy_username")
                .expect("username present")
                .clone();
            let password = persisted
                .get("proxy_password")
                .expect("password present")
                .clone();
            let credentials =
                proxy_credentials_for(kind, Some(username), Some(password))?.expect("both given");
            persisted.insert("proxy_password".to_owned(), credentials.stored_form());
        }
        (None, None) => {}
        _ => {
            return Err(ControlError::InvalidOption {
                option: "proxy_password".to_owned(),
                reason: "proxy_username and proxy_password are both required",
            });
        }
    }
    // Plan 342: resolve the outproxy block as a whole, then seal its
    // credential before the definition can reach a generation file.
    //
    // The order inside this arm is load-bearing. The block rule runs first,
    // so a partial or malformed block is refused by name before any key
    // material is involved; then the credential is sealed; then the
    // plaintext `OutboundSecret` is dropped at the end of the arm. There is
    // no point at which the plaintext is in a value that outlives the arm.
    if let Some(block) =
        crate::outproxy_options::parse_outproxy_block(name, tunnel_type.name(), options)?
    {
        seal_outproxy_credential(name, store, block.credential, &mut persisted)?;
        // `block` drops here. Its `credential` was moved into
        // `seal_outproxy_credential`, and `block.config` is a policy value
        // the spec build re-derives from `persisted` rather than carrying
        // across, so nothing observable escapes this arm.
    }
    // Plan 380: seal the ELS2 consumer client credential, by the same rule and
    // for the same reason as the outproxy credential above — a value that
    // cannot be sealed fails the transaction before any listener or
    // destination is allocated, and falling back to plaintext storage is never
    // an option.
    //
    // It runs after the outproxy arm and before the spec build so that the
    // grammar is checked before the pairing rule fires in `build_control_spec`
    // and before any allocation. The plaintext exists only between the parse
    // and the seal, both of which are in this function body.
    if let Some(plaintext) = options.get(crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION)
    {
        seal_encrypted_target_credential(name, plaintext, store, &mut persisted)?;
    }
    let mut explicit_start_on_load = start_on_load;
    if let Some(value) = persisted.get("start_on_load") {
        explicit_start_on_load = parse_bool_option("start_on_load", value)?;
    }
    let definition = ControlDefinition {
        name: name.to_owned(),
        tunnel_type,
        options: persisted,
        start_on_load: explicit_start_on_load,
    };
    // Validate the full mapping now, before any side effect. The
    // returned spec is discarded; the coordinator rebuilds it.
    let _ = build_control_spec_with_filter_root(&definition, filter_root)?;
    // Plan 334: the LeaseSet security block is a cross-field constraint, so it
    // is validated as a whole block, after the per-key mask check inside
    // `build_control_spec` has already scoped these keys to kinds that publish
    // a LeaseSet. The identical rule also ran in the request envelope; running
    // it again here is what makes a merged `create`+stored-`edit` candidate, or
    // a reloaded stored definition, obey the same law as a fresh request.
    let _ = definition.lease_set_security()?;
    Ok(definition)
}

/// One supervised control runtime: the single M10 manager holding only
/// control-owned specs, plus the coordinator state around it.
///
/// All mutations serialize on `op_lock` (a Tokio mutex: it is held
/// across reconcile awaits, and every control future stays `Send`).
/// Reads (`get`, status) never take the op lock: they observe
/// point-in-time snapshots with explicit transitioning flags.
///
/// Transaction order per mutation: validate the complete candidate
/// before any side effect → update the in-memory mirror → stage the
/// persistence generation → reconcile the manager → start supervisors
/// for added/replaced runtimes → publish the durable generation →
/// verify agreement. A supervisor failure reconciles back without
/// publishing; a publish failure reconciles back under a hard deadline.
/// Staged-but-unpublished files are orphans; retention and startup
/// cleanup remove them.
pub struct TunnelControlState {
    /// Durable generation store.
    store: ControlStore,
    /// Startup-owned inventory (inspectable, never mutated).
    startup: ServiceTunnelSet,
    /// The one M10 manager (Plan 337: shared with the product layer, not
    /// control-only).
    manager: Arc<ServiceTunnelManager>,
    /// Plan 342: the outbound-credential owner, shared with the runtime.
    ///
    /// Held as the runtime-neutral `Arc<dyn OutboundSecretStore>` trait, not
    /// the concrete router-bound type, so this layer can seal and open
    /// without holding key material itself. The composition root derives it
    /// once from the router's persisted signing seed through a
    /// closure-based accessor and hands the same `Arc` to both this state
    /// and each tunnel runtime — one owner, two consumers, no second key.
    ///
    /// `NoOutboundSecrets` is the fail-closed default: a control plane
    /// without an installed owner refuses every credential rather than
    /// silently degrading to an unauthenticated request.
    outbound_secrets: Arc<dyn RouterSecretOwner>,
    /// Serializes all mutations (same-name and cross-name).
    op_lock: tokio::sync::Mutex<()>,
    /// Current durable intent mirror (rebuilt from the store at startup).
    definitions: Mutex<BTreeMap<String, ControlDefinition>>,
    /// Names with current running intent.
    running: Mutex<BTreeSet<String>>,
    /// Names with live supervisors per manager generation.
    supervised: Mutex<HashMap<String, u64>>,
    /// Names with a recorded transition failure (bounded static reasons).
    failed: Mutex<HashMap<String, &'static str>>,
    /// Names with a transition in flight (observed as starting/stopping).
    transitioning: Mutex<BTreeSet<String>>,
    /// Supervisor scope installed at startup (None before startup).
    children: Mutex<Option<i2pr_runtime::ChildScope>>,
    /// Cancellation installed at startup (None before startup).
    cancellation: Mutex<Option<i2pr_runtime::CancellationToken>>,
}

/// One applied Plan 292 idle-sweep action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdleSweepApplied {
    /// Service id the action applied to.
    pub spec_id: String,
    /// Action label (`close`, `new-destination`, `rebuild-pools`, `reduce-pools`).
    pub action: &'static str,
}

/// Plan 292 idle-sweep interval. Per-tunnel deadlines gate action;
/// the tick itself carries no policy.
pub const IDLE_SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// Serializes the Plan 297 TLS state object for one control-owned
/// tunnel, or `None` when the definition does not enable `use_ssl`
/// (existing output shapes stay unchanged). The object carries the
/// verification mode, the provisioned identity expiry (rotation
/// observability), and the per-runtime handshake counters — never
/// key material, pins, or certificate bytes.
fn control_tls_state(
    manager: &ServiceTunnelManager,
    name: &str,
    definition: &ControlDefinition,
) -> Option<serde_json::Value> {
    if definition
        .options
        .get("use_ssl")
        .is_none_or(|value| value != "true")
    {
        return None;
    }
    let policy = manager.service_tls_policy();
    let (ok, failed) = manager.tls_handshakes_for(name);
    Some(serde_json::json!({
        "enabled": true,
        "verify": policy
            .as_ref()
            .map(|policy| policy.verify_mode())
            .unwrap_or("unconfigured"),
        "identity_expires": policy
            .as_ref()
            .and_then(|policy| policy.identity_expires_unix()),
        "handshakes_ok": ok,
        "handshakes_failed": failed,
    }))
}

impl TunnelControlState {
    /// Builds control state over an explicit store, startup inventory,
    /// and manager. Definitions load lazily at [`Self::startup`].
    pub fn new(
        store: ControlStore,
        startup: ServiceTunnelSet,
        manager: Arc<ServiceTunnelManager>,
        outbound_secrets: Arc<dyn RouterSecretOwner>,
    ) -> Self {
        Self {
            store,
            startup,
            manager,
            outbound_secrets: outbound_secrets.clone(),
            op_lock: tokio::sync::Mutex::new(()),
            definitions: Mutex::new(BTreeMap::new()),
            running: Mutex::new(BTreeSet::new()),
            supervised: Mutex::new(HashMap::new()),
            failed: Mutex::new(HashMap::new()),
            transitioning: Mutex::new(BTreeSet::new()),
            children: Mutex::new(None),
            cancellation: Mutex::new(None),
        }
    }

    /// Builds control state from the validated daemon configuration and the
    /// **one** shared service manager, with the store beneath the router data
    /// dir and the startup inventory from the configured service set.
    ///
    /// Plan 337: the manager is no longer constructed here. The composition
    /// root builds it once and injects the same instance into the product
    /// layer, so a control-created runtime is the same runtime the product
    /// delivers and publishes through. The startup inventory stays a read-only
    /// copy of daemon configuration; control generations still arrive through
    /// the store at startup.
    pub fn for_config(
        config: &crate::config::Config,
        manager: Arc<ServiceTunnelManager>,
        outbound_secrets: Arc<dyn RouterSecretOwner>,
    ) -> Result<Self, ControlError> {
        let store = ControlStore::open(&config.router.data_dir).map_err(ControlError::Store)?;
        Ok(Self::new(
            store,
            config.service_tunnels.tunnels.clone(),
            manager,
            outbound_secrets,
        ))
    }

    /// The installed outbound-credential owner (Plan 342).
    ///
    /// Public because the runtime that actually opens a credential needs the
    /// same `Arc`; it is the owner, not a capability any caller may invent.
    pub fn outbound_secrets(&self) -> Arc<dyn RouterSecretOwner> {
        self.outbound_secrets.clone()
    }

    /// Current published store generation (`0` when nothing published).
    pub fn store_generation(&self) -> u64 {
        self.store.current_id()
    }

    /// Daemon startup: removes orphan staged generations, loads and
    /// validates the newest durable control generation (recovering the
    /// prior generation on corruption), detects collisions against
    /// startup-owned names before starting anything, reconciles
    /// StartOnLoad definitions with supported backends, and starts
    /// their supervisors under the supplied scope.
    ///
    /// Failures isolate per definition: every valid StartOnLoad
    /// definition starts even when a sibling fails. Returns the
    /// per-definition failure list (empty on full success). Never
    /// rotates a persistent server identity: server destinations reuse
    /// the manager's persisted service-destination records.
    pub async fn startup(
        &self,
        children: &i2pr_runtime::ChildScope,
        cancellation: &i2pr_runtime::CancellationToken,
    ) -> Vec<(String, &'static str)> {
        {
            *lock(&self.children) = Some(children.clone());
            *lock(&self.cancellation) = Some(cancellation.clone());
        }
        self.store.cleanup_orphans();
        let loaded = match self.store.load() {
            Ok(loaded) => loaded,
            Err(_) => LoadedGeneration {
                generation_id: 0,
                definitions: Vec::new(),
            },
        };
        let mut failures = Vec::new();
        let mut definitions = BTreeMap::new();
        for definition in loaded.definitions {
            if self.startup_name(&definition.name).is_some() {
                failures.push((definition.name.clone(), "startup name collision"));
                continue;
            }
            if build_control_spec_with_filter_root(&definition, Some(&self.store.filter_root()))
                .is_err()
            {
                failures.push((definition.name.clone(), "stored definition invalid"));
                continue;
            }
            definitions.insert(definition.name.clone(), definition);
        }
        *lock(&self.definitions) = definitions;
        let start: Vec<String> = lock(&self.definitions)
            .values()
            .filter(|definition| {
                definition.start_on_load && definition.tunnel_type.has_plan291_backend()
            })
            .map(|definition| definition.name.clone())
            .collect();
        if start.is_empty() {
            return failures;
        }
        // Bulk fast path: one transaction for every StartOnLoad
        // definition, then per-definition isolation on failure.
        lock(&self.running).extend(start.iter().cloned());
        for name in &start {
            lock(&self.transitioning).insert(name.clone());
        }
        let guard = self.op_lock.lock().await;
        match self.commit_locked(&guard, &start, true).await {
            Ok(_) => {
                for name in &start {
                    self.record_success(name);
                }
            }
            Err(_) => {
                // Isolated fallback: reset running intent and start
                // each definition alone so one bad sibling cannot block
                // the rest.
                lock(&self.running).clear();
                for name in &start {
                    lock(&self.transitioning).remove(name);
                }
                drop(guard);
                for name in start {
                    if let Err(error) = self.start_inner(&name).await {
                        failures.push((name, static_control_reason(&error)));
                    }
                }
            }
        }
        failures
    }

    /// Stops every **control-owned** runtime by reconciling the shared
    /// manager back to the startup-owned set.
    ///
    /// Plan 337: this is deliberately not a manager-wide shutdown. The
    /// manager is the one shared product-layer instance, so tearing it
    /// down would stop startup-owned services and any sibling the product
    /// layer owns. Reconciling to the startup-only set removes exactly the
    /// runtimes this coordinator created.
    pub async fn shutdown(&self) {
        let candidate = ServiceTunnelSet {
            tunnels: self.startup.tunnels.clone(),
        };
        let _ = self
            .manager
            .reconcile(Arc::new(candidate), CONTROL_DRAIN_DEADLINE)
            .await;
    }

    /// The published `.b32.i2p` address for one control-owned service, if it
    /// publishes an encrypted LeaseSet (Plan 334).
    ///
    /// Returns `None` for a service that publishes nothing, for a stopped
    /// service, and for a name this coordinator does not own — so a caller
    /// cannot learn a startup-owned service's address through the control
    /// surface.
    pub fn encrypted_address_for(&self, name: &str) -> Option<String> {
        if !lock(&self.definitions).contains_key(name) {
            return None;
        }
        self.manager.els2_material_for_spec(name)?.address().ok()
    }

    /// The published addresses of every control-owned encrypted service,
    /// keyed by name (Plan 334). Bounded by the number of definitions.
    pub fn encrypted_addresses(&self) -> std::collections::BTreeMap<String, String> {
        let names: Vec<String> = lock(&self.definitions).keys().cloned().collect();
        names
            .into_iter()
            .filter_map(|name| {
                self.encrypted_address_for(&name)
                    .map(|address| (name, address))
            })
            .collect()
    }

    /// Finds a startup-owned spec kind by name.
    fn startup_name(&self, name: &str) -> Option<ServiceTunnelKind> {
        self.startup
            .tunnels
            .iter()
            .find(|spec| spec.id.as_str() == name)
            .map(|spec| spec.kind)
    }

    /// Builds the manager candidate set from the running subset of
    /// the durable mirror. Stopped definitions are excluded entirely:
    /// the manager only ever sees running specs, so staging never
    /// binds a socket for a stopped tunnel and stop transitions drain
    /// without restaging. (`enabled` is always true in the candidate;
    /// persisted intent lives on the definition.)
    fn candidate_set(&self) -> Result<ServiceTunnelSet, ControlError> {
        let mut tunnels = Vec::new();
        // Plan 337: this manager is the one shared product-layer manager,
        // and `reconcile` is a whole-set transactional replace. The
        // candidate must therefore describe the entire runtime surface it
        // owns. Startup-owned specs are carried through **verbatim** as
        // daemon configuration produced them (`enabled` included) so the
        // diff reports them unchanged and no startup-owned runtime is
        // removed or restaged by a control transaction. They are never
        // mutated here; this coordinator only adds names it owns.
        tunnels.extend(self.startup.tunnels.iter().cloned());
        {
            let definitions = lock(&self.definitions);
            let running = lock(&self.running);
            tunnels.reserve(definitions.len());
            for definition in definitions.values() {
                if !running.contains(&definition.name) {
                    continue;
                }
                let filter_root = self.store.filter_root();
                let mut spec = build_control_spec_with_filter_root(definition, Some(&filter_root))?;
                spec.enabled = true;
                tunnels.push(spec);
            }
        }
        let candidate = ServiceTunnelSet { tunnels };
        candidate
            .validate()
            .map_err(|error| ControlError::AggregateRejected(static_manager_reason2(&error)))?;
        // Cross-class bind collisions fail closed: a listener address is
        // a machine-wide exclusive resource, so a control-owned name may
        // never shadow a startup-owned socket. Startup-owned entries are
        // themselves in the candidate now, so they are skipped here.
        self.reject_cross_class_collisions(&candidate)?;
        Ok(candidate)
    }

    /// Rejects listener collisions between the candidate control set
    /// and startup-owned listeners.
    fn reject_cross_class_collisions(
        &self,
        candidate: &ServiceTunnelSet,
    ) -> Result<(), ControlError> {
        use std::collections::HashSet;
        let mut startup_listeners = HashSet::new();
        for spec in &self.startup.tunnels {
            if let Some(listener) = spec.listener {
                startup_listeners.insert(listener.socket());
            }
        }
        for spec in &candidate.tunnels {
            // Plan 337: the candidate carries startup-owned specs verbatim
            // (see `candidate_set`); comparing one against itself is not
            // a collision.
            if self.startup_name(spec.id.as_str()).is_some() {
                continue;
            }
            // Plan 381: ephemeral listeners are exempt — each port-0 bind
            // gets a distinct OS port. A startup spec holding a fixed port
            // still collides with a control spec naming the same socket.
            if let Some(listener) = spec.listener
                && !listener.is_ephemeral()
                && startup_listeners.contains(&listener.socket())
            {
                return Err(ControlError::NameCollision(spec.id.as_str().to_owned()));
            }
        }
        Ok(())
    }

    /// Rejects a single spec's listener against startup-owned
    /// listeners. Applied at create/edit time even for stopped
    /// definitions (the candidate set only carries running specs, so
    /// this check is the fail-closed guard for stopped rows shadowing
    /// startup-owned sockets).
    fn check_listener_against_startup(&self, spec: &ServiceTunnelSpec) -> Result<(), ControlError> {
        // Plan 381: ephemeral listeners are exempt on either side — each
        // port-0 bind gets a distinct OS port, matching
        // `ServiceTunnelSet::validate` and the reconcile bind-collision
        // check. (The error variant is pre-existing and misnamed; a
        // listener collision is not a name collision, but renaming the
        // variant is out of scope for this plan.)
        let Some(listener) = spec.listener else {
            return Ok(());
        };
        if listener.is_ephemeral() {
            return Ok(());
        }
        for startup in &self.startup.tunnels {
            if startup
                .listener
                .filter(|owned| !owned.is_ephemeral())
                .map(|owned| owned.socket())
                == Some(listener.socket())
            {
                return Err(ControlError::NameCollision(spec.id.as_str().to_owned()));
            }
        }
        Ok(())
    }

    /// Whether an option edit on a running definition restages its
    /// runtime (listener/destination/target/kind change). Restaging
    /// rebinds sockets, so it travels as stop-plus-start; in-place
    /// changes (resource ceilings, intent flags) commit directly.
    fn edit_restages(&self, prior: &ControlDefinition, candidate: &ControlDefinition) -> bool {
        use i2pr_service_tunnels::{DiffClass, diff_spec};
        let filter_root = self.store.filter_root();
        let mut old_spec = match build_control_spec_with_filter_root(prior, Some(&filter_root)) {
            Ok(spec) => spec,
            Err(_) => return true,
        };
        let mut new_spec = match build_control_spec_with_filter_root(candidate, Some(&filter_root))
        {
            Ok(spec) => spec,
            Err(_) => return true,
        };
        old_spec.enabled = true;
        new_spec.enabled = true;
        matches!(
            diff_spec(&old_spec, &new_spec),
            DiffClass::ReplaceListener | DiffClass::ReplaceDestination
        )
    }

    /// Core transaction under the op lock for the `names` subset:
    /// stages persistence, reconciles the manager, starts supervisors
    /// for added/replaced runtimes, publishes the generation, prunes
    /// supervision records, and verifies agreement.
    ///
    /// When `probe_ports` is set, listener sockets for `names` are
    /// briefly awaited before staging (see [`Self::await_port_release`]).
    /// A supervisor failure reconciles back without publishing; a
    /// publish failure reconciles back under a hard deadline. Either
    /// way the in-memory mirror is the caller's to roll back.
    /// Returns the published store generation id plus the manager diff.
    async fn commit_locked(
        &self,
        _guard: &tokio::sync::MutexGuard<'_, ()>,
        names: &[String],
        probe_ports: bool,
    ) -> Result<(u64, Vec<i2pr_service_tunnels::ServiceDiff>), ControlError> {
        // Release previously drained runtimes first so their listeners
        // are free before staging binds (immediate-release policy).
        self.manager.reap_expired_drains();
        if probe_ports {
            self.await_port_release(names).await;
        }
        let mirror = lock(&self.definitions).clone();
        let staged = self.store.stage(&mirror).map_err(ControlError::Store)?;
        let candidate = self.candidate_set()?;
        let outcome = self
            .manager
            .reconcile(Arc::new(candidate), CONTROL_DRAIN_DEADLINE)
            .await
            .map_err(|error| ControlError::Manager(static_manager_reason(&error)))?;
        if let Err(error) = self.sync_names(names, &outcome.diff).await {
            let _ = self.reconcile_back().await;
            return Err(error);
        }
        if let Err(store_error) = self.store.publish(staged) {
            let back = self.reconcile_back().await;
            return Err(ControlError::PublishFailed {
                reason: match store_error {
                    StoreError::Io(reason) => reason,
                    StoreError::Corrupt(reason) => reason,
                    StoreError::OverBound => "generation over ceiling",
                    StoreError::PathEscape => "path escape",
                    StoreError::NotRegular => "symlink or special file",
                },
                reconciled_back: back,
            });
        }
        // Prune supervision records for names that no longer run and
        // clear transitioning flags for this transaction's names.
        {
            let running_now = lock(&self.running).clone();
            lock(&self.supervised).retain(|name, _| running_now.contains(name));
        }
        for name in names {
            lock(&self.transitioning).remove(name);
        }
        // Plan 337: with the manager committed, every control-owned
        // service's persisted identity exists, so encrypted-LeaseSet2
        // publication material can be derived and handed to the product's
        // publication path. A failure here reconciles back rather than
        // leaving a definition whose record can never be published.
        if let Err(error) = self.sync_els2_materials() {
            let back = self.reconcile_back().await;
            return Err(ControlError::PublishFailed {
                reason: static_control_reason(&error),
                reconciled_back: back,
            });
        }
        // Plan 351: the consumer-side mirror runs in the same reconciliation
        // and with the same rollback semantics. A definition that has been
        // published but whose consumer secret could not be installed would
        // otherwise look configured while resolving against the address-only
        // key — the most confusing possible failure, because it succeeds
        // against the wrong key for a bounded window.
        if let Err(error) = self.sync_encrypted_target_secrets() {
            let back = self.reconcile_back().await;
            return Err(ControlError::PublishFailed {
                reason: static_control_reason(&error),
                reconciled_back: back,
            });
        }
        // Plan 342: the sealed outproxy credential is installed in the same
        // reconciliation with the same rollback semantics, for the same
        // reason. A published definition whose credential the request path
        // cannot find would look configured and fail every clearnet request
        // with "secret owner unavailable" — an error an operator could not
        // trace back to the tunnel whose configuration changed.
        if let Err(error) = self.sync_outproxy_providers() {
            let back = self.reconcile_back().await;
            return Err(ControlError::PublishFailed {
                reason: static_control_reason(&error),
                reconciled_back: back,
            });
        }
        // Plan 380: the sealed ELS2 consumer credential joins the same
        // reconciliation with the same rollback semantics, for a sharper
        // reason than the outproxy case above. A published definition whose
        // credential the resolve path cannot find does not merely fail every
        // request with "secret owner unavailable" — it would resolve the
        // `.b33` as a **no-credential** consumer, against a record that
        // requires one, and report the failure as an unwrap error. Rolling
        // back is what keeps "configured" and "will actually be presented"
        // the same state.
        if let Err(error) = self.sync_encrypted_target_credentials() {
            let back = self.reconcile_back().await;
            return Err(ControlError::PublishFailed {
                reason: static_control_reason(&error),
                reconciled_back: back,
            });
        }
        self.verify_agreement()?;
        Ok((staged, outcome.diff))
    }

    /// Installs or drops encrypted-LeaseSet2 publication material for every
    /// control-owned definition (Plan 337).
    ///
    /// A definition whose resolved mode publishes a type-5 record gets
    /// material derived from **its own** persisted identity, so the
    /// published address names the destination its inner LeaseSet2 is
    /// signed by. A definition that publishes nothing has its material
    /// dropped, which is what makes a mode edit or a delete take effect.
    ///
    /// Fails closed: if a type-5 definition's identity record cannot be
    /// read, the caller reconciles back rather than silently publishing an
    /// ordinary LeaseSet2 for a service the operator configured as
    /// encrypted.
    fn sync_els2_materials(&self) -> Result<(), ControlError> {
        let definitions = lock(&self.definitions).clone();
        let running = lock(&self.running).clone();
        for (name, definition) in definitions.iter() {
            let plan = definition.lease_set_security()?;
            if !running.contains(name) || !plan.publishes_type5() {
                self.manager.remove_els2_material(name);
                continue;
            }
            let material = crate::service_els2::load_service_els2_material(
                &self.manager,
                name,
                &plan,
                &definition.options,
            )
            .map_err(|error| {
                // Static reasons only: an ELS2 failure must never carry
                // the lookup secret, a client key, or a service name
                // into a response, an error string, or a log line.
                use crate::service_els2::ServiceEls2Error as E;
                ControlError::Manager(match error {
                    E::IdentityUnavailable { .. } => "service identity record unavailable",
                    E::AuthorizationMissing { .. } => "client authorization block missing",
                    E::AuthorizationUnexpected { .. } => {
                        "client authorization block not permitted by mode"
                    }
                    E::MalformedClientKey { .. } => "client authorization key malformed",
                    E::Els2 { .. } | E::Address { .. } => "encrypted leaseset material rejected",
                })
            })?;
            // `None` means the plan publishes no type-5 record after all
            // (already handled above); treat it as "no material" rather
            // than publishing an ordinary record for an encrypted service.
            if let Some(material) = material {
                self.manager.install_els2_material(name, Arc::new(material));
            } else {
                self.manager.remove_els2_material(name);
            }
        }
        Ok(())
    }

    /// Plan 351 Gate 2 — installs the ELS2 **consumer** lookup secret for
    /// every control-owned definition that names an encrypted-service target,
    /// and drops it for every definition that does not.
    ///
    /// The exact mirror image of [`Self::sync_els2_materials`], and it runs in
    /// the same reconciliation, so the two can never drift: a definition that
    /// stops naming a b33 loses its publisher material and its consumer secret
    /// in the same pass, and a deleted definition loses both.
    ///
    /// An **absent** `leaseset_password` is not an error and installs nothing.
    /// A `.b33` address alone derives today's blinded storage key; the secret
    /// Plan 342 — installs each definition's **sealed** `outproxy_password`
    /// for the request paths, and drops the ones that no longer have one.
    ///
    /// The same shape as `sync_els2_materials` and
    /// `sync_encrypted_target_secrets`, and the same reasoning: the manager
    /// cannot read a definition, so the control plane owns the mapping and
    /// runs it inside the same reconciliation with the same rollback. A
    /// definition that removes its credential, or is deleted outright, loses
    /// its installation in the same pass that publishes the change.
    fn sync_outproxy_providers(&self) -> Result<(), ControlError> {
        let definitions = lock(&self.definitions).clone();
        let store = self.outbound_secrets();
        for (name, definition) in definitions.iter() {
            // A definition with no outproxy block installs nothing, and an
            // installation from a previous pass is dropped — the same
            // behaviour as the two sibling sync passes.
            let Some(block) = crate::outproxy_options::parse_outproxy_block(
                name,
                definition.tunnel_type.name(),
                &definition.options,
            )?
            else {
                self.manager.remove_outproxy_provider(name);
                continue;
            };
            // The stored form, or `None` when the block declares no
            // credential. `String`, not `Zeroizing`: on this pass the value is
            // already ciphertext produced by the Plan 341 owner, and the
            // provider wraps it.
            let sealed = definition
                .options
                .get(crate::outproxy_options::OUTPROXY_PASSWORD_KEY)
                .filter(|value| !value.is_empty())
                .cloned();
            let provider = crate::outproxy_route::RouterOutproxyProvider::new(
                block.config,
                // Upcast to the parent capability. `RouterSecretOwner` is a
                // supertrait, so this is a view of the same `Arc` and the same
                // derived key — the provider does not get a second store, it
                // gets a narrower handle on the one the composition root built.
                Arc::clone(&store)
                    as Arc<dyn i2pr_service_tunnels::outbound_secret::OutboundSecretStore>,
                sealed,
            )
            .map_err(|_| ControlError::Manager("outproxy provider is not usable"))?;
            self.manager
                .install_outproxy_provider(name, Arc::new(provider));
        }
        // Names present in the registry but absent from the definition set are
        // dropped: a deleted definition must not leave a live credential
        // behind.
        let known: BTreeSet<String> = definitions.keys().cloned().collect();
        for spec_id in self.manager.installed_outproxy_providers() {
            if !known.contains(&spec_id) {
                self.manager.remove_outproxy_provider(&spec_id);
            }
        }
        Ok(())
    }

    /// Plan 351 Gate 2 — installs the ELS2 **consumer** lookup secret for
    /// every control-owned definition that names an encrypted-service target,
    /// and drops it for every definition that does not.
    ///
    /// The exact mirror image of [`Self::sync_els2_materials`], and it runs in
    /// the same reconciliation, so the two can never drift: a definition that
    /// stops naming a b33 loses its publisher material and its consumer secret
    /// in the same pass, and a deleted definition loses both.
    ///
    /// An **absent** `leaseset_password` is not an error and installs nothing.
    /// A `.b33` address alone derives today's blinded storage key; the secret
    /// only participates when the publisher configured one. The product layer
    /// resolves that case as a real lookup against the address-only key.
    ///
    /// An **over-long** secret is a configuration error, not a truncation: the
    /// same `MAX_LOOKUP_SECRET_LENGTH` bound the publisher applies, so the
    /// consumer and publisher cannot disagree about which value is legal.
    fn sync_encrypted_target_secrets(&self) -> Result<(), ControlError> {
        let definitions = lock(&self.definitions).clone();
        for (name, definition) in definitions.iter() {
            // `ControlDefinition` keeps only the option map, so the target is
            // re-parsed rather than read from a field. A definition that has
            // been through `normalize_definition` always parses cleanly; an
            // unparseable value is treated as "not an encrypted target", which
            // drops the secret and therefore fails closed.
            let names_encrypted_target = definition
                .options
                .get("target_destination")
                .and_then(|value| DestinationRef::parse(value).ok())
                .is_some_and(|reference| {
                    matches!(reference, DestinationRef::EncryptedService { .. })
                });
            let Some(raw) = definition.options.get("leaseset_password") else {
                self.manager.remove_encrypted_target_secret(name);
                continue;
            };
            if !names_encrypted_target {
                // `normalize_definition` already rejects this pairing, so
                // reaching it means a stored definition predates the rule. Drop
                // the secret rather than install a value nothing will read.
                self.manager.remove_encrypted_target_secret(name);
                continue;
            }
            let secret = i2pr_crypto::red25519::LookupSecret::from_str(raw).map_err(|_| {
                // Static reason only. `LookupSecret::from_str` reports the
                // offending length, and even that is not echoed here: an
                // error string that reaches a client reply or a log line
                // must not be able to carry configuration bytes.
                ControlError::Manager("encrypted consumer lookup secret is not usable")
            })?;
            self.manager
                .install_encrypted_target_secret(name, Arc::new(secret));
        }
        Ok(())
    }

    /// Plan 380 — installs the sealed ELS2 **consumer credential** for every
    /// control-owned definition that names an encrypted-service target, and
    /// drops it for every definition that does not.
    ///
    /// The exact mirror image of [`Self::sync_encrypted_target_secrets`], and
    /// it runs in the same reconciliation, so the two cannot drift: a
    /// definition that stops naming a `.b33` loses its lookup secret and its
    /// credential in the same pass, and a deleted definition loses both.
    ///
    /// Two things are worth stating because they are where this differs from
    /// the lookup-secret arm:
    ///
    /// - the value read here is **already ciphertext**. `normalize_definition`
    ///   sealed it before the definition could reach a generation file, so this
    ///   pass adopts a stored form rather than handling plaintext. That is the
    ///   whole point: the reconciliation is the last place that sees the option
    ///   map, and it has nothing sensitive left in it to leak.
    /// - a definition that supplies a credential the store cannot adopt is an
    ///   **error**, not a skip. `normalize_definition` already proved the owner
    ///   could seal, so failing to open here means the stored form was written
    ///   by a different router identity than the one now running — which is
    ///   precisely the "copied data directory" case, and it must fail loudly
    ///   rather than resolving as an unauthenticated client.
    fn sync_encrypted_target_credentials(&self) -> Result<(), ControlError> {
        let definitions = lock(&self.definitions).clone();
        for (name, definition) in definitions.iter() {
            // Same re-parse as the lookup-secret arm: `ControlDefinition` keeps
            // only the option map, so an unparseable target reads as "not an
            // encrypted target", which drops the credential and fails closed.
            let names_encrypted_target = definition
                .options
                .get("target_destination")
                .and_then(|value| DestinationRef::parse(value).ok())
                .is_some_and(|reference| {
                    matches!(reference, DestinationRef::EncryptedService { .. })
                });
            let Some(stored) = definition
                .options
                .get(crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION)
                .filter(|value| !value.is_empty())
            else {
                self.manager.remove_encrypted_target_credential(name);
                continue;
            };
            if !names_encrypted_target {
                // `normalize_definition` already rejects this pairing, so
                // reaching it means a stored definition predates the rule.
                // Drop the credential rather than install a value nothing can
                // ever present.
                self.manager.remove_encrypted_target_credential(name);
                continue;
            }
            let sealed =
                SealedEncryptedTargetCredential::from_sealed(stored, self.outbound_secrets())
                    .map_err(|_| {
                        // Static reason only. A stored form that will not open could be
                        // a foreign router's, a tampered generation, or a corrupt one,
                        // and none of those distinctions may be reported through a
                        // string that reaches a client reply.
                        ControlError::Manager("encrypted consumer credential is not usable")
                    })?;
            self.manager
                .install_encrypted_target_credential(name, Arc::new(sealed));
        }
        // Deletion sweep, exactly as `sync_outproxy_providers` does it: a
        // definition removed from the map must not leave a live credential
        // installed against a spec id nothing owns any more.
        let known: BTreeSet<String> = definitions.keys().cloned().collect();
        for spec_id in self.manager.installed_encrypted_target_credentials() {
            if !known.contains(&spec_id) {
                self.manager.remove_encrypted_target_credential(&spec_id);
            }
        }
        Ok(())
    }

    /// Best-effort wait for listener sockets to free before staging.
    ///
    /// Drained runtimes release their sockets only after their
    /// cancelled supervisor tasks exit, which races staging binds on
    /// stop-then-start and restart sequences. This probes each intended
    /// socket briefly (bounded `40 × 50 ms`); a freed socket proceeds
    /// immediately, while a still-held socket proceeds to staging
    /// anyway so genuine conflicts surface as bind errors there. Never
    /// fails by itself.
    async fn await_port_release(&self, names: &[String]) {
        const ATTEMPTS: usize = 40;
        const STEP: Duration = Duration::from_millis(50);
        for name in names {
            let Some(socket) = self.intended_listener(name) else {
                continue;
            };
            for _ in 0..ATTEMPTS {
                match tokio::net::TcpListener::bind(socket).await {
                    Ok(probe) => {
                        drop(probe);
                        break;
                    }
                    Err(_) => tokio::time::sleep(STEP).await,
                }
            }
        }
    }

    /// Intended client-listener socket for a mirror definition (`None`
    /// when the definition carries no listener or the options do not
    /// parse; staging validation reports those).
    fn intended_listener(&self, name: &str) -> Option<std::net::SocketAddr> {
        let definitions = lock(&self.definitions);
        let definition = definitions.get(name)?;
        let port: u16 = definition.options.get("listen_port")?.parse().ok()?;
        let host: std::net::IpAddr = definition
            .options
            .get("listen_host")
            .and_then(|text| text.parse().ok())
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        Some(std::net::SocketAddr::new(host, port))
    }

    /// Reconciles the manager back to the current candidate (used after
    /// supervisor or publish failures). Returns whether runtime and
    /// durable intent agree afterwards.
    async fn reconcile_back(&self) -> bool {
        let candidate = match self.candidate_set() {
            Ok(candidate) => candidate,
            Err(_) => return false,
        };
        if self
            .manager
            .reconcile(Arc::new(candidate), CONTROL_ROLLBACK_DEADLINE)
            .await
            .is_err()
        {
            return false;
        }
        self.verify_agreement().is_ok()
    }

    /// Verifies durable intent and runtime generation agree: every
    /// running control-owned name has a manager runtime, and no runtime
    /// exists that this coordinator does not own.
    ///
    /// Plan 337: the manager is shared with the product layer, so a
    /// startup-owned runtime is not an "extra" runtime for this
    /// coordinator. Any other unexplained spec id still is.
    fn verify_agreement(&self) -> Result<(), ControlError> {
        let running = lock(&self.running);
        for name in running.iter() {
            if !self.manager.has_runtime(name) {
                return Err(ControlError::Manager("runtime missing after transition"));
            }
        }
        for runtime in self.manager.all_service_runtimes() {
            if running.contains(&runtime.spec_id) {
                continue;
            }
            if self.startup_name(&runtime.spec_id).is_some() {
                continue;
            }
            return Err(ControlError::Manager("extra runtime after transition"));
        }
        Ok(())
    }

    /// Starts supervisors for diff-added/replaced runtimes in `names`.
    /// Continues past individual supervisor failures (per-definition
    /// isolation) and reports the first failure; every failure is
    /// recorded in the failed map regardless. Without an installed
    /// scope (unit scope) runtimes commit without loops.
    async fn sync_names(
        &self,
        names: &[String],
        diff: &[i2pr_service_tunnels::ServiceDiff],
    ) -> Result<(), ControlError> {
        use i2pr_service_tunnels::DiffClass;
        let generation = self.manager.committed_generation_id().unwrap_or(0);
        let scope = match (
            lock(&self.children).clone(),
            lock(&self.cancellation).clone(),
        ) {
            (Some(children), Some(cancellation)) => Some((children, cancellation)),
            _ => None,
        };
        let mut first_error: Option<ControlError> = None;
        for entry in diff {
            if !names.iter().any(|name| name == &entry.id) {
                continue;
            }
            if !matches!(
                entry.class,
                DiffClass::Add | DiffClass::ReplaceListener | DiffClass::ReplaceDestination
            ) {
                continue;
            }
            let Some(runtime) = self.manager.service_runtime_for_spec(&entry.id) else {
                continue;
            };
            let Some((children, cancellation)) = scope.clone() else {
                continue;
            };
            if let Err(error) =
                self.manager
                    .start_supervisors(vec![runtime], &children, cancellation.clone())
            {
                let reason = static_manager_reason(&error);
                lock(&self.failed).insert(entry.id.clone(), reason);
                if first_error.is_none() {
                    first_error = Some(ControlError::Manager(reason));
                }
                continue;
            }
            lock(&self.supervised).insert(entry.id.clone(), generation);
            lock(&self.failed).remove(&entry.id);
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    /// Records a successful transition: clears failure, drops the
    /// transitioning flag.
    fn record_success(&self, name: &str) {
        lock(&self.failed).remove(name);
        lock(&self.transitioning).remove(name);
    }

    /// Reads one control definition.
    fn definition(&self, name: &str) -> Option<ControlDefinition> {
        lock(&self.definitions).get(name).cloned()
    }

    /// Classifies one control-owned name for `get`.
    fn control_status(
        &self,
        name: &str,
        definition: &ControlDefinition,
    ) -> i2pr_i2pcontrol::TunnelStatus {
        use i2pr_i2pcontrol::TunnelStatus;
        if !definition.tunnel_type.has_plan291_backend() {
            return TunnelStatus::Unsupported;
        }
        let transitioning = lock(&self.transitioning).contains(name);
        let running = lock(&self.running).contains(name);
        if transitioning {
            return if running {
                TunnelStatus::Starting
            } else {
                TunnelStatus::Stopping
            };
        }
        if running && lock(&self.failed).contains_key(name) {
            return TunnelStatus::Failed;
        }
        if running && self.manager.has_runtime(name) {
            return TunnelStatus::Running;
        }
        if running {
            return TunnelStatus::Failed;
        }
        TunnelStatus::Stopped
    }

    /// Serializes one control-owned `get` response: actual runtime state
    /// reported separately from persisted intent.
    fn control_response(&self, name: &str, definition: &ControlDefinition) -> serde_json::Value {
        use i2pr_i2pcontrol::SECRET_OPTIONS;
        let status = self.control_status(name, definition);
        let running = lock(&self.running).contains(name);
        let bind = self
            .manager
            .client_listener_address(name)
            .map(|address| address.to_string())
            .or_else(|| {
                definition.options.get("listen_port").map(|port| {
                    let address = definition
                        .options
                        .get("listen_host")
                        .cloned()
                        .unwrap_or_else(|| "127.0.0.1".to_owned());
                    format!("{address}:{port}")
                })
            });
        let destination = self.manager.service_destination_b64(name);
        let connections = self.manager.active_connections(name);
        // Secret-classified options never appear, even though the 289
        // subset cannot store them: filter by key, key-only display.
        let mut options = serde_json::Map::new();
        for (key, value) in &definition.options {
            if SECRET_OPTIONS.contains(&key.as_str()) {
                options.insert(
                    key.clone(),
                    serde_json::Value::String("[redacted]".to_owned()),
                );
            } else {
                options.insert(key.clone(), serde_json::Value::String(value.clone()));
            }
        }
        let mut response = serde_json::json!({
            "name": name,
            "provenance": TunnelProvenance::ControlOwned.as_str(),
            "type": definition.tunnel_type.name(),
            "options": options,
            "start_on_load": definition.start_on_load,
            "running": running,
            "status": status.name(),
            "generation": self.store_generation(),
            "bind": bind,
            "destination": destination,
            "active_connections": connections,
        });
        // Plan 297: the TLS state object appears only on `use_ssl`
        // tunnels, so every existing output shape is unchanged.
        if let Some(tls) = control_tls_state(&self.manager, name, definition) {
            response
                .as_object_mut()
                .expect("tunnel response is an object")
                .insert("tls".to_owned(), tls);
        }
        // Plan 334: the LeaseSet security projection reports which mode is
        // stored and whether each secret is configured, never a byte of
        // either. It appears on every tunnel — an ordinary one reports the
        // ordinary posture — so a client never has to infer the absence of
        // encryption from a missing field.
        if let Some(object) = response.as_object_mut() {
            object.insert(
                "lease_set_security".to_owned(),
                lease_set_security_projection(definition),
            );
            // Plan 337: a service that publishes an encrypted LeaseSet now
            // has a real published address, so the control surface reports
            // it. Without this an operator has no way to hand a client
            // something that resolves. The address is a public
            // destination with flag bits — never the lookup secret, the
            // client keys, or any signing material — and it is absent (not
            // null, not empty) for an ordinary service, so the existing
            // output shape of every non-encrypted tunnel is unchanged.
            if let Some(material) = self.manager.els2_material_for_spec(name) {
                match material.address() {
                    Ok(address) => {
                        object.insert(
                            "encrypted_address".to_owned(),
                            serde_json::Value::String(address),
                        );
                    }
                    Err(_) => {
                        // A material that cannot encode an address is a
                        // publication the operator must see as absent, not
                        // as a silently wrong address.
                        object.remove("encrypted_address");
                    }
                }
            }
        }
        response
    }

    /// Serializes one startup-owned summary (inspectable, immutable).
    fn startup_response(&self, name: &str) -> Result<serde_json::Value, ControlError> {
        let spec = self
            .startup
            .tunnels
            .iter()
            .find(|spec| spec.id.as_str() == name)
            .ok_or_else(|| ControlError::UnknownTunnel(name.to_owned()))?;
        Ok(serde_json::json!({
            "name": name,
            "provenance": TunnelProvenance::StartupOwned.as_str(),
            "kind": spec.kind.as_str(),
            "enabled": spec.enabled,
            "listener": spec.listener.map(|listener| listener.socket().to_string()),
        }))
    }

    /// `get`: actual runtime state plus persisted intent. Absent name
    /// returns the bounded whole inventory (startup summaries plus
    /// control-owned detail).
    pub fn get(&self, name: Option<&str>) -> Result<serde_json::Value, ControlError> {
        if let Some(name) = name {
            if let Some(definition) = self.definition(name) {
                return Ok(self.control_response(name, &definition));
            }
            if self.startup_name(name).is_some() {
                return self.startup_response(name);
            }
            return Err(ControlError::UnknownTunnel(name.to_owned()));
        }
        let mut startup = serde_json::Map::new();
        for spec in &self.startup.tunnels {
            startup.insert(
                spec.id.as_str().to_owned(),
                self.startup_response(spec.id.as_str())?,
            );
        }
        let mut control = serde_json::Map::new();
        for (name, definition) in lock(&self.definitions).clone() {
            control.insert(name.clone(), self.control_response(&name, &definition));
        }
        Ok(serde_json::json!({"startup": startup, "control": control}))
    }

    /// `create`: validates the complete candidate before any side
    /// effect, stages persistence, reconciles, publishes, and starts
    /// supervisors for running intent.
    pub async fn create(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("create requires name"))?;
        let tunnel_type = request
            .tunnel_type
            .ok_or(ControlError::InvalidRequest("create requires type"))?;
        if self.definition(name).is_some() || self.startup_name(name).is_some() {
            return Err(ControlError::NameCollision(name.to_owned()));
        }
        let start_on_load = request
            .options
            .get("start_on_load")
            .map(|value| value == "true")
            .unwrap_or(true);
        let filter_root = self.store.filter_root();
        let definition = normalize_definition_with_filter_root(
            name,
            tunnel_type,
            &request.options,
            start_on_load,
            Some(&filter_root),
            self.outbound_secrets().as_ref(),
        )?;
        let probe = build_control_spec_with_filter_root(&definition, Some(&filter_root))?;
        self.check_listener_against_startup(&probe)?;
        let running_before = lock(&self.running).clone();
        lock(&self.definitions).insert(name.to_owned(), definition);
        // Creation carries running intent when the definition should
        // run now: explicit `start_on_load` intent starts immediately.
        if lock(&self.definitions)
            .get(name)
            .map(|definition| definition.start_on_load)
            .unwrap_or(false)
        {
            lock(&self.running).insert(name.to_owned());
        }
        lock(&self.transitioning).insert(name.to_owned());
        let probe = self.is_running(name);
        match self.commit_locked(&guard, &[name.to_owned()], probe).await {
            Ok((generation, _)) => {
                self.record_success(name);
                Ok(transition_response(name, generation, self.is_running(name)))
            }
            Err(error) => {
                self.rollback_state(name, None, &running_before).await;
                Err(error)
            }
        }
    }

    /// `edit`: mutates a control-owned definition (options and/or
    /// rename). Type changes are rejected by the envelope.
    pub async fn edit(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("edit requires name"))?;
        let prior = self
            .definition(name)
            .ok_or_else(|| unknown_or_startup(name, &self.startup))?;
        let target_name = request.new_name.as_deref().unwrap_or(name);
        if target_name != name
            && (self.definition(target_name).is_some() || self.startup_name(target_name).is_some())
        {
            return Err(ControlError::NameCollision(target_name.to_owned()));
        }
        // Plan 292: changing the proxy username without
        // re-supplying the password would silently bind the new
        // name to the old verifier (one-way and undetectable
        // later), so it fails explicitly here.
        if prior.options.contains_key("proxy_password")
            && request.options.contains_key("proxy_username")
            && !request.options.contains_key("proxy_password")
            && request.options.get("proxy_username") != prior.options.get("proxy_username")
        {
            return Err(ControlError::InvalidOption {
                option: "proxy_username".to_owned(),
                reason: "changing proxy_username requires proxy_password",
            });
        }
        // Merge options over the prior definition, then normalize the
        // complete candidate before any side effect.
        let mut merged = prior.options.clone();
        for (key, value) in &request.options {
            merged.insert(key.clone(), value.clone());
        }
        let start_on_load = merged
            .get("start_on_load")
            .map(|value| value == "true")
            .unwrap_or(prior.start_on_load);
        let running_before = lock(&self.running).clone();
        if target_name != name {
            // Rename travels as delete-plus-create across separate
            // commits so the old listener is released (reaped) before
            // the new runtime binds the same socket. Every crash window
            // preserves both definitions (extra stopped definitions at
            // worst); nothing is ever lost between commits.
            return self
                .rename_locked(&guard, name, target_name, &prior, &merged, &running_before)
                .await;
        }
        let filter_root = self.store.filter_root();
        let candidate = normalize_definition_with_filter_root(
            target_name,
            prior.tunnel_type,
            &merged,
            start_on_load,
            Some(&filter_root),
            self.outbound_secrets().as_ref(),
        )?;
        let probe = build_control_spec_with_filter_root(&candidate, Some(&filter_root))?;
        self.check_listener_against_startup(&probe)?;
        let running_before = lock(&self.running).clone();
        let was_running = running_before.contains(target_name);
        lock(&self.definitions).insert(target_name.to_owned(), candidate.clone());
        lock(&self.transitioning).insert(target_name.to_owned());
        // A running definition whose edit restages its runtime travels
        // as stop-plus-start: staging rebinds sockets while the old
        // runtime still holds them, so the old runtime must drain (and
        // be reaped) before the new one stages.
        if was_running && self.edit_restages(&prior, &candidate) {
            return self
                .edit_restage_locked(&guard, target_name, &prior, &running_before)
                .await;
        }
        match self
            .commit_locked(&guard, &[target_name.to_owned()], false)
            .await
        {
            Ok((generation, _)) => {
                self.record_success(target_name);
                Ok(transition_response(
                    target_name,
                    generation,
                    self.is_running(target_name),
                ))
            }
            Err(error) => {
                self.rollback_state(target_name, Some((name, prior)), &running_before)
                    .await;
                Err(error)
            }
        }
    }

    /// Restaging edit on a running definition: stop phase drains the
    /// old runtime, start phase reaps (freeing the socket) and stages
    /// the new runtime. A stop-phase failure rolls the mirror fully
    /// back; a start-phase failure leaves the candidate committed but
    /// stopped with running intent restored, so the status reports
    /// failed and the next start converges.
    async fn edit_restage_locked(
        &self,
        guard: &tokio::sync::MutexGuard<'_, ()>,
        name: &str,
        prior: &ControlDefinition,
        running_before: &BTreeSet<String>,
    ) -> Result<serde_json::Value, ControlError> {
        // Plan 292: the restaged runtime rebuilds from the edited
        // definition, so any sweep reduction override is dropped
        // before the rebuild (explicit configuration supersedes
        // sweep state; failures leave a stopped runtime that an
        // operator start rebuilds fresh).
        self.manager.clear_reduced_shaping(name);
        lock(&self.running).remove(name);
        if let Err(error) = self.commit_locked(guard, &[name.to_owned()], false).await {
            self.rollback_state(name, Some((name, prior.clone())), running_before)
                .await;
            return Err(error);
        }
        lock(&self.running).insert(name.to_owned());
        match self.commit_locked(guard, &[name.to_owned()], true).await {
            Ok((generation, _)) => {
                self.record_success(name);
                Ok(transition_response(name, generation, true))
            }
            Err(error) => {
                lock(&self.running).insert(name.to_owned());
                lock(&self.transitioning).remove(name);
                Err(error)
            }
        }
    }

    /// Rename as crash-safe delete-plus-create: phase one commits the
    /// new stopped definition beside the old one, phase two removes
    /// the old definition (releasing its listener), phase three starts
    /// the new definition when the old one ran. Every intermediate
    /// generation preserves both definitions; a crash between commits
    /// leaves an extra stopped definition at worst, never a loss.
    async fn rename_locked(
        &self,
        guard: &tokio::sync::MutexGuard<'_, ()>,
        name: &str,
        target_name: &str,
        prior: &ControlDefinition,
        merged: &BTreeMap<String, String>,
        running_before: &BTreeSet<String>,
    ) -> Result<serde_json::Value, ControlError> {
        let was_running = running_before.contains(name);
        let start_on_load = merged
            .get("start_on_load")
            .map(|value| value == "true")
            .unwrap_or(prior.start_on_load);
        let filter_root = self.store.filter_root();
        let candidate = normalize_definition_with_filter_root(
            target_name,
            prior.tunnel_type,
            merged,
            start_on_load,
            Some(&filter_root),
            self.outbound_secrets().as_ref(),
        )?;
        let probe = build_control_spec_with_filter_root(&candidate, Some(&filter_root))?;
        self.check_listener_against_startup(&probe)?;
        // Phase one: add the new definition stopped. The old
        // name's sweep reduction override is dropped: the renamed
        // runtime rebuilds from the merged definition.
        self.manager.clear_reduced_shaping(name);
        lock(&self.definitions).insert(target_name.to_owned(), candidate);
        lock(&self.transitioning).insert(target_name.to_owned());
        if let Err(error) = self
            .commit_locked(guard, &[target_name.to_owned()], false)
            .await
        {
            self.rollback_state(target_name, None, running_before).await;
            return Err(error);
        }
        // Phase two: remove the old definition.
        lock(&self.definitions).remove(name);
        lock(&self.running).remove(name);
        lock(&self.supervised).remove(name);
        lock(&self.failed).remove(name);
        if let Err(error) = self.commit_locked(guard, &[name.to_owned()], false).await {
            // Restore the old definition: the store still publishes a
            // generation containing both rows (phase one published), so
            // the mirror is rebuilt to match it exactly.
            lock(&self.definitions).insert(name.to_owned(), prior.clone());
            if was_running {
                lock(&self.running).insert(name.to_owned());
            }
            lock(&self.transitioning).remove(target_name);
            lock(&self.transitioning).remove(name);
            return Err(error);
        }
        lock(&self.transitioning).remove(name);
        // Phase three: start the new definition when the old one ran.
        if was_running {
            lock(&self.running).insert(target_name.to_owned());
            match self
                .commit_locked(guard, &[target_name.to_owned()], true)
                .await
            {
                Ok((generation, _)) => {
                    self.record_success(target_name);
                    Ok(transition_response(target_name, generation, true))
                }
                Err(error) => {
                    lock(&self.running).remove(target_name);
                    lock(&self.transitioning).remove(target_name);
                    Err(error)
                }
            }
        } else {
            self.record_success(target_name);
            Ok(transition_response(
                target_name,
                self.store_generation(),
                false,
            ))
        }
    }

    /// `delete`: removes a control-owned definition and drains its runtime.
    pub async fn delete(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("delete requires name"))?;
        let prior = self
            .definition(name)
            .ok_or_else(|| unknown_or_startup(name, &self.startup))?;
        let running_before = lock(&self.running).clone();
        lock(&self.definitions).remove(name);
        lock(&self.running).remove(name);
        lock(&self.failed).remove(name);
        // Plan 292: a deleted service drops its sweep reduction
        // override so a later same-name tunnel starts fresh.
        self.manager.clear_reduced_shaping(name);
        lock(&self.transitioning).insert(name.to_owned());
        match self.commit_locked(&guard, &[name.to_owned()], false).await {
            Ok((generation, _)) => {
                lock(&self.supervised).remove(name);
                self.record_success(name);
                Ok(transition_response(name, generation, false))
            }
            Err(error) => {
                self.rollback_state(name, Some((name, prior)), &running_before)
                    .await;
                Err(error)
            }
        }
    }

    /// `start`: runtime action over the exact control-owned definition.
    /// Already-running names answer the current state without a new
    /// generation.
    pub async fn start(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("start requires name"))?;
        if self.definition(name).is_none() {
            return Err(unknown_or_startup(name, &self.startup));
        }
        if self.is_running(name) {
            return Ok(transition_response(name, self.store_generation(), true));
        }
        self.start_inner(name)
            .await
            .map(|generation| transition_response(name, generation, true))
    }

    /// `stop`: runtime action over the exact control-owned definition.
    /// Already-stopped names answer without a new generation.
    pub async fn stop(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("stop requires name"))?;
        if self.definition(name).is_none() {
            return Err(unknown_or_startup(name, &self.startup));
        }
        if !self.is_running(name) {
            lock(&self.transitioning).remove(name);
            return Ok(transition_response(name, self.store_generation(), false));
        }
        let running_before = lock(&self.running).clone();
        lock(&self.running).remove(name);
        lock(&self.failed).remove(name);
        lock(&self.transitioning).insert(name.to_owned());
        match self.commit_locked(&guard, &[name.to_owned()], false).await {
            Ok((generation, _)) => {
                lock(&self.supervised).remove(name);
                self.record_success(name);
                Ok(transition_response(name, generation, false))
            }
            Err(error) => {
                *lock(&self.running) = running_before;
                lock(&self.transitioning).remove(name);
                Err(error)
            }
        }
    }

    /// `restart`: stop then start over the exact control-owned
    /// definition under one op lock. Server identities persist across
    /// the transition through the manager's service-destination records.
    pub async fn restart(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let guard = self.op_lock.lock().await;
        let name = request
            .name
            .as_deref()
            .ok_or(ControlError::InvalidRequest("restart requires name"))?;
        if self.definition(name).is_none() {
            return Err(unknown_or_startup(name, &self.startup));
        }
        let running_before = lock(&self.running).clone();
        lock(&self.running).remove(name);
        lock(&self.failed).remove(name);
        lock(&self.transitioning).insert(name.to_owned());
        if let Err(error) = self.commit_locked(&guard, &[name.to_owned()], false).await {
            *lock(&self.running) = running_before;
            lock(&self.transitioning).remove(name);
            return Err(error);
        }
        lock(&self.supervised).remove(name);
        lock(&self.running).insert(name.to_owned());
        match self.commit_locked(&guard, &[name.to_owned()], true).await {
            Ok((generation, _)) => {
                self.record_success(name);
                Ok(transition_response(name, generation, true))
            }
            Err(error) => {
                *lock(&self.running) = running_before;
                lock(&self.transitioning).remove(name);
                Err(error)
            }
        }
    }

    /// Plan 292 idle sweep over control-owned runtimes: computes
    /// decisions from the manager, then applies each through the
    /// existing stop/restart transactions. Reduction overrides
    /// install during computation; a failed restart drops its
    /// override so the stored definition stays authoritative.
    /// Stale decisions (a tunnel stopped concurrently) are skipped
    /// by re-checking running intent before applying.
    pub async fn idle_sweep_once(&self, now_ms: u64) -> Vec<IdleSweepApplied> {
        let mut applied = Vec::new();
        for decision in self.manager.idle_sweep(now_ms) {
            if !self.is_running(&decision.spec_id) {
                continue;
            }
            let request = TunnelManagerRequest {
                action: match decision.action {
                    IdleSweepAction::Close => TunnelAction::Stop,
                    IdleSweepAction::RotateDestination
                    | IdleSweepAction::RebuildPools
                    | IdleSweepAction::ReducePools => TunnelAction::Restart,
                },
                all: false,
                name: Some(decision.spec_id.clone()),
                tunnel_type: None,
                new_name: None,
                options: BTreeMap::new(),
            };
            let outcome = match decision.action {
                IdleSweepAction::Close => self.stop(&request).await,
                IdleSweepAction::RotateDestination
                | IdleSweepAction::RebuildPools
                | IdleSweepAction::ReducePools => self.restart(&request).await,
            };
            let label = match decision.action {
                IdleSweepAction::Close => "close",
                IdleSweepAction::RotateDestination => "new-destination",
                IdleSweepAction::RebuildPools => "rebuild-pools",
                IdleSweepAction::ReducePools => "reduce-pools",
            };
            match outcome {
                Ok(_) => applied.push(IdleSweepApplied {
                    spec_id: decision.spec_id,
                    action: label,
                }),
                Err(error) => {
                    if decision.action == IdleSweepAction::ReducePools {
                        self.manager.clear_reduced_shaping(&decision.spec_id);
                    }
                    lock(&self.failed).insert(decision.spec_id, static_control_reason(&error));
                }
            }
        }
        applied
    }

    /// Spawns the daemon-owned periodic idle sweeper. Production
    /// calls this once after startup; tests drive
    /// [`Self::idle_sweep_once`] directly for determinism.
    pub fn spawn_idle_sweeper(
        self: &Arc<Self>,
        children: &i2pr_runtime::ChildScope,
        cancellation: &i2pr_runtime::CancellationToken,
    ) {
        let control = Arc::clone(self);
        let stop = cancellation.clone();
        if let Err(error) = children.spawn(move |task| async move {
            loop {
                tokio::select! {
                    () = task.cancelled() => break,
                    () = stop.cancelled() => break,
                    () = tokio::time::sleep(IDLE_SWEEP_INTERVAL) => {}
                }
                for applied in control
                    .idle_sweep_once(crate::service_tunnels::service_streaming_now_ms())
                    .await
                {
                    tracing::info!(
                        tunnel = %applied.spec_id,
                        action = %applied.action,
                        "idle sweep applied"
                    );
                }
            }
            Ok(())
        }) {
            tracing::warn!(error = ?error, "idle sweeper failed to spawn");
        }
    }

    /// Shared start path (also used at daemon startup): records running
    /// intent, commits, and starts supervisors for the transitioned
    /// diff. Acquires the op lock.
    async fn start_inner(&self, name: &str) -> Result<u64, ControlError> {
        let guard = self.op_lock.lock().await;
        let definition = self
            .definition(name)
            .ok_or_else(|| unknown_or_startup(name, &self.startup))?;
        if !definition.tunnel_type.has_plan291_backend() {
            return Err(ControlError::UnsupportedType(
                definition.tunnel_type.name().to_owned(),
            ));
        }
        // Plan 292: an operator start builds fresh pools from the
        // stored definition, dropping any sweep reduction override
        // (sweep restarts preserve it; only explicit starts clear).
        self.manager.clear_reduced_shaping(name);
        let running_before = lock(&self.running).clone();
        lock(&self.running).insert(name.to_owned());
        lock(&self.transitioning).insert(name.to_owned());
        match self.commit_locked(&guard, &[name.to_owned()], true).await {
            Ok((generation, _)) => {
                self.record_success(name);
                Ok(generation)
            }
            Err(error) => {
                *lock(&self.running) = running_before;
                lock(&self.transitioning).remove(name);
                Err(error)
            }
        }
    }

    /// Restores the durable mirror plus running intent after a failed
    /// commit. Staged-but-unpublished generation files are orphans;
    /// retention and startup cleanup remove them.
    /// Rolls the in-memory mirror back to its prior state **and** reconciles
    /// the shared manager to the rolled-back candidate (Plan 338).
    ///
    /// The reconcile is not optional. `commit_locked` stages and commits
    /// runtimes before it can fail on a supervisor start, a store publish, or
    /// the ELS2 material, so a mirror-only rollback leaves a runtime installed
    /// with no durable definition behind it — a service the operator cannot see
    /// and cannot stop. That hole predates Plan 338; it reached the
    /// coordinator's own failure paths, and Plan 337 made it the visible
    /// outcome of every refused encrypted create.
    ///
    /// Every failure path calls this, so no call site can grow a mirror-only
    /// rollback. A reconcile that itself fails is reported rather than
    /// swallowed: the caller returns the original error, and the ghost
    /// runtime is then a visible defect rather than a silent one.
    async fn rollback_state(
        &self,
        name: &str,
        prior: Option<(&str, ControlDefinition)>,
        running_before: &BTreeSet<String>,
    ) {
        Self::rollback_mirror(
            &self.definitions,
            &self.running,
            &self.transitioning,
            name,
            prior,
            running_before,
        );
        if !self.reconcile_back().await {
            // A rollback that cannot reconcile leaves the manager ahead of
            // the mirror. Report it rather than pretending the transition was
            // undone: the ghost runtime is then a visible defect.
            tracing::error!(
                "control transition rollback could not reconcile the shared manager; \
                 a runtime may outlive its durable definition"
            );
        }
    }

    /// The mirror-only half of [`Self::rollback_state`].
    fn rollback_mirror(
        definitions: &Mutex<BTreeMap<String, ControlDefinition>>,
        running: &Mutex<BTreeSet<String>>,
        transitioning: &Mutex<BTreeSet<String>>,
        name: &str,
        prior: Option<(&str, ControlDefinition)>,
        running_before: &BTreeSet<String>,
    ) {
        {
            let mut definitions = lock(definitions);
            match prior {
                Some((prior_name, definition)) => {
                    if prior_name != name {
                        definitions.remove(name);
                    }
                    definitions.insert(prior_name.to_owned(), definition);
                }
                None => {
                    definitions.remove(name);
                }
            }
        }
        *lock(running) = running_before.clone();
        lock(transitioning).remove(name);
    }

    /// Whether a name currently carries running intent.
    fn is_running(&self, name: &str) -> bool {
        lock(&self.running).contains(name)
    }

    /// Dispatches one decoded TunnelManager request.
    pub async fn dispatch(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        if request.all {
            return self.dispatch_all(request.action).await;
        }
        self.dispatch_single(request).await
    }

    async fn dispatch_single(
        &self,
        request: &TunnelManagerRequest,
    ) -> Result<serde_json::Value, ControlError> {
        match request.action {
            TunnelAction::Get => self.get(request.name.as_deref()),
            TunnelAction::Create => self.create(request).await,
            TunnelAction::Edit => self.edit(request).await,
            TunnelAction::Delete => self.delete(request).await,
            TunnelAction::Start => self.start(request).await,
            TunnelAction::Stop => self.stop(request).await,
            TunnelAction::Restart => self.restart(request).await,
        }
    }

    /// Applies a bulk lifecycle action to a bounded, deterministic
    /// snapshot of control-owned definitions only. Startup-owned entries
    /// remain under their original configuration owner and are excluded.
    async fn dispatch_all(&self, action: TunnelAction) -> Result<serde_json::Value, ControlError> {
        if !matches!(
            action,
            TunnelAction::Start | TunnelAction::Stop | TunnelAction::Restart
        ) {
            return Err(ControlError::UnsupportedOption("All".to_owned()));
        }
        let names: Vec<String> = lock(&self.definitions).keys().cloned().collect();
        if names.len() > MAX_SERVICE_TUNNELS {
            return Err(ControlError::AggregateRejected(
                "control-owned tunnel snapshot exceeds the service ceiling",
            ));
        }
        let mut results = Vec::with_capacity(names.len());
        for name in names {
            let single = TunnelManagerRequest {
                action,
                all: false,
                name: Some(name.clone()),
                tunnel_type: None,
                new_name: None,
                options: BTreeMap::new(),
            };
            match self.dispatch_single(&single).await {
                Ok(result) => results.push(serde_json::json!({
                    "name": name,
                    "status": "success",
                    "result": result,
                })),
                Err(_) => results.push(serde_json::json!({
                    "name": name,
                    "status": "error",
                })),
            }
        }
        Ok(serde_json::json!({ "results": results }))
    }
}

/// Locks a std mutex, recovering from poisoning with the inner value.
/// Poisoning implies a panicking holder elsewhere; inspection and
/// control prefer degraded continuity over a cascade. Every use is a
/// short critical section with no awaits and no I/O.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Mutation acknowledgement shape.
fn transition_response(name: &str, generation: u64, running: bool) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "running": running,
        "generation": generation,
    })
}

/// Classifies an unknown name as startup-rejected or unknown.
fn unknown_or_startup(name: &str, startup: &ServiceTunnelSet) -> ControlError {
    if startup.tunnels.iter().any(|spec| spec.id.as_str() == name) {
        ControlError::StartupMutationRejected(name.to_owned())
    } else {
        ControlError::UnknownTunnel(name.to_owned())
    }
}

/// Static reason for a daemon manager failure (never echoes values).
fn static_manager_reason(error: &crate::service_tunnels::ServiceTunnelError) -> &'static str {
    use crate::service_tunnels::ServiceTunnelError as E;
    match error {
        E::InvalidConfig(_) => "manager configuration rejected",
        E::DestinationRuntime(_) => "destination runtime failed",
        E::Bind(_) => "listener bind failed",
        E::Storage(_) => "service storage failed",
    }
}

/// Static reason for a runtime-neutral set failure.
fn static_manager_reason2(error: &i2pr_service_tunnels::ServiceTunnelError) -> &'static str {
    use i2pr_service_tunnels::ServiceTunnelError as E;
    match error {
        E::ExceedsCeiling { .. } => "aggregate exceeds a service ceiling",
        E::ContradictoryOptions { .. } => "candidate contradicts a service kind",
        E::DuplicateId { .. } => "duplicate service id",
        E::DuplicateListener { .. } => "duplicate listener",
        _ => "candidate validation failed",
    }
}

/// Static reason for a control failure (recovery paths only).
fn static_control_reason(error: &ControlError) -> &'static str {
    match error {
        ControlError::UnknownTunnel(_) => "unknown tunnel",
        ControlError::NameCollision(_) => "name collision",
        ControlError::StartupMutationRejected(_) => "startup-owned",
        ControlError::UnsupportedType(_) => "unsupported type",
        ControlError::UnsupportedOption(_) => "unsupported option",
        ControlError::InvalidOption { reason, .. } => reason,
        // The LeaseSet rule's reason is owned text and can name a mode, so the
        // recovery paths get a static label instead. That is also the safer
        // choice: `static_control_reason` feeds bounded diagnostic strings.
        ControlError::LeaseSetSecurityRejected(_) => "invalid LeaseSet security block",
        // Plan 342: same reasoning — the outproxy block's reason names the
        // seven keys, so recovery paths get a static label instead.
        ControlError::OutproxyBlockRejected(_) => "invalid outproxy block",
        ControlError::ContradictoryOptions { reason, .. } => reason,
        ControlError::InvalidRequest(reason) => reason,
        ControlError::AggregateRejected(reason) => reason,
        ControlError::Store(_) => "store failed",
        ControlError::Manager(reason) => reason,
        ControlError::PublishFailed { reason, .. } => reason,
        ControlError::Unavailable => "control unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_manager(data_dir: &Path) -> Arc<ServiceTunnelManager> {
        test_manager_with(data_dir, ServiceTunnelSet::new())
    }

    /// A manager built exactly as the composition root builds the shared
    /// one: over an explicit startup spec set, so `prepare` produces the
    /// startup-owned runtimes the product layer would produce.
    fn test_manager_with(data_dir: &Path, specs: ServiceTunnelSet) -> Arc<ServiceTunnelManager> {
        let config = crate::service_tunnels::ServiceTunnelManagerConfig {
            data_dir: data_dir.to_path_buf(),
            aggregate_connection_ceiling: 1024,
            per_service_connection_ceiling: 128,
            specs: Arc::new(specs),
            aliases: Arc::new(i2pr_service_tunnels::StaticAliasTable::new()),
        };
        Arc::new(ServiceTunnelManager::new(config).expect("manager builds"))
    }

    fn test_control(data_dir: &Path) -> TunnelControlState {
        let store = ControlStore::open(data_dir).expect("store opens");
        TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            test_manager(data_dir),
            test_outbound_secrets(),
        )
    }

    /// Plan 337: exactly **one** manager owns every service runtime.
    ///
    /// This is the positive form of the row Plan 334 could only state
    /// negatively. `TunnelControlState` is handed the composition root's
    /// manager and keeps exactly that `Arc`, so a runtime created through
    /// TunnelManager is the same runtime the product layer delivers and
    /// publishes through. The manager can hold a delivery capability —
    /// the product installs one on this same instance — so the row is
    /// about construction and injection, not about the type.
    #[test]
    fn plan337_control_reconciles_onto_the_product_manager() {
        let directory = TempDir::new().expect("temp dir");
        let shared = test_manager(directory.path());
        let store = ControlStore::open(directory.path()).expect("store opens");
        let control = TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            Arc::clone(&shared),
            test_outbound_secrets(),
        );
        assert!(
            Arc::ptr_eq(&control.manager, &shared),
            "the control state must hold the composition root's manager instance"
        );

        // The same instance accepts a delivery capability, which is what
        // the product installs once SSU2 starts. Before Plan 337 the
        // control built a private manager that nothing ever gave one.
        assert!(shared.uninstall_router_delivery().is_none());
        shared.install_router_delivery(crate::service_delivery::ServiceDestinationDelivery::new());
        assert!(
            shared.uninstall_router_delivery().is_some(),
            "the shared manager must be able to carry the product's delivery capability"
        );

        // A control-created server lands on that shared manager.
        block_on(control.create(&create_request(
            "srv337",
            TunnelType::HttpServer,
            server_options("127.0.0.1:8080"),
        )))
        .expect("control create succeeds");
        assert!(
            shared.has_runtime("srv337"),
            "a control-created runtime must be owned by the shared manager"
        );
    }

    /// Plan 337: a control-owned server is a real definition **and** it
    /// reaches the product layer's publication path, because the manager
    /// it reconciles onto is the one the product layer sweeps.
    ///
    /// The product's publication driver reads the manager's **committed
    /// generation** (`destination_group_runtimes` /
    /// `destination_group_has_server`), not the manager's original
    /// configured spec set. That is what makes a later control reconcile
    /// visible to it: the generation changes, the product rebuilds its
    /// destination lists, and the new server destination is marked
    /// publishable. This row pins that visibility, which Plan 334's
    /// negative pin could only show was absent.
    #[test]
    fn plan337_control_created_server_reaches_the_product_publication_sweep() {
        let directory = TempDir::new().expect("temp dir");
        let shared = test_manager(directory.path());
        let store = ControlStore::open(directory.path()).expect("store opens");
        let control = TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            Arc::clone(&shared),
            test_outbound_secrets(),
        );

        block_on(control.create(&create_request(
            "encsrv",
            TunnelType::HttpServer,
            server_options("127.0.0.1:8080"),
        )))
        .expect("control create succeeds");
        assert!(shared.has_runtime("encsrv"));

        // The shared manager now owns a server runtime, and the product
        // layer's sweep discovers it from the committed generation. This
        // is the seam that was never reached before Plan 337.
        let runtime = shared
            .service_runtime_for_spec("encsrv")
            .expect("the shared manager owns the control-created runtime");
        assert!(
            shared.destination_group_has_server(runtime.destination_id),
            "the product sweep must see the control-created server as publishable"
        );
        assert!(
            shared
                .destination_group_runtimes()
                .iter()
                .any(|current| current.destination_id == runtime.destination_id),
            "the control-created destination must appear in the sweep's runtime list"
        );
    }

    /// Plan 338: a control-created **encrypted** server publishes a type-5
    /// record at the day's blinded storage key and exposes a resolving
    /// address.
    ///
    /// This is the positive form of the row Plan 337 could only state as a
    /// refusal. Plan 337's failure was a **store-path** error, not a missing
    /// capability: every server group is already persistent
    /// (`ServiceTunnelSet::destination_groups` sets `persistent` for
    /// `kind.is_server()`), so the manager has always written a
    /// `ServiceDestinationRecord` for a control-created server — at
    /// `for_group(data_dir, spec_id)`. The ELS2 loader read
    /// `for_service(data_dir, spec_id)` instead, so it found nothing and the
    /// create was refused. Plan 338 routes the material through
    /// `ServiceTunnelManager::service_identity_record`, which is the same code
    /// that wrote the record.
    #[test]
    fn plan338_control_created_encrypted_server_publishes_type5_at_its_blinded_key() {
        let directory = TempDir::new().expect("temp dir");
        let shared = test_manager(directory.path());
        let store = ControlStore::open(directory.path()).expect("store opens");
        let control = TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            Arc::clone(&shared),
            test_outbound_secrets(),
        );

        let mut options = server_options("127.0.0.1:8080");
        options.insert("encrypt_lease_set".to_owned(), "blinded".to_owned());
        block_on(control.create(&create_request("encsrv", TunnelType::HttpServer, options)))
            .expect("an encrypted control-created server must now be accepted");
        assert!(shared.has_runtime("encsrv"));

        // The material was installed, derived from this service's own
        // persisted identity.
        let material = shared
            .els2_material_for_spec("encsrv")
            .expect("publication material is installed");

        // The control surface reports a real address for it, decoded with the
        // protocol's own decoder rather than a hand-rolled one.
        let status = control.get(Some("encsrv")).expect("get");
        let address = status["encrypted_address"]
            .as_str()
            .expect("encrypted_address is a string")
            .to_owned();
        assert!(address.ends_with(".b32.i2p"), "got {address}");
        assert_eq!(
            address,
            material.address().expect("material encodes an address"),
            "the reported address must come from the published material"
        );
        let parsed = i2pr_proto::EncryptedServiceAddress::from_text(&address)
            .expect("the reported address decodes as an encrypted service address");
        assert_eq!(
            parsed.unblinded_sigtype(),
            i2pr_proto::B32_UNBLINDED_SIGTYPE_ED25519,
            "an unblinded address names the Ed25519 unblinded type"
        );
        assert_eq!(
            parsed.blinded_sigtype(),
            i2pr_proto::B32_BLINDED_SIGTYPE,
            "a blinded address names the blinded signature type"
        );
        // The address flags follow the *mode*, not the fact of blinding: a
        // client only needs the lookup secret when the mode has one.
        assert_eq!(
            parsed.requires_blinding_secret(),
            material.requires_blinding_secret(),
            "the address flag must match the mode"
        );
        assert_eq!(parsed.requires_client_key(), material.requires_client_key());
    }

    /// Plan 338: the same path with a mode that does carry a lookup secret and
    /// a client authorization, so the address carries **both** flag bits. The
    /// row above cannot cover this: it uses a mode with neither.
    #[test]
    fn plan338_control_created_authorized_server_address_carries_both_flags() {
        let directory = TempDir::new().expect("temp dir");
        let shared = test_manager(directory.path());
        let store = ControlStore::open(directory.path()).expect("store opens");
        let control = TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            Arc::clone(&shared),
            test_outbound_secrets(),
        );

        let client_key = "ab".repeat(32);
        let mut options = server_options("127.0.0.1:8080");
        options.insert(
            "encrypt_lease_set".to_owned(),
            "encrypted with lookup password and per-user key (psk)".to_owned(),
        );
        options.insert("leaseset_password".to_owned(), "plan338-secret".to_owned());
        options.insert(
            "leaseset_client_auth".to_owned(),
            format!("client0:{client_key}"),
        );
        block_on(control.create(&create_request("authsrv", TunnelType::HttpServer, options)))
            .expect("an authorized encrypted control-created server must be accepted");

        let material = shared
            .els2_material_for_spec("authsrv")
            .expect("publication material is installed");
        let address = control.get(Some("authsrv")).expect("get")["encrypted_address"]
            .as_str()
            .expect("encrypted_address is a string")
            .to_owned();
        let parsed = i2pr_proto::EncryptedServiceAddress::from_text(&address)
            .expect("the reported address decodes");
        assert!(
            parsed.requires_blinding_secret(),
            "a mode with a lookup secret must set the blinding-secret flag"
        );
        assert!(
            parsed.requires_client_key(),
            "a mode with a per-user key must set the client-key flag"
        );
        assert_eq!(material.client_count(), 1, "one authorized client");
        // The secret itself is not in the response.
        let status = control.get(Some("authsrv")).expect("get").to_string();
        assert!(!status.contains("plan338-secret"));
        assert!(!status.contains(client_key.as_str()));
    }

    /// Plan 338: the coordinator's rollback leaves **no** ghost runtime.
    ///
    /// `commit_locked` stages and commits runtimes before it can fail on a
    /// supervisor start (`sync_names`), a store publish, or the ELS2 material
    /// (`sync_els2_materials`). A mirror-only rollback at any of those points
    /// leaves a service the operator cannot see, stop, or delete.
    ///
    /// This row drives the rollback path **directly** rather than waiting for a
    /// natural post-reconcile failure, because none of the three failure
    /// classes can be triggered deterministically from an input: `sync_names`
    /// needs a supervisor that fails to start, the store publish needs a write
    /// that fails after the generation is staged, and the ELS2 material now
    /// resolves for every mode a control surface can express. Invoking the same
    /// helper the failure paths call keeps the row meaningful — it fails if the
    /// reconcile half is removed — while the three call sites are covered
    /// structurally by every one of them awaiting this helper.
    #[test]
    fn plan338_a_rolled_back_transition_leaves_no_ghost_runtime() {
        let directory = TempDir::new().expect("temp dir");
        let shared = test_manager(directory.path());
        let store = ControlStore::open(directory.path()).expect("store opens");
        let control = TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            Arc::clone(&shared),
            test_outbound_secrets(),
        );

        // A sibling that must survive the rollback.
        block_on(control.create(&create_request(
            "keeper",
            TunnelType::HttpServer,
            server_options("127.0.0.1:9090"),
        )))
        .expect("the surviving service is created");
        let running_before = lock(&control.running).clone();

        // A second service whose runtime is now installed, exactly as it would
        // be at the point `commit_locked` fails.
        block_on(control.create(&create_request(
            "ghost",
            TunnelType::HttpServer,
            server_options("127.0.0.1:9091"),
        )))
        .expect("the second service is created");
        assert!(shared.has_runtime("ghost"), "the runtime is staged");
        lock(&control.transitioning).insert("ghost".to_owned());

        // The failure path: roll the mirror back and reconcile.
        block_on(control.rollback_state("ghost", None, &running_before));

        assert!(
            !shared.has_runtime("ghost"),
            "a rolled-back transition must not leave a runtime on the shared manager"
        );
        assert!(
            lock(&control.definitions).get("ghost").is_none(),
            "a rolled-back transition must not leave a durable definition"
        );
        assert!(
            lock(&control.transitioning).get("ghost").is_none(),
            "a rolled-back transition must clear the transitioning marker"
        );
        assert!(
            shared.has_runtime("keeper"),
            "a rollback must not disturb a sibling"
        );
        assert_mirror_and_manager_agree(&control, &shared, "after a rollback");

        // A rollback that restores a prior definition under the same name
        // (the `edit` shape) reconciles the definition it restored.
        block_on(control.create(&create_request(
            "renamed",
            TunnelType::HttpServer,
            server_options("127.0.0.1:9093"),
        )))
        .expect("a service is created");
        let prior = lock(&control.definitions)
            .get("renamed")
            .cloned()
            .expect("the prior definition");
        let running_now = lock(&control.running).clone();
        lock(&control.definitions).insert("renamed-2".to_owned(), prior.clone());
        assert!(shared.has_runtime("renamed"));
        block_on(control.rollback_state("renamed-2", Some(("renamed", prior)), &running_now));
        assert!(
            !shared.has_runtime("renamed-2"),
            "a restored-under-another-name rollback must not leave the failed name's runtime"
        );
        assert!(
            shared.has_runtime("renamed"),
            "the restored name keeps its runtime"
        );
        assert_mirror_and_manager_agree(&control, &shared, "after a renaming rollback");
    }

    /// Asserts that the shared manager and the durable mirror name the same
    /// set of running control-owned services, in both directions.
    fn assert_mirror_and_manager_agree(
        control: &TunnelControlState,
        manager: &Arc<ServiceTunnelManager>,
        context: &str,
    ) {
        let running = lock(&control.running).clone();
        for name in running.iter() {
            assert!(
                manager.has_runtime(name),
                "{context}: running service {name} has no runtime"
            );
        }
        for runtime in manager.all_service_runtimes() {
            let owned = running.contains(&runtime.spec_id)
                || control.startup_name(&runtime.spec_id).is_some();
            assert!(
                owned,
                "{context}: runtime {} is neither control-running nor startup-owned",
                runtime.spec_id
            );
        }
    }

    /// Plan 338 negative: a mode that names a client-authorization scheme the
    /// service cannot satisfy is still refused, and a supplied-but-unused
    /// secret is still an error. The ELS2 path being wired must not weaken any
    /// Plan 334 validation rule.
    #[test]
    fn plan338_els2_wiring_does_not_weaken_the_block_validation() {
        let directory = TempDir::new().expect("temp dir");
        let shared = test_manager(directory.path());
        let store = ControlStore::open(directory.path()).expect("store opens");
        let control = TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            Arc::clone(&shared),
            test_outbound_secrets(),
        );

        // A per-user-key mode with no client authorizations at all.
        let mut options = server_options("127.0.0.1:8080");
        options.insert(
            "encrypt_lease_set".to_owned(),
            "encrypted with per-user key (dh)".to_owned(),
        );
        let refused =
            block_on(control.create(&create_request("noauth", TunnelType::HttpServer, options)));
        assert!(refused.is_err(), "a per-user-key mode needs its keys");
        assert!(
            !shared.has_runtime("noauth"),
            "a refused definition must not install a runtime"
        );
        assert!(shared.els2_material_for_spec("noauth").is_none());

        // A lookup secret supplied to a mode that does not use one.
        let mut options = server_options("127.0.0.1:8080");
        options.insert("encrypt_lease_set".to_owned(), "blinded".to_owned());
        options.insert("leaseset_password".to_owned(), "unused".to_owned());
        let refused =
            block_on(control.create(&create_request("unused", TunnelType::HttpServer, options)));
        assert!(refused.is_err(), "a supplied-but-unused secret is an error");
        assert!(!shared.has_runtime("unused"));
        assert!(shared.els2_material_for_spec("unused").is_none());
    }

    /// Plan 337 isolation: a control delete removes only its own runtime.
    /// A startup-owned sibling sharing the manager survives untouched —
    /// the property that made a shared manager safe to adopt.
    #[test]
    fn plan337_control_delete_leaves_a_startup_sibling_intact() {
        let directory = TempDir::new().expect("temp dir");
        let mut startup_specs = vec![
            build_control_spec(&ControlDefinition {
                name: "startup-srv".to_owned(),
                tunnel_type: TunnelType::HttpServer,
                options: server_options("127.0.0.1:9090"),
                start_on_load: false,
            })
            .expect("startup spec"),
        ];
        // Daemon configuration marks a startup tunnel enabled; the
        // control-side builder leaves it off, so mirror the configured
        // shape the shared manager is actually constructed over.
        startup_specs[0].enabled = true;
        let startup = ServiceTunnelSet {
            tunnels: startup_specs,
        };
        let startup_spec = startup.tunnels[0].clone();
        let shared = test_manager_with(directory.path(), startup.clone());
        // Prepare the shared manager the way the product does, so the
        // startup-owned spec is a live runtime before control touches it.
        block_on(async {
            let runtimes = shared
                .prepare()
                .await
                .expect("the shared manager prepares startup-owned specs");
            assert_eq!(runtimes.len(), 1, "one startup-owned runtime");
        });
        let store = ControlStore::open(directory.path()).expect("store opens");
        let control =
            TunnelControlState::new(store, startup, Arc::clone(&shared), test_outbound_secrets());
        let generation_before = shared.committed_generation_id();

        block_on(control.create(&create_request(
            "ctrl-srv",
            TunnelType::HttpServer,
            server_options("127.0.0.1:9091"),
        )))
        .expect("control create succeeds");
        assert!(shared.has_runtime("startup-srv"));
        assert!(shared.has_runtime("ctrl-srv"));
        assert_ne!(
            shared.committed_generation_id(),
            generation_before,
            "a control reconcile must advance the shared manager's generation"
        );

        block_on(control.delete(&name_request(TunnelAction::Delete, "ctrl-srv")))
            .expect("control delete succeeds");
        assert!(
            !shared.has_runtime("ctrl-srv"),
            "the control-owned runtime is gone"
        );
        assert!(
            shared.has_runtime("startup-srv"),
            "a startup-owned sibling must survive a control delete"
        );
        // The sibling's spec is untouched: a shared manager must not
        // rewrite startup configuration through a control transaction.
        assert_eq!(
            shared
                .service_runtime_for_spec("startup-srv")
                .expect("startup runtime survives")
                .spec_id,
            startup_spec.id.as_str()
        );
    }

    fn create_request(
        name: &str,
        tunnel_type: TunnelType,
        options: BTreeMap<String, String>,
    ) -> TunnelManagerRequest {
        TunnelManagerRequest {
            action: TunnelAction::Create,
            all: false,
            name: Some(name.to_owned()),
            tunnel_type: Some(tunnel_type),
            new_name: None,
            options,
        }
    }

    /// A name-only request for the actions that take no type or options
    /// (delete / start / stop / restart).
    fn name_request(action: TunnelAction, name: &str) -> TunnelManagerRequest {
        TunnelManagerRequest {
            action,
            all: false,
            name: Some(name.to_owned()),
            tunnel_type: None,
            new_name: None,
            options: BTreeMap::new(),
        }
    }

    fn named_request(action: TunnelAction, name: &str) -> TunnelManagerRequest {
        TunnelManagerRequest {
            action,
            all: false,
            name: Some(name.to_owned()),
            tunnel_type: None,
            new_name: None,
            options: BTreeMap::new(),
        }
    }

    /// Plan 342: the fail-closed outbound-credential owner.
    ///
    /// Tests that do not exercise a credential get `NoOutboundSecrets`, which
    /// is the honest value: no identity was loaded, so no store exists. A test
    /// that *does* exercise a credential installs a real router-bound store,
    /// so an inert-acceptance bug cannot hide behind this helper.
    fn test_outbound_secrets() -> Arc<dyn RouterSecretOwner> {
        Arc::new(i2pr_service_tunnels::outbound_secret::NoOutboundSecrets)
    }

    fn client_options(destination: &str, port: u16) -> BTreeMap<String, String> {
        let mut options = BTreeMap::new();
        options.insert("target_destination".to_owned(), destination.to_owned());
        options.insert("listen_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("listen_port".to_owned(), port.to_string());
        options
    }

    fn server_options(target: &str) -> BTreeMap<String, String> {
        let (host, port) = target.rsplit_once(':').expect("host:port");
        let mut options = BTreeMap::new();
        options.insert("target_host".to_owned(), host.to_owned());
        options.insert("target_port".to_owned(), port.to_owned());
        options
    }

    /// Distinct loopback ports for multi-service tests. Each call
    /// reserves a unique block process-wide (atomic counter), so
    /// parallel tests never share a port; the base spreads by process
    /// id so parallel test binaries rarely share a block either.
    /// Services bind them immediately, so the reservation race is
    /// negligible.
    fn distinct_ports(count: u16) -> Vec<u16> {
        use std::sync::atomic::{AtomicU16, Ordering};
        static NEXT_BLOCK: AtomicU16 = AtomicU16::new(0);
        static BASE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
        let base = *BASE.get_or_init(|| 25_000 + (std::process::id() % 5_000) as u16);
        let block = NEXT_BLOCK.fetch_add(count, Ordering::Relaxed);
        (0..count).map(|n| base + (block + n) * 37).collect()
    }

    // All control tests run through a throwaway current-thread runtime:
    // the coordinator awaits manager reconcile and file syncs.
    fn block_on<F, T>(future: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    fn test_scope() -> (i2pr_runtime::ChildScope, i2pr_runtime::CancellationToken) {
        use i2pr_runtime::{CancellationToken, ChildFailurePolicy, ChildScope};
        let parent = CancellationToken::new();
        let scope = ChildScope::for_test(&parent, ChildFailurePolicy::FailParent);
        (scope, parent)
    }

    #[test]
    fn plan297_use_ssl_generations_carry_no_key_material() {
        // Plan 297: `use_ssl` persists as a boolean; TLS identity
        // and trust material lives only in daemon configuration
        // files, never in control definitions, mirrors, or
        // generation files.
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        let mut options = server_options("127.0.0.1:9443");
        options.insert("use_ssl".to_owned(), "true".to_owned());
        let definition = normalize_definition("redact", TunnelType::Server, &options, false)
            .expect("use_ssl admitted");
        let mut definitions = BTreeMap::new();
        definitions.insert("redact".to_owned(), definition);
        let staged = store.stage(&definitions).expect("stage");
        store.publish(staged).expect("publish");
        let raw = std::fs::read(store.generation_path(staged)).expect("generation bytes");
        let text = String::from_utf8(raw).expect("generation is JSON text");
        assert!(text.contains("use_ssl"), "the flag persists");
        for marker in [
            "-----BEGIN",
            "PRIVATE KEY",
            "CERTIFICATE",
            "sha256",
            "BEGIN RSA",
        ] {
            assert!(
                !text.contains(marker),
                "generation files never carry key material"
            );
        }
        let debug = format!("{:?}", definitions["redact"]);
        assert!(
            !debug.contains("PRIVATE"),
            "definition Debug never carries secrets"
        );
    }

    #[test]
    fn plan289_store_round_trip_and_recovery() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        assert_eq!(store.current_id(), 0);
        let mut definitions = BTreeMap::new();
        definitions.insert(
            "alpha".to_owned(),
            ControlDefinition {
                name: "alpha".to_owned(),
                tunnel_type: TunnelType::Client,
                options: client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0),
                start_on_load: true,
            },
        );
        let first = store.stage(&definitions).expect("stage");
        store.publish(first).expect("publish");
        assert_eq!(store.current_id(), first);
        let loaded = store.load().expect("load");
        assert_eq!(loaded.generation_id, first);
        assert_eq!(loaded.definitions.len(), 1);
        assert_eq!(loaded.definitions[0].name, "alpha");
        // Second generation publishes; the first stays as the prior.
        let second = store.stage(&BTreeMap::new()).expect("stage empty");
        store.publish(second).expect("publish");
        assert_eq!(store.current_id(), second);
        // Corrupt the latest file: recovery falls back to the prior.
        std::fs::write(store.generation_path(second), b"{corrupt").expect("corrupt");
        let recovered = store.load().expect("recovery loads");
        assert_eq!(recovered.generation_id, first);
        // Corrupt the pointer too: scan fallback finds the newest valid.
        std::fs::write(store.pointer_path(), b"{corrupt").expect("corrupt pointer");
        let scanned = store.load().expect("scan loads");
        assert_eq!(scanned.generation_id, first);
    }

    #[test]
    fn plan289_store_rejects_symlinks_and_oversized_reads() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        let mut definitions = BTreeMap::new();
        definitions.insert(
            "alpha".to_owned(),
            ControlDefinition {
                name: "alpha".to_owned(),
                tunnel_type: TunnelType::Client,
                options: BTreeMap::new(),
                start_on_load: false,
            },
        );
        let id = store.stage(&definitions).expect("stage");
        store.publish(id).expect("publish");
        // Oversized generation file: bounded read rejects before parse.
        let big = vec![b'x'; MAX_GENERATION_BYTES + 1];
        std::fs::write(store.generation_path(999), &big).expect("write big");
        assert_eq!(store.load().expect("load skips big").generation_id, id);
        std::fs::remove_file(store.generation_path(999)).expect("remove big");
        // Symlink generation file: rejected as non-regular.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(store.generation_path(id), store.generation_path(1000))
                .expect("symlink");
            assert_eq!(store.load().expect("load skips symlink").generation_id, id);
            std::fs::remove_file(store.generation_path(1000)).expect("remove link");
        }
        // Symlink root: open refuses.
        #[cfg(unix)]
        {
            let link_parent = tempfile::tempdir().expect("tempdir");
            let link = link_parent.path().join("linked");
            std::os::unix::fs::symlink(directory.path(), &link).expect("symlink root");
            let root = link.join(CONTROL_STATE_SUBDIR).join(CONTROL_TUNNELS_SUBDIR);
            std::fs::create_dir_all(&root).expect("create through link");
            assert!(
                matches!(ControlStore::open(&link), Err(StoreError::NotRegular)),
                "symlink data dir refused"
            );
        }
    }

    #[test]
    fn plan289_store_enforces_ceilings_and_retention() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        // 33 definitions exceed the 32-service ceiling at stage time.
        let mut definitions = BTreeMap::new();
        for n in 0..MAX_SERVICE_TUNNELS + 1 {
            definitions.insert(
                format!("tunnel-{n:03}"),
                ControlDefinition {
                    name: format!("tunnel-{n:03}"),
                    tunnel_type: TunnelType::Client,
                    options: BTreeMap::new(),
                    start_on_load: false,
                },
            );
        }
        assert_eq!(store.stage(&definitions), Err(StoreError::OverBound));
        // Three publications keep current plus prior only.
        let mut ids = Vec::new();
        for _ in 0..3 {
            let id = store.stage(&BTreeMap::new()).expect("stage");
            store.publish(id).expect("publish");
            ids.push(id);
        }
        assert_eq!(store.current_id(), ids[2]);
        assert!(!store.generation_path(ids[0]).exists(), "oldest pruned");
        assert!(store.generation_path(ids[1]).exists(), "prior kept");
        assert!(store.generation_path(ids[2]).exists(), "current kept");
    }

    #[test]
    fn plan289_store_orphan_cleanup() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("open");
        // Two staged files, nothing published: staged files are never
        // loadable, so cleanup drops them all.
        let first = store.stage(&BTreeMap::new()).expect("stage");
        let second = store.stage(&BTreeMap::new()).expect("stage");
        assert_ne!(first, second);
        store.cleanup_orphans();
        assert_eq!(store.list_staged_ids().expect("list").len(), 0);
        assert_eq!(store.current_id(), 0);
        // Publish, then stage an orphan: cleanup drops the staged file
        // while the published generation stays.
        let published = store.stage(&BTreeMap::new()).expect("stage");
        store.publish(published).expect("publish");
        let _ = store.stage(&BTreeMap::new()).expect("stage orphan");
        store.cleanup_orphans();
        assert_eq!(store.list_staged_ids().expect("staged").len(), 0);
        assert_eq!(store.current_id(), published);
    }

    #[test]
    fn plan290_family_mapping_ten_backends() {
        use i2pr_service_tunnels::ServiceTunnelKind;
        for (tunnel_type, kind) in [
            (TunnelType::Client, ServiceTunnelKind::GenericClient),
            (TunnelType::Server, ServiceTunnelKind::GenericServer),
            (TunnelType::HttpClient, ServiceTunnelKind::HttpClient),
            (TunnelType::Socks, ServiceTunnelKind::Socks5Client),
            (TunnelType::IrcClient, ServiceTunnelKind::IrcClient),
            (TunnelType::IrcServer, ServiceTunnelKind::IrcServer),
            // Plan 290: composed families over the existing M10
            // manager and shared primitives.
            (TunnelType::ConnectClient, ServiceTunnelKind::ConnectClient),
            (TunnelType::SocksIrc, ServiceTunnelKind::SocksIrc),
            (TunnelType::HttpServer, ServiceTunnelKind::HttpServer),
            (
                TunnelType::HttpBidirServer,
                ServiceTunnelKind::HttpBidirServer,
            ),
            // Plan 291: Streamr families over the
            // repliable-datagram substrate.
            (TunnelType::StreamrClient, ServiceTunnelKind::StreamrClient),
            (TunnelType::StreamrServer, ServiceTunnelKind::StreamrServer),
        ] {
            assert_eq!(map_tunnel_type(tunnel_type).expect("backend"), kind);
        }
    }

    #[test]
    fn plan323_shared_uses_one_explicit_client_destination_group() {
        use i2pr_service_tunnels::{DestinationGroupKey, DestinationPolicy, ServiceTunnelSet};

        let mut definitions = Vec::new();
        for (name, tunnel_type) in [
            ("shared-http", TunnelType::HttpClient),
            ("shared-socks", TunnelType::Socks),
        ] {
            let definition = ControlDefinition {
                name: name.to_owned(),
                tunnel_type,
                options: BTreeMap::from([
                    ("shared".to_owned(), "true".to_owned()),
                    (
                        "target_destination".to_owned(),
                        format!("{}.b32.i2p", "a".repeat(52)),
                    ),
                ]),
                start_on_load: false,
            };
            let spec = build_control_spec(&definition).expect("shared client spec");
            assert!(matches!(
                spec.policy,
                DestinationPolicy::SharedClientGroup(_)
            ));
            definitions.push(spec);
        }
        let groups = ServiceTunnelSet {
            tunnels: definitions,
        }
        .destination_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 2);
        assert_eq!(
            groups[0].key,
            DestinationGroupKey::Explicit(
                DestinationGroupId::parse("i2pcontrol-shared-client").expect("group id")
            )
        );

        let persistent = ControlDefinition {
            name: "persistent-client".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                ("persistent_client_key".to_owned(), "true".to_owned()),
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
            ]),
            start_on_load: false,
        };
        let persistent_spec = build_control_spec(&persistent).expect("persistent client spec");
        assert_eq!(persistent_spec.policy, DestinationPolicy::PersistentClient);
        assert!(
            ServiceTunnelSet {
                tunnels: vec![persistent_spec],
            }
            .destination_groups()[0]
                .persistent
        );

        let from_new_dest = ControlDefinition {
            name: "new-dest-persistent".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                ("new_dest".to_owned(), "2".to_owned()),
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
            ]),
            start_on_load: false,
        };
        assert_eq!(
            build_control_spec(&from_new_dest)
                .expect("NewDest=2 selects persistence")
                .policy,
            DestinationPolicy::PersistentClient
        );
        let new_dest_on_resume = ControlDefinition {
            name: "new-dest-on-resume".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                ("new_dest".to_owned(), "1".to_owned()),
                ("close_on_idle".to_owned(), "true".to_owned()),
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
            ]),
            start_on_load: false,
        };
        let resume_spec = build_control_spec(&new_dest_on_resume)
            .expect("NewDest=1 selects the ephemeral idle-rotation owner");
        assert_eq!(resume_spec.policy, DestinationPolicy::Dedicated);
        assert!(resume_spec.idle.close_on_idle);
        assert!(resume_spec.idle.rotate_destination_on_idle);
        assert!(!resume_spec.idle.new_dest_on_idle);

        let shared_new_dest = ControlDefinition {
            name: "shared-new-dest-on-resume".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                ("new_dest".to_owned(), "1".to_owned()),
                ("close_on_idle".to_owned(), "true".to_owned()),
                ("shared".to_owned(), "true".to_owned()),
            ]),
            start_on_load: false,
        };
        assert!(matches!(
            build_control_spec(&shared_new_dest),
            Err(ControlError::ContradictoryOptions { .. })
        ));
        let conflicting = ControlDefinition {
            name: "new-dest-conflict".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                ("new_dest".to_owned(), "0".to_owned()),
                ("persistent_client_key".to_owned(), "true".to_owned()),
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
            ]),
            start_on_load: false,
        };
        assert!(matches!(
            build_control_spec(&conflicting),
            Err(ControlError::ContradictoryOptions { .. })
        ));

        let shared_persistent = ControlDefinition {
            name: "shared-persistent".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                ("shared".to_owned(), "true".to_owned()),
                ("persistent_client_key".to_owned(), "true".to_owned()),
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
            ]),
            start_on_load: false,
        };
        let shared_persistent =
            build_control_spec(&shared_persistent).expect("shared persistent client spec");
        assert!(matches!(
            shared_persistent.policy,
            DestinationPolicy::PersistentSharedClientGroup(_)
        ));

        for tunnel_type in [TunnelType::Server, TunnelType::StreamrClient] {
            let definition = ControlDefinition {
                name: "bad-shared".to_owned(),
                tunnel_type,
                options: BTreeMap::from([("shared".to_owned(), "true".to_owned())]),
                start_on_load: false,
            };
            assert!(build_control_spec(&definition).is_err());
        }
    }

    #[test]
    fn plan323_priv_key_file_is_a_confined_persistent_logical_reference() {
        use i2pr_service_tunnels::{DestinationPolicy, ServiceKeyReference, ServiceTunnelSet};

        let client = ControlDefinition {
            name: "keyed-client".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
                ("priv_key_file".to_owned(), "primary".to_owned()),
            ]),
            start_on_load: false,
        };
        let spec = build_control_spec(&client).expect("client key reference");
        assert_eq!(
            spec.policy,
            DestinationPolicy::KeyReference(
                ServiceKeyReference::parse("primary").expect("key reference")
            )
        );
        assert!(
            ServiceTunnelSet {
                tunnels: vec![spec],
            }
            .destination_groups()[0]
                .persistent
        );

        let server = ControlDefinition {
            name: "keyed-server".to_owned(),
            tunnel_type: TunnelType::Server,
            options: BTreeMap::from([
                ("target_host".to_owned(), "127.0.0.1".to_owned()),
                ("target_port".to_owned(), "8080".to_owned()),
                ("priv_key_file".to_owned(), "site-key".to_owned()),
            ]),
            start_on_load: false,
        };
        assert!(matches!(
            build_control_spec(&server)
                .expect("server key reference")
                .policy,
            DestinationPolicy::KeyReference(_)
        ));

        let traversal = ControlDefinition {
            name: "unsafe-keyed-client".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
                ("priv_key_file".to_owned(), "../escape".to_owned()),
            ]),
            start_on_load: false,
        };
        assert!(matches!(
            build_control_spec(&traversal),
            Err(ControlError::InvalidOption { .. })
        ));

        let shared = ControlDefinition {
            name: "shared-keyed-client".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
                ("priv_key_file".to_owned(), "primary".to_owned()),
                ("shared".to_owned(), "true".to_owned()),
            ]),
            start_on_load: false,
        };
        assert!(matches!(
            build_control_spec(&shared),
            Err(ControlError::ContradictoryOptions { .. })
        ));
    }

    #[test]
    fn plan323_connection_rates_use_authenticated_server_admission() {
        let definition = ControlDefinition {
            name: "server-rates".to_owned(),
            tunnel_type: TunnelType::Server,
            options: BTreeMap::from([
                ("target_host".to_owned(), "127.0.0.1".to_owned()),
                ("target_port".to_owned(), "8080".to_owned()),
                ("client_per_minute".to_owned(), "2".to_owned()),
                ("client_per_hour".to_owned(), "10".to_owned()),
                ("client_per_day".to_owned(), "30".to_owned()),
                ("total_in_per_minute".to_owned(), "20".to_owned()),
                ("total_in_per_hour".to_owned(), "100".to_owned()),
                ("total_in_per_day".to_owned(), "300".to_owned()),
            ]),
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("server connection limits map");
        assert_eq!(spec.access.connection_rates.client_per_minute, 2);
        assert_eq!(spec.access.connection_rates.client_per_hour, 10);
        assert_eq!(spec.access.connection_rates.client_per_day, 30);
        assert_eq!(spec.access.connection_rates.total_per_minute, 20);
        assert_eq!(spec.access.connection_rates.total_per_hour, 100);
        assert_eq!(spec.access.connection_rates.total_per_day, 300);

        let irc_server = ControlDefinition {
            name: "irc-server-rates".to_owned(),
            tunnel_type: TunnelType::IrcServer,
            options: BTreeMap::from([
                ("target_host".to_owned(), "127.0.0.1".to_owned()),
                ("target_port".to_owned(), "6667".to_owned()),
                ("client_per_minute".to_owned(), "2".to_owned()),
                ("total_in_per_minute".to_owned(), "20".to_owned()),
            ]),
            start_on_load: false,
        };
        let irc_spec = build_control_spec(&irc_server).expect("IRC server limits map");
        assert_eq!(irc_spec.access.connection_rates.client_per_minute, 2);
        assert_eq!(irc_spec.access.connection_rates.total_per_minute, 20);

        let client = ControlDefinition {
            name: "client-rates".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
                ("listen_port".to_owned(), "8123".to_owned()),
                ("client_per_minute".to_owned(), "2".to_owned()),
            ]),
            start_on_load: false,
        };
        assert!(matches!(
            build_control_spec(&client),
            Err(ControlError::ContradictoryOptions { .. })
        ));

        let http_server = ControlDefinition {
            name: "http-post-rates".to_owned(),
            tunnel_type: TunnelType::HttpServer,
            options: BTreeMap::from([
                ("target_host".to_owned(), "127.0.0.1".to_owned()),
                ("target_port".to_owned(), "8080".to_owned()),
                ("post_limit".to_owned(), "300".to_owned()),
                ("post_limit_time".to_owned(), "600".to_owned()),
                ("per_client_period".to_owned(), "6".to_owned()),
                ("total_period".to_owned(), "20".to_owned()),
                ("total_ban_time".to_owned(), "1200".to_owned()),
            ]),
            start_on_load: false,
        };
        let http_spec = build_control_spec(&http_server).expect("HTTP POST limits map");
        assert_eq!(http_spec.http_policy.post_limits.window_seconds, 300);
        assert_eq!(http_spec.http_policy.post_limits.client_ban_seconds, 600);
        assert_eq!(http_spec.http_policy.post_limits.client_max, 6);
        assert_eq!(http_spec.http_policy.post_limits.total_max, 20);
        assert_eq!(http_spec.http_policy.post_limits.total_ban_seconds, 1_200);

        let streamr_server = ControlDefinition {
            name: "streamr-rates".to_owned(),
            tunnel_type: TunnelType::StreamrServer,
            options: BTreeMap::from([
                ("local_udp_host".to_owned(), "127.0.0.1".to_owned()),
                ("local_udp_port".to_owned(), "12345".to_owned()),
                ("client_per_minute".to_owned(), "3".to_owned()),
                ("total_in_per_minute".to_owned(), "24".to_owned()),
            ]),
            start_on_load: false,
        };
        let streamr_spec =
            build_control_spec(&streamr_server).expect("Streamr subscription limits map");
        assert_eq!(streamr_spec.access.connection_rates.client_per_minute, 3);
        assert_eq!(streamr_spec.access.connection_rates.total_per_minute, 24);
    }

    #[test]
    fn proposal_filter_file_is_confined_bounded_and_owns_server_access() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("control store opens");
        let filter_root = store.filter_root();
        let peer = format!("{}.b32.i2p", "a".repeat(52));
        let filter = filter_root.join("allowed.txt");
        std::fs::write(&filter, format!("# comment\n{peer}\n")).expect("filter writes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&filter, std::fs::Permissions::from_mode(0o600))
                .expect("private filter permissions");
        }
        let options = BTreeMap::from([
            ("target_host".to_owned(), "127.0.0.1".to_owned()),
            ("target_port".to_owned(), "8080".to_owned()),
            ("access_option".to_owned(), "allow".to_owned()),
            ("filter_file_path".to_owned(), "allowed.txt".to_owned()),
        ]);
        let definition = normalize_definition_with_filter_root(
            "filter-server",
            TunnelType::Server,
            &options,
            false,
            Some(&filter_root),
            &NoOutboundSecrets,
        )
        .expect("bounded filter definition normalizes");
        let spec = build_control_spec_with_filter_root(&definition, Some(&filter_root))
            .expect("filter file feeds access policy");
        let expected = ServerAccessPolicy::parse(&[peer.as_str()], &[]).expect("peer parses");
        assert_eq!(spec.access.allow, expected.allow);
        assert!(spec.access.deny.is_empty());

        let traversal = BTreeMap::from([
            ("target_host".to_owned(), "127.0.0.1".to_owned()),
            ("target_port".to_owned(), "8080".to_owned()),
            ("filter_file_path".to_owned(), "../escape.txt".to_owned()),
        ]);
        assert!(
            normalize_definition_with_filter_root(
                "filter-traversal",
                TunnelType::Server,
                &traversal,
                false,
                Some(&filter_root),
                &NoOutboundSecrets,
            )
            .is_err()
        );

        let oversized = filter_root.join("large.txt");
        std::fs::write(&oversized, vec![b'a'; MAX_FILTER_FILE_BYTES + 1])
            .expect("oversized filter writes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&oversized, std::fs::Permissions::from_mode(0o600))
                .expect("private filter permissions");
        }
        assert!(read_confined_filter_file(&filter_root, "large.txt").is_err());

        #[cfg(unix)]
        {
            let outside = directory.path().join("outside.txt");
            std::fs::write(&outside, peer).expect("outside file writes");
            std::os::unix::fs::symlink(&outside, filter_root.join("linked.txt"))
                .expect("symlink creates");
            assert!(read_confined_filter_file(&filter_root, "linked.txt").is_err());
        }
    }

    #[test]
    fn plan323_connect_delay_has_bounded_generic_client_owner() {
        let definition = ControlDefinition {
            name: "delayed-client".to_owned(),
            tunnel_type: TunnelType::Client,
            options: BTreeMap::from([
                ("connect_delay".to_owned(), "true".to_owned()),
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
            ]),
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("generic client spec");
        assert_eq!(
            spec.timeouts.streaming_connect_delay_ms,
            Some(i2pr_service_tunnels::DEFAULT_STREAMING_CONNECT_DELAY_MS)
        );

        let delayed_open = ControlDefinition {
            name: "delayed-open-client".to_owned(),
            tunnel_type: TunnelType::Socks,
            options: BTreeMap::from([
                ("delay_open".to_owned(), "true".to_owned()),
                (
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                ),
            ]),
            start_on_load: false,
        };
        let delayed_spec = build_control_spec(&delayed_open).expect("SOCKS DelayOpen spec");
        assert!(delayed_spec.timeouts.delay_open);

        let invalid = ControlDefinition {
            name: "delayed-http".to_owned(),
            tunnel_type: TunnelType::HttpClient,
            options: BTreeMap::from([("connect_delay".to_owned(), "true".to_owned())]),
            start_on_load: false,
        };
        assert!(build_control_spec(&invalid).is_err());

        let invalid_server = ControlDefinition {
            name: "delayed-server".to_owned(),
            tunnel_type: TunnelType::Server,
            options: BTreeMap::from([("delay_open".to_owned(), "true".to_owned())]),
            start_on_load: false,
        };
        assert!(matches!(
            normalize_definition(
                &invalid_server.name,
                invalid_server.tunnel_type,
                &invalid_server.options,
                invalid_server.start_on_load,
            ),
            Err(ControlError::ContradictoryOptions { .. })
        ));
    }

    #[test]
    fn plan291_streamr_control_spec_builds() {
        use i2pr_service_tunnels::ServiceTunnelKind;
        // Server: explicit loopback UDP source plus bounded
        // overrides; no TCP listener or target.
        let mut options = BTreeMap::new();
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5001".to_owned());
        options.insert("streamr_expiry".to_owned(), "90000".to_owned());
        options.insert("streamr_max_subscribers".to_owned(), "5".to_owned());
        let definition = ControlDefinition {
            name: "pub".to_owned(),
            tunnel_type: TunnelType::StreamrServer,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("server spec builds");
        assert_eq!(spec.kind, ServiceTunnelKind::StreamrServer);
        let udp = spec
            .streamr_options
            .expect("options")
            .local_udp
            .expect("udp");
        assert_eq!(udp.port(), 5001);
        assert!(spec.listener.is_none());
        assert!(spec.target.is_none());
        // Client: producer destination plus loopback UDP target.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5000".to_owned());
        options.insert("target_i2p_port".to_owned(), "7".to_owned());
        let definition = ControlDefinition {
            name: "sub".to_owned(),
            tunnel_type: TunnelType::StreamrClient,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("client spec builds");
        assert_eq!(spec.kind, ServiceTunnelKind::StreamrClient);
        assert!(spec.destination.is_some());
        assert_eq!(spec.streamr_options.expect("options").target_i2p_port, 7);
        // Missing UDP endpoint fails before allocation.
        let definition = ControlDefinition {
            name: "bad".to_owned(),
            tunnel_type: TunnelType::StreamrServer,
            options: BTreeMap::new(),
            start_on_load: false,
        };
        assert!(build_control_spec(&definition).is_err());
        // Cross-kind keys fail: server-only key on the client,
        // TCP target keys on either Streamr half.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5000".to_owned());
        options.insert("streamr_expiry".to_owned(), "90000".to_owned());
        let definition = ControlDefinition {
            name: "xclient".to_owned(),
            tunnel_type: TunnelType::StreamrClient,
            options,
            start_on_load: false,
        };
        assert!(build_control_spec(&definition).is_err());
    }

    #[test]
    fn plan292_streamr_sink_redirect_builds() {
        use i2pr_service_tunnels::ServiceTunnelKind;
        // Subscriber redirect: media goes to remote_udp_host
        // paired with the local media port.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5000".to_owned());
        options.insert("remote_udp_host".to_owned(), "127.0.0.2".to_owned());
        let definition = ControlDefinition {
            name: "sinksub".to_owned(),
            tunnel_type: TunnelType::StreamrClient,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("sink spec builds");
        assert_eq!(spec.kind, ServiceTunnelKind::StreamrClient);
        let sink = spec
            .streamr_options
            .expect("options")
            .remote_sink
            .expect("sink");
        assert_eq!(sink.ip().to_string(), "127.0.0.2");
        assert_eq!(sink.port(), 5000);
        // Without the redirect the sink stays empty.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5000".to_owned());
        let definition = ControlDefinition {
            name: "plainsub".to_owned(),
            tunnel_type: TunnelType::StreamrClient,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("plain spec builds");
        assert!(spec.streamr_options.expect("options").remote_sink.is_none());
        // Non-loopback redirect fails naming the key, never the value.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5000".to_owned());
        options.insert("remote_udp_host".to_owned(), "192.0.2.1".to_owned());
        let definition = ControlDefinition {
            name: "badsink".to_owned(),
            tunnel_type: TunnelType::StreamrClient,
            options,
            start_on_load: false,
        };
        let error = build_control_spec(&definition).expect_err("non-loopback fails");
        assert!(
            matches!(error, ControlError::InvalidOption { ref option, .. } if option == "remote_udp_host"),
            "unexpected error: {error:?}"
        );
        // Publisher and non-Streamr kinds reject the redirect.
        for tunnel_type in [TunnelType::StreamrServer, TunnelType::Client] {
            let mut options = BTreeMap::new();
            if tunnel_type == TunnelType::Client {
                options.insert(
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                );
                options.insert("listen_port".to_owned(), "0".to_owned());
            } else {
                options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
                options.insert("local_udp_port".to_owned(), "5001".to_owned());
            }
            options.insert("remote_udp_host".to_owned(), "127.0.0.2".to_owned());
            let definition = ControlDefinition {
                name: "xsink".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let error = build_control_spec(&definition).expect_err("cross-kind fails");
            assert!(
                matches!(error, ControlError::ContradictoryOptions { .. }),
                "unexpected error: {error:?}"
            );
        }
    }

    #[test]
    fn plan292_server_dial_and_presentation_options_build() {
        // unique_local_address lands on the masked server kinds.
        for tunnel_type in [
            TunnelType::Server,
            TunnelType::HttpServer,
            TunnelType::HttpBidirServer,
        ] {
            let mut options = server_options("127.0.0.1:9090");
            options.insert("unique_local_address".to_owned(), "true".to_owned());
            let definition = ControlDefinition {
                name: "uniq".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let spec = build_control_spec(&definition).expect("unique-local builds");
            assert!(spec.unique_local_address);
        }
        // Off-mask kinds reject it.
        for tunnel_type in [TunnelType::Client, TunnelType::IrcServer, TunnelType::Socks] {
            let mut options = if tunnel_type == TunnelType::IrcServer {
                server_options("127.0.0.1:9090")
            } else {
                client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0)
            };
            options.insert("unique_local_address".to_owned(), "true".to_owned());
            let definition = ControlDefinition {
                name: "uniqx".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let error = build_control_spec(&definition).expect_err("off-mask fails");
            assert!(
                matches!(error, ControlError::ContradictoryOptions { .. }),
                "unexpected error: {error:?}"
            );
        }
        // Presentation gates land on the HTTP server kinds only,
        // with both gates open by default.
        for tunnel_type in [TunnelType::HttpServer, TunnelType::HttpBidirServer] {
            let options = server_options("127.0.0.1:9090");
            let definition = ControlDefinition {
                name: "pres".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let spec = build_control_spec(&definition).expect("presentation builds");
            assert_eq!(
                spec.http_policy,
                i2pr_service_tunnels::HttpServerPolicy::default()
            );
            let mut options = server_options("127.0.0.1:9090");
            options.insert("spoofed_host".to_owned(), "site.example.i2p".to_owned());
            let definition = ControlDefinition {
                name: "spoofed".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let spec = build_control_spec(&definition).expect("SpoofedHost has an owner");
            assert_eq!(
                spec.http_policy.spoofed_host.as_deref(),
                Some("site.example.i2p")
            );
            let mut options = server_options("127.0.0.1:9090");
            options.insert("block_referers".to_owned(), "false".to_owned());
            let definition = ControlDefinition {
                name: "referer-policy".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let spec = build_control_spec(&definition).expect("BlockReferers has an owner");
            assert!(!spec.http_policy.block_referers);
            let mut options = server_options("127.0.0.1:9090");
            options.insert("address_helper".to_owned(), "false".to_owned());
            options.insert("jump_list".to_owned(), "false".to_owned());
            let definition = ControlDefinition {
                name: "presclosed".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let spec = build_control_spec(&definition).expect("closed gates build");
            assert!(!spec.http_policy.address_helper);
            assert!(!spec.http_policy.jump_list);
        }
        // Raw TCP servers and clients reject the presentation keys
        // (no HTTP layer to gate).
        for tunnel_type in [TunnelType::Server, TunnelType::Client] {
            let mut options = if tunnel_type == TunnelType::Server {
                server_options("127.0.0.1:9090")
            } else {
                client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0)
            };
            options.insert("address_helper".to_owned(), "false".to_owned());
            let definition = ControlDefinition {
                name: "presx".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let error = build_control_spec(&definition).expect_err("off-kind fails");
            assert!(
                matches!(error, ControlError::ContradictoryOptions { .. }),
                "unexpected error: {error:?}"
            );
        }
        for tunnel_type in [TunnelType::Server, TunnelType::Client] {
            let mut options = if tunnel_type == TunnelType::Server {
                server_options("127.0.0.1:9090")
            } else {
                client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0)
            };
            options.insert("block_referers".to_owned(), "false".to_owned());
            let definition = ControlDefinition {
                name: "referer-off-kind".to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            assert!(matches!(
                build_control_spec(&definition),
                Err(ControlError::ContradictoryOptions { .. })
            ));
        }
        let mut options = server_options("127.0.0.1:9090");
        options.insert("spoofed_host".to_owned(), "bad host".to_owned());
        let malformed = ControlDefinition {
            name: "badspoof".to_owned(),
            tunnel_type: TunnelType::HttpServer,
            options,
            start_on_load: false,
        };
        assert!(matches!(
            build_control_spec(&malformed),
            Err(ControlError::InvalidOption { ref option, .. }) if option == "spoofed_host"
        ));
    }

    #[test]
    fn plan323_server_user_agent_filter_has_a_bounded_http_owner() {
        let mut options = server_options("127.0.0.1:9090");
        options.insert("block_user_agents".to_owned(), "true".to_owned());
        options.insert("user_agents".to_owned(), "crawler,none".to_owned());
        options.insert("block_access_in_proxies".to_owned(), "true".to_owned());
        let definition = ControlDefinition {
            name: "ua-filter".to_owned(),
            tunnel_type: TunnelType::HttpServer,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("HTTP server owner builds");
        assert!(spec.http_policy.block_user_agents);
        assert_eq!(spec.http_policy.user_agents, ["crawler", "none"]);
        assert!(spec.http_policy.block_access_in_proxies);

        let mut invalid = server_options("127.0.0.1:9090");
        invalid.insert("block_user_agents".to_owned(), "true".to_owned());
        assert!(matches!(
            build_control_spec(&ControlDefinition {
                name: "ua-empty".to_owned(),
                tunnel_type: TunnelType::HttpServer,
                options: invalid,
                start_on_load: false,
            }),
            Err(ControlError::InvalidOption { ref option, .. }) if option == "block_user_agents"
        ));
        let mut incompatible = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        incompatible.insert("user_agents".to_owned(), "crawler".to_owned());
        let error = build_control_spec(&ControlDefinition {
            name: "ua-client".to_owned(),
            tunnel_type: TunnelType::Client,
            options: incompatible,
            start_on_load: false,
        })
        .expect_err("UserAgents is rejected on client tunnels");
        assert!(
            matches!(error, ControlError::ContradictoryOptions { .. }),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn plan289_option_subset_has_real_effect() {
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 8180);
        options.insert("max_streams".to_owned(), "24".to_owned());
        options.insert("start_on_load".to_owned(), "false".to_owned());
        let definition = ControlDefinition {
            name: "alpha".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("spec builds");
        assert_eq!(spec.max_connections, 24);
        assert_eq!(spec.listener.expect("listener").socket().port(), 8180);
        assert!(spec.destination.is_some());
        // Defaults: ephemeral port, loopback bind, 16 connections.
        let definition = ControlDefinition {
            name: "beta".to_owned(),
            tunnel_type: TunnelType::Client,
            options: client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0)
                .into_iter()
                .filter(|(key, _)| *key != "listen_port" && *key != "listen_host")
                .collect(),
            start_on_load: true,
        };
        let spec = build_control_spec(&definition).expect("defaults build");
        assert_eq!(spec.listener.expect("listener").socket().port(), 0);
        assert_eq!(spec.max_connections, DEFAULT_CONTROL_MAX_CONNECTIONS);
        // Server kinds map target_host/target_port with real effect.
        let options = server_options("127.0.0.1:9090");
        let definition = ControlDefinition {
            name: "srv".to_owned(),
            tunnel_type: TunnelType::Server,
            options,
            start_on_load: true,
        };
        let spec = build_control_spec(&definition).expect("server builds");
        assert!(spec.target.is_some());
        assert!(spec.listener.is_none());
    }

    #[test]
    fn plan289_unsupported_options_rejected_before_storage() {
        // Every secret-classified key is rejected on a client kind, which
        // publishes no LeaseSet at all. The rejection is a mask rejection
        // (the key is out of scope for the kind), so it still names the key.
        for secret in i2pr_i2pcontrol::SECRET_OPTIONS {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(secret.to_owned(), "hunter2".to_owned());
            let error = normalize_definition("alpha", TunnelType::Client, &options, true)
                .expect_err("secret rejected");
            if secret == "proxy_password" {
                // Plan 292: the paired-halves check fires before
                // the secret gate for a lone password half; both
                // reject before storage and both name the key.
                assert_eq!(
                    error,
                    ControlError::InvalidOption {
                        option: "proxy_password".to_owned(),
                        reason: "proxy_username and proxy_password are both required",
                    }
                );
            } else if secret == "leaseset_password" {
                // Plan 351 Gate 2 — an intentional change to this row, not a
                // regression.
                //
                // `leaseset_password` on a client is no longer refused by the
                // *mask* (the key is out of scope for the kind); a client may
                // need it as a **consumer** to derive the blinded storage key
                // for a `.b33` target. It is still refused here — the
                // `target_destination` in these options is an ordinary `.b32`,
                // so the cross-field pairing rule fires instead of the mask.
                //
                // Both paths reject before storage and neither accepts the
                // configuration, which is the property Plan 289 asserted. What
                // changed is only which rule speaks, and the new rule names the
                // actual objection ("this secret is useless on this client")
                // rather than "this key is out of scope", which was never true.
                assert_eq!(
                    error,
                    ControlError::ContradictoryOptions {
                        name: "alpha".to_owned(),
                        reason: "leaseset_password on a client tunnel requires target_destination to be an encrypted-service address (.b33.i2p)",
                    }
                );
            } else if secret == "outproxy_password" {
                // Plan 342 — an intentional change to this row, not a
                // regression.
                //
                // `outproxy_password` is no longer refused by the *mask*; it
                // is one of the seven outproxy block keys and is admitted to
                // a definition. It is still refused here, because a lone
                // password is a partial block: the all-or-none rule fires
                // first and names the six keys that are missing.
                //
                // That is a strictly better failure than the mask's. The mask
                // said "this key is out of scope for a client", which was
                // never true — a client is exactly where an outproxy
                // credential belongs. The block rule says what is actually
                // wrong: the operator supplied one seventh of a configuration
                // whose other six parts are required for it to mean anything.
                match error {
                    ControlError::OutproxyBlockRejected(reason) => {
                        assert!(reason.contains("outproxy_password"), "{reason}");
                        assert!(reason.contains("missing proxy_list"), "{reason}");
                    }
                    other => panic!("unexpected error for {secret}: {other:?}"),
                }
            } else {
                assert_eq!(error, ControlError::UnsupportedOption(secret.to_owned()));
            }
        }
        // Plan 334: on a *publishing* kind the two real LeaseSet secrets are
        // accepted by the supported-subset gate, and are then refused by the
        // cross-field rule instead — a supplied secret for a mode that does
        // not consume it is an error, never a silent no-op. The retired
        // duplicate slot keeps its own refusal.
        for secret in ["leaseset_password", "leaseset_client_auth"] {
            let mut options = server_options("127.0.0.1:9090");
            options.insert(secret.to_owned(), "hunter2".to_owned());
            let error = normalize_definition("alpha", TunnelType::Server, &options, true)
                .expect_err("unused leaseset secret rejected");
            // Either the mode rule (nothing consumes it) or the shape rule
            // (the value is not a valid client list) may fire first; both
            // reject before storage, and neither may echo the value.
            assert!(
                matches!(&error, ControlError::LeaseSetSecurityRejected(message)
                    if message.contains("not used by") || message.contains("Key must be")),
                "unexpected error for {secret}: {error:?}"
            );
            assert!(
                !format!("{error:?}").contains("hunter2"),
                "secret value leaked for {secret}"
            );
        }
        let mut options = server_options("127.0.0.1:9090");
        options.insert("leaseset_blinding_secret".to_owned(), "hunter2".to_owned());
        let error = normalize_definition("alpha", TunnelType::Server, &options, true)
            .expect_err("retired duplicate slot rejected");
        assert!(
            matches!(&error, ControlError::UnsupportedOption(message)
                if message.contains("leaseset_blinding_secret")
                    && message.contains("OptionalLookup")),
            "unexpected error: {error:?}"
        );
        assert!(
            !format!("{error:?}").contains("hunter2"),
            "secret value leaked for the retired duplicate slot"
        );
        // Known-but-not-yet-supported options fail explicitly too,
        // naming the owning plan for blocked and corrective cells.
        for key in ["outproxy", "description", "leaseset_type"] {
            if i2pr_i2pcontrol::tunnel_options::find_option(key).is_err() {
                continue;
            }
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), "x".to_owned());
            let error = normalize_definition("alpha", TunnelType::Client, &options, true)
                .expect_err("unsupported rejected");
            assert!(
                matches!(&error, ControlError::UnsupportedOption(message) if message.contains(key)),
                "unexpected error: {error:?}"
            );
        }
        // Contradictions and malformed values fail with static reasons.
        let mut options = BTreeMap::new();
        options.insert(
            "target_destination".to_owned(),
            format!("{}.b32.i2p", "a".repeat(52)),
        );
        let error = normalize_definition("srv", TunnelType::Server, &options, true)
            .expect_err("destination on server fails");
        assert!(matches!(error, ControlError::ContradictoryOptions { .. }));
        let mut options = BTreeMap::new();
        options.insert("target_host".to_owned(), "unix:/tmp/sock".to_owned());
        options.insert("target_port".to_owned(), "9".to_owned());
        let error = normalize_definition("srv", TunnelType::Server, &options, true)
            .expect_err("unix target fails");
        assert!(matches!(error, ControlError::InvalidOption { .. }));
        let options = client_options("", 0);
        let error = normalize_definition("alpha", TunnelType::Client, &options, true)
            .expect_err("empty destination fails");
        assert!(matches!(error, ControlError::InvalidOption { .. }));
        // Debug never prints values (keys are schema, values are data).
        let definition = ControlDefinition {
            name: "alpha".to_owned(),
            tunnel_type: TunnelType::Client,
            options: client_options("example.i2p", 8180),
            start_on_load: true,
        };
        let debug = format!("{:?}", definition);
        assert!(!debug.contains("example.i2p"), "values redacted: {debug}");
        assert!(!debug.contains("8180"), "values redacted: {debug}");
        assert!(debug.contains("alpha"), "name present: {debug}");
    }

    #[test]
    fn plan323_description_is_bounded_owned_and_survives_generation_reload() {
        let description = "Managed HTTP client";
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("description".to_owned(), description.to_owned());
        let definition = normalize_definition("described", TunnelType::Client, &options, false)
            .expect("description uses the control metadata owner");
        assert_eq!(
            definition.options.get("description"),
            Some(&description.to_owned())
        );
        let spec = build_control_spec(&definition).expect("description does not alter routing");
        assert!(spec.destination.is_some());

        let directory = tempfile::tempdir().expect("tempdir");
        let store = ControlStore::open(directory.path()).expect("store opens");
        let definitions = BTreeMap::from([(definition.name.clone(), definition)]);
        let generation = store.stage(&definitions).expect("generation stages");
        store.publish(generation).expect("generation publishes");
        let recovered = store.load().expect("generation recovers");
        assert_eq!(
            recovered.definitions[0].options.get("description"),
            Some(&description.to_owned())
        );
    }

    #[test]
    fn plan323_http_request_filters_have_a_real_http_client_owner() {
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("allow_user_agent".to_owned(), "true".to_owned());
        options.insert("allow_referer".to_owned(), "true".to_owned());
        options.insert("allow_accept".to_owned(), "false".to_owned());
        options.insert("allow_internal_ssl".to_owned(), "false".to_owned());
        let definition =
            normalize_definition("http-filters", TunnelType::HttpClient, &options, false)
                .expect("Proposal filter fields are supported");
        let spec = build_control_spec(&definition).expect("HTTP client owner consumes filters");
        let privacy = &spec.http_options.as_ref().expect("HTTP options").privacy;
        assert_eq!(
            privacy.user_agent,
            i2pr_service_tunnels::UserAgentPolicy::Keep
        );
        assert!(!privacy.strip_referer);
        assert!(!privacy.allow_accept);
        assert!(
            !spec
                .http_options
                .as_ref()
                .expect("HTTP options")
                .allow_internal_ssl
        );

        let mut incompatible = client_options(&format!("{}.b32.i2p", "b".repeat(52)), 0);
        incompatible.insert("allow_referer".to_owned(), "true".to_owned());
        assert!(matches!(
            normalize_definition("socks-filter", TunnelType::Socks, &incompatible, false)
                .and_then(|definition| build_control_spec(&definition)),
            Err(ControlError::ContradictoryOptions { .. })
        ));

        let mut incompatible = client_options(&format!("{}.b32.i2p", "c".repeat(52)), 0);
        incompatible.insert("allow_internal_ssl".to_owned(), "true".to_owned());
        assert!(matches!(
            normalize_definition(
                "socks-internal-ssl",
                TunnelType::Socks,
                &incompatible,
                false
            )
            .and_then(|definition| build_control_spec(&definition)),
            Err(ControlError::ContradictoryOptions { .. })
        ));
    }

    #[test]
    fn plan292_shaping_options_have_real_effect() {
        use i2pr_service_tunnels::ServiceTunnelKind;
        // Symmetric quantity/length drive the pool projection; the
        // rest reproduces the balanced defaults.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("tunnel_quantity".to_owned(), "4".to_owned());
        options.insert("tunnel_length".to_owned(), "3".to_owned());
        let definition = ControlDefinition {
            name: "shaped".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("shaped spec builds");
        assert_eq!(spec.shaping.inbound_quantity, 4);
        assert_eq!(spec.shaping.outbound_quantity, 4);
        assert_eq!(spec.shaping.length_hops, 3);
        let projected = crate::service_tunnels::ServiceTunnelManager::destination_config_for(&spec);
        assert_eq!(projected.inbound_target(), 4);
        assert_eq!(projected.outbound_target(), 4);
        assert_eq!(projected.length_hops(), 3);
        // Per-direction quantities override independently.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("inbound_quantity".to_owned(), "5".to_owned());
        options.insert("outbound_quantity".to_owned(), "1".to_owned());
        let definition = ControlDefinition {
            name: "split".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("split spec builds");
        assert_eq!(spec.shaping.inbound_quantity, 5);
        assert_eq!(spec.shaping.outbound_quantity, 1);
        assert_eq!(spec.shaping.length_hops, 2);
        // Agreeing per-direction lengths are accepted ...
        let mut options = server_options("127.0.0.1:9090");
        options.insert("inbound_length".to_owned(), "1".to_owned());
        options.insert("outbound_length".to_owned(), "1".to_owned());
        let definition = ControlDefinition {
            name: "srvlen".to_owned(),
            tunnel_type: TunnelType::Server,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("server lengths build");
        assert_eq!(spec.shaping.length_hops, 1);
        // ... while differing lengths fail instead of silently
        // dropping one side.
        let mut options = server_options("127.0.0.1:9090");
        options.insert("inbound_length".to_owned(), "1".to_owned());
        options.insert("outbound_length".to_owned(), "3".to_owned());
        let error = normalize_definition("srvbad", TunnelType::Server, &options, false)
            .expect_err("differing lengths fail");
        assert!(matches!(error, ControlError::ContradictoryOptions { .. }));
        // Bounds: quantity 1..=6, length 1..=3 (zero-hop rejected).
        for (key, value) in [
            ("tunnel_quantity", "0"),
            ("tunnel_quantity", "7"),
            ("inbound_quantity", "0"),
            ("tunnel_length", "0"),
            ("tunnel_length", "4"),
            ("outbound_length", "0"),
        ] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), value.to_owned());
            let error = normalize_definition("bad", TunnelType::Client, &options, false)
                .expect_err("shaping bound enforced");
            assert!(
                matches!(error, ControlError::InvalidOption { .. }),
                "unexpected error for {key}={value}: {error:?}"
            );
        }
        // Residuals name their limitation (in-mask kinds only): 293
        // deep primitives name the determination and its Plan 295
        // carriage. Plan 296 residuals now apply (tested below), so
        // malformed values fail as invalid options, never as
        // unsupported.
        for (key, value) in [("tunnel_backup_quantity", "x"), ("tunnel_variance", "x")] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), value.to_owned());
            let error = normalize_definition("bad", TunnelType::Client, &options, false)
                .expect_err("malformed residual rejected");
            assert!(
                matches!(&error, ControlError::InvalidOption { .. }),
                "unexpected error for {key}: {error:?}"
            );
        }
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("sig_type".to_owned(), "EDDSA_SHA512_ED25519".to_owned());
        options.insert("enc_type".to_owned(), "4".to_owned());
        let definition = normalize_definition("typed-crypto", TunnelType::Client, &options, false)
            .expect("the supported identity policy is typed and persisted");
        let spec = build_control_spec(&definition).expect("the supported identity policy maps");
        assert!(matches!(
            spec.policy,
            DestinationPolicy::WithCrypto { crypto, .. }
                if crypto == i2pr_service_tunnels::DestinationCryptoPolicy::default()
        ));
        for (key, value) in [
            ("sig_type", "Ed25519"),
            ("sig_type", "DSA-SHA1"),
            ("enc_type", "0"),
            ("enc_type", "5"),
        ] {
            let mut invalid = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            invalid.insert(key.to_owned(), value.to_owned());
            assert!(matches!(
                normalize_definition("bad-crypto", TunnelType::Client, &invalid, false),
                Err(ControlError::InvalidOption { .. })
            ));
        }
        // Plan 342: the outproxy provider residual is no longer a residual.
        // Plan 293 recorded this cell as an explicit incompatibility
        // ("outproxy provider semantics have no I2P-routed provider") and
        // Plan 342 supplied the provider, so the row that asserted the
        // determination no longer describes the code.
        //
        // What replaced it is a stronger property, not a weaker one: the
        // key alone is refused by the all-or-none block rule, which names
        // the six fields it is missing. The Plan 293 determination itself is
        // left in `specs/protocols/14-tunnel-deep-option-determinations.md`
        // as history.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("use_outproxy_plugin".to_owned(), "true".to_owned());
        let error = normalize_definition("httpout", TunnelType::HttpClient, &options, false)
            .expect_err("a lone outproxy field is refused");
        match error {
            ControlError::OutproxyBlockRejected(reason) => {
                assert!(reason.contains("all-or-none"), "{reason}");
                assert!(reason.contains("missing proxy_list"), "{reason}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
        // Plan 297: `use_ssl` applies on server kinds through the
        // daemon TLS policy owner.
        for value in ["true", "false"] {
            let mut options = server_options("127.0.0.1:9090");
            options.insert("use_ssl".to_owned(), value.to_owned());
            let definition = normalize_definition("srvtls", TunnelType::Server, &options, false)
                .expect("use_ssl admitted");
            let spec = build_control_spec(&definition).expect("use_ssl builds");
            assert_eq!(spec.use_ssl, value == "true");
        }
        // Non-server kinds stay rejected; malformed values fail.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("use_ssl".to_owned(), "true".to_owned());
        let error = normalize_definition("clitls", TunnelType::Client, &options, false)
            .and_then(|definition| build_control_spec(&definition))
            .expect_err("client use_ssl rejected");
        assert!(
            matches!(&error, ControlError::ContradictoryOptions { .. }),
            "unexpected error: {error:?}"
        );
        let mut options = server_options("127.0.0.1:9090");
        options.insert("use_ssl".to_owned(), "maybe".to_owned());
        let error = normalize_definition("badtls", TunnelType::Server, &options, false)
            .and_then(|definition| build_control_spec(&definition))
            .expect_err("malformed use_ssl rejected");
        assert!(
            matches!(&error, ControlError::InvalidOption { option, .. } if option == "use_ssl"),
            "unexpected error: {error:?}"
        );
        // Plan 297: the TLS state object appears only on `use_ssl`
        // definitions.
        let manager = crate::service_tunnels::ServiceTunnelManager::new(
            crate::service_tunnels::ServiceTunnelManagerConfig {
                data_dir: std::env::temp_dir(),
                aggregate_connection_ceiling: 1,
                per_service_connection_ceiling: 1,
                specs: std::sync::Arc::new(ServiceTunnelSet::new()),
                aliases: std::sync::Arc::new(i2pr_service_tunnels::StaticAliasTable::new()),
            },
        )
        .expect("manager builds");
        let plain_definition = normalize_definition(
            "srvplain",
            TunnelType::Server,
            &server_options("127.0.0.1:9090"),
            false,
        )
        .expect("plain admitted");
        assert!(
            control_tls_state(&manager, "srvplain", &plain_definition).is_none(),
            "plain tunnels carry no TLS state"
        );
        // Plan 297: a `use_ssl` definition reports its verification
        // mode, identity expiry, and handshake counters — and never
        // key material or certificate bytes.
        let pinned = {
            let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()])
                .expect("fixture cert");
            certified.cert.pem().into_bytes()
        };
        let policy = crate::service_tunnels_tls::ServiceTlsPolicy::from_parts(
            None,
            Some(pinned.clone()),
            None,
        )
        .expect("pin policy builds");
        manager.set_service_tls_policy(std::sync::Arc::new(policy));
        let mut options = server_options("127.0.0.1:9090");
        options.insert("use_ssl".to_owned(), "true".to_owned());
        let definition = normalize_definition("srvtls", TunnelType::Server, &options, false)
            .expect("use_ssl admitted");
        let state = control_tls_state(&manager, "srvtls", &definition)
            .expect("use_ssl tunnels carry TLS state");
        assert_eq!(state.get("enabled"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(
            state.get("verify"),
            Some(&serde_json::Value::String("pin".to_owned()))
        );
        assert_eq!(
            state.get("handshakes_ok"),
            Some(&serde_json::Value::Number(0.into()))
        );
        assert_eq!(
            state.get("handshakes_failed"),
            Some(&serde_json::Value::Number(0.into()))
        );
        let rendered = serde_json::to_string(&state).expect("state renders");
        assert!(
            !rendered.contains("-----BEGIN"),
            "certificate bytes never appear in control output"
        );
        // Plan 296: backup quantity and length variance apply to
        // shaping with Proposal bounds; the pool-maximum sum check
        // rejects contradictory combinations.
        for (key, value) in [("tunnel_backup_quantity", "2"), ("tunnel_variance", "-1")] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), value.to_owned());
            let definition = normalize_definition("shp296", TunnelType::Client, &options, false)
                .expect("296 shaping admitted");
            let spec = build_control_spec(&definition).expect("296 shaping builds");
            if key == "tunnel_backup_quantity" {
                assert_eq!(spec.shaping.backup_quantity, 2);
            } else {
                assert_eq!(spec.shaping.length_variance, -1);
            }
        }
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("tunnel_backup_quantity".to_owned(), "2".to_owned());
        options.insert("inbound_quantity".to_owned(), "4".to_owned());
        options.insert("outbound_quantity".to_owned(), "4".to_owned());
        let definition = normalize_definition("shp296b", TunnelType::Client, &options, false)
            .expect("296 shaping admitted");
        let spec = build_control_spec(&definition).expect("296 shaping builds");
        let projected = crate::service_tunnels::ServiceTunnelManager::destination_config_for(&spec);
        assert_eq!(projected.backup_quantity(), 2);
        assert_eq!(projected.effective_inbound_target(), 6);
        assert_eq!(projected.effective_outbound_target(), 6);
        assert!(projected.pool_config().is_ok());
        // Bounds and contradictions name the key.
        for (key, value) in [
            ("tunnel_backup_quantity", "4"),
            ("tunnel_backup_quantity", "-1"),
            ("tunnel_variance", "3"),
            ("tunnel_variance", "-3"),
            ("tunnel_variance", "x"),
        ] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), value.to_owned());
            let error = normalize_definition("bad", TunnelType::Client, &options, false)
                .and_then(|definition| build_control_spec(&definition))
                .expect_err("296 bound rejected");
            assert!(
                matches!(&error, ControlError::InvalidOption { option, .. } if option == key),
                "unexpected error for {key}={value}: {error:?}"
            );
        }
        // Quantity plus backup past the pool directional maximum is
        // contradictory, never clamped.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("tunnel_quantity".to_owned(), "6".to_owned());
        options.insert("tunnel_backup_quantity".to_owned(), "3".to_owned());
        let error = normalize_definition("bad", TunnelType::Client, &options, false)
            .and_then(|definition| build_control_spec(&definition))
            .expect_err("oversum rejected");
        assert!(
            matches!(&error, ControlError::ContradictoryOptions { .. }),
            "unexpected error: {error:?}"
        );
        // Plan 296: reply bundling applies to every kind through
        // the destination delivery flag.
        for tunnel_type in [TunnelType::Client, TunnelType::Server] {
            let mut options = if tunnel_type == TunnelType::Server {
                server_options("127.0.0.1:9090")
            } else {
                client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0)
            };
            options.insert("reply_bundling".to_owned(), "true".to_owned());
            let definition = normalize_definition("bnd296", tunnel_type, &options, false)
                .expect("bundling admitted");
            let spec = build_control_spec(&definition).expect("bundling builds");
            assert!(spec.reply_bundling);
            assert!(
                crate::service_tunnels::ServiceTunnelManager::destination_config_for(&spec)
                    .reply_bundling()
            );
        }
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("reply_bundling".to_owned(), "maybe".to_owned());
        let error = normalize_definition("bad", TunnelType::Client, &options, false)
            .and_then(|definition| build_control_spec(&definition))
            .expect_err("malformed bundling rejected");
        assert!(
            matches!(&error, ControlError::InvalidOption { option, .. } if option == "reply_bundling"),
            "unexpected error: {error:?}"
        );
        // Plan 296: multihoming parses on server kinds but needs two
        // configured targets; the control surface carries a singular
        // target, so the two-target minimum rejects it here while
        // the dial-selection owner serves multi-target specs.
        let mut options = server_options("127.0.0.1:9090");
        options.insert("multihoming".to_owned(), "true".to_owned());
        let error = normalize_definition("mhsrv", TunnelType::Server, &options, false)
            .and_then(|definition| build_control_spec(&definition))
            .expect_err("single-target multihoming rejected");
        assert!(
            matches!(&error, ControlError::ContradictoryOptions { reason, .. } if reason.contains("two configured targets")),
            "unexpected error: {error:?}"
        );
        // Client kinds never dial a server target.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("multihoming".to_owned(), "true".to_owned());
        let error = normalize_definition("mhcli", TunnelType::Client, &options, false)
            .and_then(|definition| build_control_spec(&definition))
            .expect_err("client multihoming rejected");
        assert!(
            matches!(&error, ControlError::ContradictoryOptions { .. }),
            "unexpected error: {error:?}"
        );
        // Plan 296: the service-tunnels pool-maximum mirror equals
        // the destination directional ceilings it shadows.
        assert_eq!(
            u16::from(MAX_EFFECTIVE_DIRECTION_TUNNELS),
            i2pr_client::MAX_DESTINATION_INBOUND
        );
        assert_eq!(
            u16::from(MAX_EFFECTIVE_DIRECTION_TUNNELS),
            i2pr_client::MAX_DESTINATION_OUTBOUND
        );
        // Shaping applies to Streamr kinds too (uniform pool sizing).
        let mut options = BTreeMap::new();
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5001".to_owned());
        options.insert("tunnel_quantity".to_owned(), "3".to_owned());
        let definition = ControlDefinition {
            name: "strm".to_owned(),
            tunnel_type: TunnelType::StreamrServer,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("streamr shaping builds");
        assert_eq!(spec.kind, ServiceTunnelKind::StreamrServer);
        assert_eq!(spec.shaping.inbound_quantity, 3);
    }

    #[test]
    fn plan293_deep_primitives_rejected_with_named_limitation() {
        fn base_options(tunnel_type: TunnelType) -> BTreeMap<String, String> {
            match tunnel_type {
                TunnelType::Server
                | TunnelType::IrcServer
                | TunnelType::HttpServer
                | TunnelType::HttpBidirServer => server_options("127.0.0.1:9090"),
                TunnelType::StreamrClient | TunnelType::StreamrServer => {
                    let mut options = BTreeMap::new();
                    options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
                    options.insert("local_udp_port".to_owned(), "5001".to_owned());
                    if tunnel_type == TunnelType::StreamrClient {
                        options.insert(
                            "target_destination".to_owned(),
                            format!("{}.b32.i2p", "a".repeat(52)),
                        );
                    }
                    options
                }
                _ => client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0),
            }
        }
        // The current Ed25519 policy is accepted for every destination
        // owner; unsupported legacy and alternate algorithms fail closed.
        for tunnel_type in [
            TunnelType::Client,
            TunnelType::Server,
            TunnelType::HttpClient,
            TunnelType::Socks,
            TunnelType::IrcClient,
            TunnelType::IrcServer,
            TunnelType::ConnectClient,
            TunnelType::SocksIrc,
            TunnelType::HttpServer,
            TunnelType::HttpBidirServer,
            TunnelType::StreamrClient,
            TunnelType::StreamrServer,
        ] {
            for value in ["EDDSA_SHA512_ED25519", "7"] {
                let mut options = base_options(tunnel_type);
                options.insert("sig_type".to_owned(), value.to_owned());
                let normalized = normalize_definition("sig-ok", tunnel_type, &options, false);
                assert!(
                    normalized.is_ok(),
                    "unexpected supported SigType failure for {tunnel_type:?}: {normalized:?}"
                );
            }
            for value in ["Ed25519", "DSA-SHA1"] {
                let mut options = base_options(tunnel_type);
                options.insert("sig_type".to_owned(), value.to_owned());
                assert!(matches!(
                    normalize_definition("sigbad", tunnel_type, &options, false),
                    Err(ControlError::InvalidOption { .. })
                ));
            }
            let mut options = base_options(tunnel_type);
            options.insert("enc_type".to_owned(), "4".to_owned());
            let normalized = normalize_definition("enc-ok", tunnel_type, &options, false);
            assert!(
                normalized.is_ok(),
                "unexpected supported EncType failure for {tunnel_type:?}: {normalized:?}"
            );
            for value in ["0", "5"] {
                let mut options = base_options(tunnel_type);
                options.insert("enc_type".to_owned(), value.to_owned());
                assert!(matches!(
                    normalize_definition("encbad", tunnel_type, &options, false),
                    Err(ControlError::InvalidOption { .. })
                ));
            }
        }
        // Plan 334 supersedes the Plan 293 LeaseSet block. On every
        // publishing kind, all nine applied modes now normalize with their own
        // companions; the tenth is recognized and refused by name; and the
        // retired duplicate secret slot keeps its own refusal. Every failure
        // still names the key and the mode, and no secret value reaches the
        // message, the mirror, or the store.
        for tunnel_type in [
            TunnelType::Server,
            TunnelType::HttpServer,
            TunnelType::HttpBidirServer,
            TunnelType::StreamrServer,
        ] {
            for (mode, lookup, clients) in [
                ("disable", None, false),
                ("blinded", None, false),
                ("blinded with lookup password", Some("hunter2"), false),
                ("encrypted (psk)", None, true),
                (
                    "encrypted with lookup password (psk)",
                    Some("hunter2"),
                    true,
                ),
                ("encrypted with per-user key (psk)", None, true),
                (
                    "encrypted with lookup password and per-user key (psk)",
                    Some("hunter2"),
                    true,
                ),
                ("encrypted with per-user key (dh)", None, true),
                (
                    "encrypted with lookup password and per-user key (dh)",
                    Some("hunter2"),
                    true,
                ),
            ] {
                let mut options = base_options(tunnel_type);
                options.insert("encrypt_lease_set".to_owned(), mode.to_owned());
                if let Some(secret) = lookup {
                    options.insert("leaseset_password".to_owned(), secret.to_owned());
                }
                if clients {
                    options.insert(
                        "leaseset_client_auth".to_owned(),
                        format!("alpha:{}", "ab".repeat(32)),
                    );
                }
                let definition = normalize_definition("lsok", tunnel_type, &options, false)
                    .unwrap_or_else(|error| {
                        panic!("{mode} must normalize for {tunnel_type:?}: {error:?}")
                    });
                // The stored options carry the mode verbatim and the secret in
                // its bounded encoding, and the derived posture agrees.
                assert_eq!(
                    definition
                        .options
                        .get("encrypt_lease_set")
                        .map(String::as_str),
                    Some(mode)
                );
                let plan = definition
                    .lease_set_security()
                    .expect("normalize_definition already validated this block");
                assert_eq!(plan.spelling(), Some(mode), "{tunnel_type:?}");
                assert_eq!(plan.lookup_secret_supplied(), lookup.is_some());
                assert_eq!(plan.client_count(), usize::from(clients));
            }
            // The legacy AES mode is refused, and named as deprecated.
            let mut options = base_options(tunnel_type);
            options.insert("encrypt_lease_set".to_owned(), "encrypted (aes)".to_owned());
            let error = normalize_definition("lsaes", tunnel_type, &options, false)
                .expect_err("legacy aes refused");
            assert!(
                matches!(&error, ControlError::LeaseSetSecurityRejected(message)
                    if message.contains("deprecated") && message.contains("encrypted LeaseSet2")),
                "unexpected error for {tunnel_type:?}: {error:?}"
            );
            // A mode name is an identifier: no boolean or normalized spelling
            // is accepted, and each rejection names the field.
            for value in ["true", "false", "Disable", "blinded  ", "encrypted(psk)"] {
                let mut options = base_options(tunnel_type);
                options.insert("encrypt_lease_set".to_owned(), value.to_owned());
                let error = normalize_definition("lsbad", tunnel_type, &options, false)
                    .expect_err("bad mode rejected");
                assert!(
                    matches!(&error, ControlError::LeaseSetSecurityRejected(message)
                        if message.contains("ten Proposal 170 values")),
                    "unexpected error for {tunnel_type:?} {value:?}: {error:?}"
                );
            }
            // The retired duplicate secret slot stays refused by name, and a
            // malformed client list is refused by the shape rule. Both reject
            // before storage and neither may echo the supplied value.
            for (key, value) in [
                ("leaseset_blinding_secret", "hunter2"),
                ("leaseset_client_auth", "hunter2"),
            ] {
                let mut options = base_options(tunnel_type);
                options.insert(key.to_owned(), value.to_owned());
                let error = normalize_definition("lsbad", tunnel_type, &options, false)
                    .expect_err("leaseset secret rejected");
                let acceptable = match key {
                    "leaseset_blinding_secret" => matches!(
                        &error,
                        ControlError::UnsupportedOption(message)
                            if message.contains("OptionalLookup")
                    ),
                    _ => matches!(&error, ControlError::LeaseSetSecurityRejected(_)),
                };
                assert!(
                    acceptable,
                    "unexpected error for {tunnel_type:?} {key}: {error:?}"
                );
                assert!(
                    !format!("{error:?}").contains(value),
                    "secret value leaked for {tunnel_type:?} {key}"
                );
            }
        }
        // Plan 342: the outproxy provider limitation Plan 293 recorded for
        // the proxy kinds is resolved, so these rows no longer assert it.
        //
        // A lone `use_outproxy_plugin` — whatever its value, including a
        // plausible-looking plugin URL, which i2pr would never load anyway —
        // is refused by the all-or-none block rule. Two things are being
        // asserted here that were not before: the key is in the mask, and
        // the refusal names the missing fields instead of a limitation that
        // no longer describes the code.
        for tunnel_type in [TunnelType::HttpClient, TunnelType::ConnectClient] {
            for value in ["true", "false", "http://outproxy.i2p", "", "maybe"] {
                let mut options = base_options(tunnel_type);
                options.insert("use_outproxy_plugin".to_owned(), value.to_owned());
                let error = normalize_definition("opbad", tunnel_type, &options, false)
                    .expect_err("outproxy rejected");
                match error {
                    ControlError::OutproxyBlockRejected(reason) => {
                        assert!(reason.contains("use_outproxy_plugin"), "{reason}");
                        assert!(reason.contains("missing proxy_list"), "{reason}");
                    }
                    other => panic!("unexpected error for {tunnel_type:?}: {other:?}"),
                }
            }
        }
        // Out-of-mask pairs still reject before allocation; the message
        // names the key without a limitation (no disposition applies).
        let mut options = base_options(TunnelType::Client);
        options.insert("encrypt_lease_set".to_owned(), "true".to_owned());
        let error = normalize_definition("lsmask", TunnelType::Client, &options, false)
            .expect_err("out-of-mask rejected");
        assert_eq!(
            error,
            ControlError::UnsupportedOption("encrypt_lease_set".to_owned())
        );
    }

    #[test]
    fn plan292_profile_selects_streaming_windows() {
        // profile=interactive and interactive=true both select the
        // constrained windows; bulk (or absent) keeps balanced.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("profile".to_owned(), "interactive".to_owned());
        let definition = ControlDefinition {
            name: "prof".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("profile builds");
        assert!(spec.streaming_interactive);
        let selected = crate::service_tunnels::ServiceTunnelManager::streaming_config_for(&spec);
        assert_eq!(selected.max_send_window_packets, 16);
        assert_eq!(selected.max_recv_window_packets, 16);
        assert_eq!(selected.delayed_ack_ms, 100);
        let mut options = server_options("127.0.0.1:9090");
        options.insert("interactive".to_owned(), "true".to_owned());
        let definition = ControlDefinition {
            name: "srvprof".to_owned(),
            tunnel_type: TunnelType::Server,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("interactive builds");
        assert!(spec.streaming_interactive);
        // Explicit bulk and absent keys stay balanced.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("profile".to_owned(), "bulk".to_owned());
        options.insert("interactive".to_owned(), "false".to_owned());
        let definition = ControlDefinition {
            name: "bulk".to_owned(),
            tunnel_type: TunnelType::Client,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("bulk builds");
        assert!(!spec.streaming_interactive);
        let selected = crate::service_tunnels::ServiceTunnelManager::streaming_config_for(&spec);
        assert_eq!(
            selected,
            i2pr_client::streaming::config::StreamingConfig::balanced()
        );
        // Unknown profile values fail instead of storing inertly.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("profile".to_owned(), "turbo".to_owned());
        let error = normalize_definition("badprof", TunnelType::Client, &options, false)
            .expect_err("unknown profile fails");
        assert!(matches!(error, ControlError::InvalidOption { .. }));
        // Streamr kinds have no streaming stack: both keys fail.
        for key in ["profile", "interactive"] {
            let mut options = BTreeMap::new();
            options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
            options.insert("local_udp_port".to_owned(), "5001".to_owned());
            options.insert(
                key.to_owned(),
                if key == "profile" {
                    "interactive"
                } else {
                    "true"
                }
                .to_owned(),
            );
            let error =
                normalize_definition("strmprof", TunnelType::StreamrServer, &options, false)
                    .expect_err("streamr profile fails");
            assert!(
                matches!(error, ControlError::ContradictoryOptions { .. }),
                "unexpected error for {key}: {error:?}"
            );
        }
    }

    #[test]
    fn plan292_idle_policy_parses_and_pairs() {
        // Full policy on a client kind: explicit deadline plus one
        // action flag each.
        for flag in ["close_on_idle", "new_dest_on_idle", "reduce_on_idle"] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert("idle_timeout".to_owned(), "5000".to_owned());
            options.insert(flag.to_owned(), "true".to_owned());
            if flag == "new_dest_on_idle" {
                options.insert("close_on_idle".to_owned(), "true".to_owned());
            }
            let definition = ControlDefinition {
                name: format!("idle-{flag}"),
                tunnel_type: TunnelType::Client,
                options,
                start_on_load: false,
            };
            let spec = build_control_spec(&definition).expect("idle spec builds");
            assert_eq!(spec.idle.timeout_ms, Some(5_000));
            assert!(spec.idle.enabled());
        }
        // Flags without a deadline take the documented default.
        let mut options = server_options("127.0.0.1:9090");
        options.insert("close_on_idle".to_owned(), "true".to_owned());
        let definition = ControlDefinition {
            name: "srv_idle".to_owned(),
            tunnel_type: TunnelType::Server,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("default deadline builds");
        assert_eq!(
            spec.idle.timeout_ms,
            Some(i2pr_service_tunnels::DEFAULT_IDLE_TIMEOUT_MS)
        );
        // A deadline without flags is inert and fails.
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert("idle_timeout".to_owned(), "5000".to_owned());
        let error = normalize_definition("lone", TunnelType::Client, &options, false)
            .expect_err("lone deadline fails");
        assert!(
            matches!(
                error,
                ControlError::InvalidRequest(_) | ControlError::ContradictoryOptions { .. }
            ),
            "unexpected error: {error:?}"
        );
        // Out-of-range and non-numeric deadlines fail at the boundary.
        for value in ["0", "999", "86400001", "five"] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert("idle_timeout".to_owned(), value.to_owned());
            options.insert("close_on_idle".to_owned(), "true".to_owned());
            let error = normalize_definition("badidle", TunnelType::Client, &options, false)
                .expect_err("bad deadline fails");
            assert!(
                matches!(error, ControlError::InvalidOption { .. }),
                "unexpected error for {value}: {error:?}"
            );
        }
        // Idle applies to Streamr kinds too (subscriber/media
        // activity, not streaming connections).
        let mut options = BTreeMap::new();
        options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
        options.insert("local_udp_port".to_owned(), "5001".to_owned());
        options.insert("idle_timeout".to_owned(), "5000".to_owned());
        options.insert("close_on_idle".to_owned(), "true".to_owned());
        let definition = ControlDefinition {
            name: "strmidle".to_owned(),
            tunnel_type: TunnelType::StreamrServer,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("streamr idle builds");
        assert!(spec.idle.enabled());
    }

    #[test]
    fn plan292_proxy_auth_parses_and_attaches() {
        // Each proxy family accepts credentials and attaches them
        // to its own options struct (verifiers, never plaintext).
        let cases = [
            (TunnelType::HttpClient, "http-proxy"),
            (TunnelType::Socks, "socks-proxy"),
            (TunnelType::ConnectClient, "connect-proxy"),
            (TunnelType::SocksIrc, "socksirc-proxy"),
        ];
        for (tunnel_type, name) in cases {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert("proxy_username".to_owned(), "operator".to_owned());
            options.insert("proxy_password".to_owned(), "s3cret!".to_owned());
            options.insert("proxy_auth".to_owned(), "true".to_owned());
            let definition = ControlDefinition {
                name: name.to_owned(),
                tunnel_type,
                options,
                start_on_load: false,
            };
            let spec = build_control_spec(&definition).expect("auth spec builds");
            let attached = match tunnel_type {
                TunnelType::HttpClient => {
                    assert!(spec.socks5_options.is_none());
                    spec.http_options.expect("http options").proxy_auth
                }
                TunnelType::ConnectClient => {
                    spec.connect_options.expect("connect options").proxy_auth
                }
                _ => spec.socks5_options.expect("socks options").proxy_auth,
            };
            let credentials = attached.expect("credentials attached");
            assert!(credentials.verify("operator", "s3cret!"));
            assert!(!credentials.verify("operator", "wrong"));
            // The definition mirror holds the marked verifier, never
            // the password (what reaches the store).
            let mut raw = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            raw.insert("proxy_username".to_owned(), "operator".to_owned());
            raw.insert("proxy_password".to_owned(), "s3cret!".to_owned());
            let stored = normalize_definition(&format!("{name}-stored"), tunnel_type, &raw, false)
                .expect("normalize stores");
            let persisted = stored.options.get("proxy_password").expect("persisted");
            assert!(
                persisted.starts_with(i2pr_service_tunnels::PROXY_VERIFIER_MARKER),
                "marked verifier: {persisted}"
            );
            assert!(!persisted.contains("s3cret"));
        }
        for (proxy_auth, with_credentials) in [("true", false), ("false", true)] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert("proxy_auth".to_owned(), proxy_auth.to_owned());
            if with_credentials {
                options.insert("proxy_username".to_owned(), "operator".to_owned());
                options.insert("proxy_password".to_owned(), "s3cret!".to_owned());
            }
            let definition = ControlDefinition {
                name: "proxy-auth-conflict".to_owned(),
                tunnel_type: TunnelType::Socks,
                options,
                start_on_load: false,
            };
            assert!(matches!(
                build_control_spec(&definition),
                Err(ControlError::ContradictoryOptions { .. })
            ));
        }
        // Either half alone fails (both required together).
        for (key, value) in [
            ("proxy_username", "operator"),
            ("proxy_password", "s3cret!"),
        ] {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            options.insert(key.to_owned(), value.to_owned());
            let error = normalize_definition("half", TunnelType::Socks, &options, false)
                .expect_err("half credentials fail");
            assert!(
                matches!(error, ControlError::InvalidOption { .. }),
                "unexpected error: {error:?}"
            );
        }
        // Proxy keys on non-proxy kinds fail (matrix not-applicable).
        for tunnel_type in [
            TunnelType::Client,
            TunnelType::Server,
            TunnelType::StreamrClient,
        ] {
            let mut options = if tunnel_type == TunnelType::Server {
                server_options("127.0.0.1:9090")
            } else if tunnel_type == TunnelType::StreamrClient {
                let mut options = BTreeMap::new();
                options.insert(
                    "target_destination".to_owned(),
                    format!("{}.b32.i2p", "a".repeat(52)),
                );
                options.insert("local_udp_host".to_owned(), "127.0.0.1".to_owned());
                options.insert("local_udp_port".to_owned(), "5001".to_owned());
                options
            } else {
                client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0)
            };
            options.insert("proxy_username".to_owned(), "operator".to_owned());
            options.insert("proxy_password".to_owned(), "s3cret!".to_owned());
            let error = normalize_definition("cross", tunnel_type, &options, false)
                .expect_err("cross-kind auth fails");
            assert!(
                matches!(error, ControlError::ContradictoryOptions { .. }),
                "unexpected error: {error:?}"
            );
        }
    }

    #[test]
    fn plan292_username_rotation_requires_password() {
        // Changing the proxy username without re-supplying the
        // password would silently bind the new name to the old
        // verifier, so edit rejects it explicitly.
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let mut options = client_options(&destination, 0);
        options.insert("proxy_username".to_owned(), "operator".to_owned());
        options.insert("proxy_password".to_owned(), "s3cret!".to_owned());
        block_on(control.create(&create_request("authrot", TunnelType::Socks, options)))
            .expect("create with auth");
        // Username-only rotation fails.
        let mut rotation = BTreeMap::new();
        rotation.insert("proxy_username".to_owned(), "renamed".to_owned());
        let error = block_on(control.edit(&TunnelManagerRequest {
            action: TunnelAction::Edit,
            all: false,
            name: Some("authrot".to_owned()),
            tunnel_type: None,
            new_name: None,
            options: rotation,
        }))
        .expect_err("rotation without password fails");
        assert!(
            matches!(error, ControlError::InvalidOption { .. }),
            "unexpected error: {error:?}"
        );
        // Rotation with a fresh password succeeds.
        let mut rotation = BTreeMap::new();
        rotation.insert("proxy_username".to_owned(), "renamed".to_owned());
        rotation.insert("proxy_password".to_owned(), "n3w-secret".to_owned());
        block_on(control.edit(&TunnelManagerRequest {
            action: TunnelAction::Edit,
            all: false,
            name: Some("authrot".to_owned()),
            tunnel_type: None,
            new_name: None,
            options: rotation,
        }))
        .expect("rotation with password succeeds");
    }

    #[test]
    fn plan292_access_lists_parse_and_gate() {
        let hash_a = format!("{}.b32.i2p", "a".repeat(52));
        // NOTE: the distinct label char leads: the final char of a
        // canonical 32-byte encoding must carry zero padding bits.
        let hash_b = format!("b{}.b32.i2p", "a".repeat(51));
        // access_list unions white_list into allow; black_list denies.
        let mut options = server_options("127.0.0.1:9090");
        options.insert("access_list".to_owned(), hash_a.clone());
        options.insert("white_list".to_owned(), hash_b.clone());
        options.insert("black_list".to_owned(), hash_a.clone());
        let definition = ControlDefinition {
            name: "acl".to_owned(),
            tunnel_type: TunnelType::Server,
            options,
            start_on_load: false,
        };
        let spec = build_control_spec(&definition).expect("access spec builds");
        assert_eq!(spec.access.allow.len(), 2);
        assert_eq!(spec.access.deny.len(), 1);
        let mut proposal_deny = server_options("127.0.0.1:9090");
        proposal_deny.insert("access_option".to_owned(), "deny".to_owned());
        proposal_deny.insert("access_list".to_owned(), hash_b.clone());
        let denied =
            normalize_definition("proposal-deny", TunnelType::Server, &proposal_deny, false)
                .expect("Proposal deny list builds");
        let denied = build_control_spec(&denied).expect("Proposal deny policy builds");
        assert!(denied.access.allow.is_empty());
        let expected_deny =
            ServerAccessPolicy::parse(&[], &[&hash_b]).expect("expected access hash parses");
        assert_eq!(denied.access.deny, expected_deny.deny);
        let mut missing_list = server_options("127.0.0.1:9090");
        missing_list.insert("access_option".to_owned(), "allow".to_owned());
        assert!(matches!(
            normalize_definition(
                "proposal-missing-list",
                TunnelType::Server,
                &missing_list,
                false
            ),
            Err(ControlError::ContradictoryOptions { .. })
        ));
        // Non-hash entries fail naming the key, never the value.
        for (key, value) in [
            ("access_list", "example.i2p"),
            ("white_list", "127.0.0.1"),
            ("black_list", "not a hash"),
        ] {
            let mut options = server_options("127.0.0.1:9090");
            options.insert(key.to_owned(), value.to_owned());
            let error = normalize_definition("aclbad", TunnelType::Server, &options, false)
                .expect_err("bad entry fails");
            assert!(
                matches!(error, ControlError::InvalidOption { ref option, .. } if option == key),
                "unexpected error: {error:?}"
            );
        }
        // Access keys on non-server kinds fail (matrix
        // not-applicable, including ircserver).
        for tunnel_type in [TunnelType::Client, TunnelType::IrcServer, TunnelType::Socks] {
            // NOTE: option shapes are kept valid for the kind so the
            // failure proves the access gate (not a missing field):
            // server-shaped for ircserver, client-shaped otherwise.
            let mut options = if tunnel_type == TunnelType::IrcServer {
                server_options("127.0.0.1:9090")
            } else {
                client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0)
            };
            options.insert("black_list".to_owned(), hash_a.clone());
            let error = normalize_definition("aclx", tunnel_type, &options, false)
                .expect_err("cross-kind access fails");
            assert!(
                matches!(error, ControlError::ContradictoryOptions { .. }),
                "unexpected error: {error:?}"
            );
        }
    }

    #[test]
    fn plan292_idle_sweep_closes_quiet_tunnels() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        // Quiet tunnel with close-on-idle plus a plain sibling that
        // must never fire (distinct binds; two ephemeral port-0
        // listeners would collide at the boundary).
        let ports = distinct_ports(2);
        let mut options = client_options(&destination, ports[0]);
        options.insert("idle_timeout".to_owned(), "1000".to_owned());
        options.insert("close_on_idle".to_owned(), "true".to_owned());
        block_on(control.create(&create_request("idleclose", TunnelType::Client, options)))
            .expect("create runs");
        let plain = client_options(&destination, ports[1]);
        block_on(control.create(&create_request("plainsibling", TunnelType::Client, plain)))
            .expect("sibling runs");
        assert!(control.is_running("idleclose"));
        let base = crate::service_tunnels::service_streaming_now_ms();
        // Before the deadline nothing fires.
        let applied = block_on(control.idle_sweep_once(base + 500));
        assert!(applied.is_empty(), "no early fire: {applied:?}");
        // Past the deadline the quiet tunnel stops; the sibling is
        // untouched.
        let applied = block_on(control.idle_sweep_once(base + 60_000));
        assert_eq!(applied.len(), 1, "one decision: {applied:?}");
        assert_eq!(applied[0].spec_id, "idleclose");
        assert_eq!(applied[0].action, "close");
        assert!(!control.is_running("idleclose"));
        assert!(control.is_running("plainsibling"));
        // A second sweep finds nothing (no runtime, no decision).
        let applied = block_on(control.idle_sweep_once(base + 120_000));
        assert!(applied.is_empty(), "no repeat: {applied:?}");
    }

    #[test]
    fn plan292_idle_sweep_reduces_to_floor_then_stops() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let mut options = client_options(&destination, 0);
        options.insert("tunnel_quantity".to_owned(), "4".to_owned());
        options.insert("idle_timeout".to_owned(), "1000".to_owned());
        options.insert("reduce_on_idle".to_owned(), "true".to_owned());
        block_on(control.create(&create_request("idlereduce", TunnelType::Client, options)))
            .expect("create runs");
        let generation_before = control.store_generation();
        let base = crate::service_tunnels::service_streaming_now_ms();
        let now = base + 60_000;
        // First sweep halves 4/4 to 2/2 and restarts.
        let applied = block_on(control.idle_sweep_once(now));
        assert_eq!(applied.len(), 1, "reduce fires: {applied:?}");
        assert_eq!(applied[0].action, "reduce-pools");
        assert!(control.store_generation() > generation_before);
        assert!(control.is_running("idlereduce"));
        let halved = control
            .manager
            .effective_shaping_for("idlereduce")
            .expect("runtime lives");
        assert_eq!(halved.inbound_quantity, 2);
        assert_eq!(halved.outbound_quantity, 2);
        // Second sweep halves to the 1/1 floor and restarts again.
        let applied = block_on(control.idle_sweep_once(now));
        assert_eq!(applied.len(), 1, "reduce fires again: {applied:?}");
        let floored = control
            .manager
            .effective_shaping_for("idlereduce")
            .expect("runtime lives");
        assert_eq!(floored.inbound_quantity, 1);
        assert_eq!(floored.outbound_quantity, 1);
        // At the floor the sweep decides nothing (no restart loop).
        let applied = block_on(control.idle_sweep_once(now));
        assert!(applied.is_empty(), "floor holds: {applied:?}");
        assert!(control.is_running("idlereduce"));
    }

    #[test]
    fn plan323_idle_controls_use_independent_time_and_reduce_count() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let mut options = client_options(&destination, 0);
        options.insert("tunnel_quantity".to_owned(), "4".to_owned());
        options.insert("close_on_idle".to_owned(), "true".to_owned());
        options.insert("close_time".to_owned(), "30".to_owned());
        options.insert("reduce_on_idle".to_owned(), "true".to_owned());
        options.insert("reduce_time".to_owned(), "20".to_owned());
        options.insert("reduce_count".to_owned(), "1".to_owned());
        block_on(control.create(&create_request("proposalidle", TunnelType::Client, options)))
            .expect("create runs");

        let base = crate::service_tunnels::service_streaming_now_ms();
        let reduced = block_on(control.idle_sweep_once(base + 1_200_000));
        assert_eq!(reduced.len(), 1, "reduce deadline: {reduced:?}");
        assert_eq!(reduced[0].action, "reduce-pools");
        let shaping = control
            .manager
            .effective_shaping_for("proposalidle")
            .unwrap();
        assert_eq!(shaping.inbound_quantity, 3);
        assert_eq!(shaping.outbound_quantity, 3);
        assert!(control.is_running("proposalidle"));

        // The pure service-tunnels test covers the independent close deadline;
        // this runtime assertion pins the Proposal-specific decrement effect.
    }

    #[test]
    fn plan292_idle_sweep_rebuilds_pools_in_place() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let mut options = client_options(&destination, 0);
        options.insert("idle_timeout".to_owned(), "1000".to_owned());
        options.insert("new_dest_on_idle".to_owned(), "true".to_owned());
        block_on(control.create(&create_request("idle-rebuild", TunnelType::Client, options)))
            .expect("create runs");
        let generation_before = control.store_generation();
        let base = crate::service_tunnels::service_streaming_now_ms();
        let applied = block_on(control.idle_sweep_once(base + 60_000));
        assert_eq!(applied.len(), 1, "rebuild fires: {applied:?}");
        assert_eq!(applied[0].action, "rebuild-pools");
        assert!(control.store_generation() > generation_before);
        assert!(control.is_running("idle-rebuild"));
    }

    #[test]
    fn plan323_new_dest_one_rotates_identity_on_idle_resume() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let mut options = client_options(&destination, 0);
        options.insert("new_dest".to_owned(), "1".to_owned());
        options.insert("close_on_idle".to_owned(), "true".to_owned());
        block_on(control.create(&create_request("idlerebuild", TunnelType::Client, options)))
            .expect("create runs");
        let destination_before = control
            .manager
            .service_destination_id("idlerebuild")
            .expect("initial ephemeral Destination");
        let generation_before = control.store_generation();
        let base = crate::service_tunnels::service_streaming_now_ms();
        assert!(block_on(control.idle_sweep_once(base + 599_000)).is_empty());
        let applied = block_on(control.idle_sweep_once(base + 660_000));
        assert_eq!(applied.len(), 1, "idle rotation fires: {applied:?}");
        assert_eq!(applied[0].action, "new-destination");
        assert!(control.store_generation() > generation_before);
        assert!(control.is_running("idlerebuild"));
        let destination_after = control
            .manager
            .service_destination_id("idlerebuild")
            .expect("resumed ephemeral Destination");
        assert_ne!(destination_after, destination_before);
    }

    #[test]
    fn plan289_create_get_start_stop_delete_lifecycle() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        // Create without running intent: committed but stopped.
        let response = block_on(
            control.create(&create_request("alpha", TunnelType::Client, {
                let mut options = client_options(&destination, 0);
                options.insert("start_on_load".to_owned(), "false".to_owned());
                options
            })),
        )
        .expect("create");
        assert_eq!(response["running"], serde_json::json!(false));
        let generation = response["generation"].as_u64().expect("generation");
        assert!(generation >= 1);
        // Get separates persisted intent from runtime state.
        let response = control.get(Some("alpha")).expect("get");
        assert_eq!(response["provenance"], serde_json::json!("control"));
        assert_eq!(response["type"], serde_json::json!("client"));
        assert_eq!(response["start_on_load"], serde_json::json!(false));
        assert_eq!(response["running"], serde_json::json!(false));
        assert_eq!(response["status"], serde_json::json!("stopped"));
        // Start, stop, delete move running intent with new generations.
        let response =
            block_on(control.start(&named_request(TunnelAction::Start, "alpha"))).expect("start");
        assert_eq!(response["running"], serde_json::json!(true));
        assert!(response["generation"].as_u64().expect("gen") > generation);
        let response = control.get(Some("alpha")).expect("get running");
        assert_eq!(response["status"], serde_json::json!("running"));
        assert_eq!(response["running"], serde_json::json!(true));
        let response =
            block_on(control.stop(&named_request(TunnelAction::Stop, "alpha"))).expect("stop");
        assert_eq!(response["running"], serde_json::json!(false));
        let response = control.get(Some("alpha")).expect("get stopped");
        assert_eq!(response["status"], serde_json::json!("stopped"));
        block_on(control.delete(&named_request(TunnelAction::Delete, "alpha"))).expect("delete");
        assert_eq!(
            control.get(Some("alpha")),
            Err(ControlError::UnknownTunnel("alpha".to_owned()))
        );
    }

    #[test]
    fn plan323_all_lifecycle_uses_sorted_control_owned_snapshot() {
        let directory = tempfile::tempdir().expect("tempdir");
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let ports = distinct_ports(3);
        let startup = ServiceTunnelSet {
            tunnels: vec![
                build_control_spec(&ControlDefinition {
                    name: "startup".to_owned(),
                    tunnel_type: TunnelType::Client,
                    options: client_options(&destination, ports[2]),
                    start_on_load: false,
                })
                .expect("startup spec"),
            ],
        };
        let store = ControlStore::open(directory.path()).expect("store opens");
        let control = TunnelControlState::new(
            store,
            startup,
            test_manager(directory.path()),
            test_outbound_secrets(),
        );
        for (name, port) in [("zeta", ports[0]), ("alpha", ports[1])] {
            block_on(control.create(&create_request(
                name,
                TunnelType::Client,
                client_options(&destination, port),
            )))
            .expect("create control-owned tunnel");
        }
        let all_start = TunnelManagerRequest {
            action: TunnelAction::Start,
            all: true,
            name: None,
            tunnel_type: None,
            new_name: None,
            options: BTreeMap::new(),
        };
        let started = block_on(control.dispatch(&all_start)).expect("bulk start");
        let results = started["results"].as_array().expect("result array");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["name"], "alpha");
        assert_eq!(results[1]["name"], "zeta");
        assert!(results.iter().all(|result| result["status"] == "success"));
        assert!(control.is_running("alpha"));
        assert!(control.is_running("zeta"));
        let startup_status = control.get(Some("startup")).expect("startup get");
        assert_eq!(startup_status["provenance"], "startup");
        assert_eq!(startup_status["enabled"], false);

        let all_restart = TunnelManagerRequest {
            action: TunnelAction::Restart,
            ..all_start
        };
        let restarted = block_on(control.dispatch(&all_restart)).expect("bulk restart");
        assert_eq!(restarted["results"].as_array().unwrap().len(), 2);
        assert!(control.is_running("alpha"));
        assert!(control.is_running("zeta"));

        let all_stop = TunnelManagerRequest {
            action: TunnelAction::Stop,
            all: true,
            name: None,
            tunnel_type: None,
            new_name: None,
            options: BTreeMap::new(),
        };
        let stopped = block_on(control.dispatch(&all_stop)).expect("bulk stop");
        assert_eq!(stopped["results"].as_array().unwrap().len(), 2);
        assert!(!control.is_running("alpha"));
        assert!(!control.is_running("zeta"));
    }

    #[test]
    fn plan289_create_validates_before_side_effects() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        // Malformed destination: no store file, no runtime, empty mirror.
        // (Empty fails; arbitrary bounded strings are valid configured
        // destinations per the M10 reference type.)
        let error = block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options("", 0),
        )))
        .expect_err("invalid fails");
        assert!(matches!(error, ControlError::InvalidOption { .. }));
        assert_eq!(control.store.current_id(), 0, "nothing staged");
        assert_eq!(
            control.get(Some("alpha")),
            Err(ControlError::UnknownTunnel("alpha".to_owned()))
        );
        // Plan 291: streamrclient validates its required fields
        // before any side effect (missing destination and UDP
        // endpoint: no store file, no runtime, empty mirror).
        let error = block_on(control.create(&create_request(
            "stream",
            TunnelType::StreamrClient,
            BTreeMap::new(),
        )))
        .expect_err("invalid fails");
        assert!(matches!(
            error,
            ControlError::InvalidRequest(_) | ControlError::InvalidOption { .. }
        ));
        assert_eq!(control.store.current_id(), 0);
    }

    #[test]
    fn plan289_collision_and_startup_protection() {
        let directory = tempfile::tempdir().expect("tempdir");
        let ports = distinct_ports(2);
        let mut startup = ServiceTunnelSet::new();
        startup.tunnels.push(
            build_control_spec(&ControlDefinition {
                name: "owned".to_owned(),
                tunnel_type: TunnelType::Client,
                options: client_options(&format!("{}.b32.i2p", "a".repeat(52)), ports[0]),
                start_on_load: false,
            })
            .expect("startup spec"),
        );
        let store = ControlStore::open(directory.path()).expect("open");
        let control = TunnelControlState::new(
            store,
            startup,
            test_manager(directory.path()),
            test_outbound_secrets(),
        );
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        // Control name collides with a startup-owned name.
        let error = block_on(control.create(&create_request(
            "owned",
            TunnelType::Client,
            client_options(&destination, 0),
        )))
        .expect_err("collision fails");
        assert_eq!(error, ControlError::NameCollision("owned".to_owned()));
        // Startup-owned definitions reject every mutation.
        for action in [
            TunnelAction::Edit,
            TunnelAction::Delete,
            TunnelAction::Start,
            TunnelAction::Stop,
        ] {
            let mut request = named_request(action, "owned");
            if action == TunnelAction::Edit {
                request
                    .options
                    .insert("listen_port".to_owned(), "1".to_owned());
            }
            let error = block_on(control.dispatch(&request)).expect_err("mutation rejected");
            assert_eq!(
                error,
                ControlError::StartupMutationRejected("owned".to_owned())
            );
        }
        // Duplicate control names collide.
        block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect("first create");
        let error = block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect_err("duplicate fails");
        assert_eq!(error, ControlError::NameCollision("alpha".to_owned()));
        // Startup summaries stay inspectable.
        let response = control.get(Some("owned")).expect("startup get");
        assert_eq!(response["provenance"], serde_json::json!("startup"));
    }

    #[test]
    fn plan289_edit_rename_and_options() {
        let directory = tempfile::tempdir().expect("tempdir");
        let ports = distinct_ports(3);
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, ports[0]),
        )))
        .expect("create");
        // Edit options: max_connections takes real effect on the spec.
        let mut request = named_request(TunnelAction::Edit, "alpha");
        request
            .options
            .insert("max_streams".to_owned(), "32".to_owned());
        block_on(control.dispatch(&request)).expect("edit");
        let response = control.get(Some("alpha")).expect("get");
        assert_eq!(response["options"]["max_streams"], serde_json::json!("32"));
        // Rename moves the definition; the old name is unknown.
        let mut request = named_request(TunnelAction::Edit, "alpha");
        request.new_name = Some("beta".to_owned());
        block_on(control.dispatch(&request)).expect("rename");
        assert_eq!(
            control.get(Some("alpha")),
            Err(ControlError::UnknownTunnel("alpha".to_owned()))
        );
        let response = control.get(Some("beta")).expect("renamed get");
        assert_eq!(response["options"]["max_streams"], serde_json::json!("32"));
        // Rename onto an existing name collides.
        block_on(control.create(&create_request(
            "gamma",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect("second create");
        let mut request = named_request(TunnelAction::Edit, "beta");
        request.new_name = Some("gamma".to_owned());
        let error = block_on(control.dispatch(&request)).expect_err("rename collision");
        assert_eq!(error, ControlError::NameCollision("gamma".to_owned()));
    }

    #[test]
    fn plan289_start_on_load_vs_running_truthfulness() {
        let directory = tempfile::tempdir().expect("tempdir");
        let ports = distinct_ports(2);
        let control = test_control(directory.path());
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        // start_on_load=false: committed but never running.
        let mut options = client_options(&destination, ports[0]);
        options.insert("start_on_load".to_owned(), "false".to_owned());
        block_on(control.create(&create_request("lazy", TunnelType::Client, options)))
            .expect("create");
        // A fresh coordinator over the same store restarts: lazy stays stopped.
        let fresh = test_control(directory.path());
        let (scope, parent) = test_scope();
        let failures = block_on(fresh.startup(&scope, &parent));
        assert!(failures.is_empty(), "startup clean: {failures:?}");
        let response = fresh.get(Some("lazy")).expect("get");
        assert_eq!(response["start_on_load"], serde_json::json!(false));
        assert_eq!(response["running"], serde_json::json!(false));
        assert_eq!(response["status"], serde_json::json!("stopped"));
        // start_on_load=true: running immediately and across restarts.
        block_on(control.create(&create_request(
            "eager",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect("create eager");
        // Release the first coordinator's listeners before the fresh
        // coordinator binds the same sockets (a real restart frees
        // them when the old process exits).
        drop(control);
        let fresh = test_control(directory.path());
        let failures = block_on(fresh.startup(&scope, &parent));
        assert!(failures.is_empty(), "startup clean: {failures:?}");
        let response = fresh.get(Some("eager")).expect("get");
        assert_eq!(response["start_on_load"], serde_json::json!(true));
        assert_eq!(response["running"], serde_json::json!(true));
        assert_eq!(response["status"], serde_json::json!("running"));
        drop(parent);
        drop(scope);
    }

    #[test]
    fn plan289_server_identity_stable_across_restart() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        let target = std::net::TcpListener::bind("127.0.0.1:0").expect("target binds");
        let target_address = target.local_addr().expect("target addr");
        let options = server_options(&target_address.to_string());
        block_on(control.create(&create_request("srv", TunnelType::Server, options)))
            .expect("create");
        let before = control.get(Some("srv")).expect("get");
        let before_b64 = before["destination"]
            .as_str()
            .expect("server b64")
            .to_owned();
        assert!(!before_b64.is_empty());
        // Restart preserves the persistent server identity.
        block_on(control.dispatch(&named_request(TunnelAction::Restart, "srv"))).expect("restart");
        let after = control.get(Some("srv")).expect("get after");
        assert_eq!(after["destination"].as_str().expect("b64"), before_b64);
        assert_eq!(after["status"], serde_json::json!("running"));
        // A fresh coordinator over the same store recovers the same identity.
        let fresh = test_control(directory.path());
        let (scope, parent) = test_scope();
        block_on(fresh.startup(&scope, &parent));
        let recovered = fresh.get(Some("srv")).expect("recovered get");
        assert_eq!(recovered["destination"].as_str().expect("b64"), before_b64);
        drop(parent);
        drop(scope);
        drop(target);
    }

    #[test]
    fn plan289_publish_failure_reconciles_back() {
        let directory = tempfile::tempdir().expect("tempdir");
        // Sabotage publication: a directory at the pointer path makes
        // the atomic rename fail deterministically.
        let store = ControlStore::open(directory.path()).expect("open");
        std::fs::create_dir(store.pointer_path()).expect("pointer dir");
        let control = TunnelControlState::new(
            store,
            ServiceTunnelSet::new(),
            test_manager(directory.path()),
            test_outbound_secrets(),
        );
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let error = block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, 0),
        )))
        .expect_err("publish fails");
        match error {
            ControlError::PublishFailed {
                reconciled_back, ..
            } => {
                assert!(reconciled_back, "runtime reconciled back");
            }
            other => panic!("expected PublishFailed, got {other:?}"),
        }
        // Mirror rolled back: unknown, unpublished, no runtime.
        assert_eq!(
            control.get(Some("alpha")),
            Err(ControlError::UnknownTunnel("alpha".to_owned()))
        );
        assert_eq!(control.store.current_id(), 0);
    }

    #[test]
    fn plan289_concurrent_same_name_serializes_deterministically() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = std::sync::Arc::new(test_control(directory.path()));
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let control = std::sync::Arc::clone(&control);
                let destination = destination.clone();
                std::thread::spawn(move || {
                    block_on(control.create(&create_request(
                        "race",
                        TunnelType::Client,
                        client_options(&destination, 0),
                    )))
                    .is_ok()
                })
            })
            .collect();
        let mut successes = 0;
        for handle in handles {
            if handle.join().expect("join") {
                successes += 1;
            }
        }
        assert_eq!(successes, 1, "exactly one create wins");
        assert!(control.get(Some("race")).is_ok());
    }

    #[test]
    fn plan289_concurrent_different_names_stay_bounded_and_consistent() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = std::sync::Arc::new(test_control(directory.path()));
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let ports = distinct_ports(8);
        let handles: Vec<_> = (0..8)
            .map(|n| {
                let control = std::sync::Arc::clone(&control);
                let destination = destination.clone();
                let port = ports[n as usize];
                std::thread::spawn(move || {
                    block_on(control.create(&create_request(
                        &format!("tunnel-{n}"),
                        TunnelType::Client,
                        client_options(&destination, port),
                    )))
                })
            })
            .collect();
        let mut generations = Vec::new();
        for handle in handles {
            let response = handle.join().expect("join").expect("create succeeds");
            generations.push(response["generation"].as_u64().expect("generation"));
        }
        generations.sort_unstable();
        // One publication per mutation: strictly increasing generations.
        for window in generations.windows(2) {
            assert!(
                window[0] < window[1],
                "generations increase: {generations:?}"
            );
        }
        let inventory = control.get(None).expect("inventory");
        assert_eq!(inventory["control"].as_object().expect("map").len(), 8);
    }

    #[test]
    fn plan289_failed_start_reports_failed_status() {
        let directory = tempfile::tempdir().expect("tempdir");
        let control = test_control(directory.path());
        // Occupy a port: staging the listener bind fails deterministically.
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").expect("occupy");
        let port = occupied.local_addr().expect("addr").port();
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        let error = block_on(control.create(&create_request(
            "blocked",
            TunnelType::Client,
            client_options(&destination, port),
        )))
        .expect_err("bind fails");
        assert!(
            matches!(error, ControlError::Manager(_)),
            "manager error: {error:?}"
        );
        // Failed staging leaves no durable or runtime trace.
        assert_eq!(
            control.get(Some("blocked")),
            Err(ControlError::UnknownTunnel("blocked".to_owned()))
        );
        assert_eq!(control.store.current_id(), 0);
        drop(occupied);
    }

    #[test]
    fn plan289_get_inventory_lists_both_classes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let ports = distinct_ports(2);
        let mut startup = ServiceTunnelSet::new();
        startup.tunnels.push(
            build_control_spec(&ControlDefinition {
                name: "owned".to_owned(),
                tunnel_type: TunnelType::Client,
                options: client_options(&format!("{}.b32.i2p", "a".repeat(52)), ports[0]),
                start_on_load: false,
            })
            .expect("startup spec"),
        );
        let store = ControlStore::open(directory.path()).expect("open");
        let control = TunnelControlState::new(
            store,
            startup,
            test_manager(directory.path()),
            test_outbound_secrets(),
        );
        let destination = format!("{}.b32.i2p", "a".repeat(52));
        block_on(control.create(&create_request(
            "alpha",
            TunnelType::Client,
            client_options(&destination, ports[1]),
        )))
        .expect("create");
        let inventory = control.get(None).expect("inventory");
        assert_eq!(
            inventory["startup"]["owned"]["provenance"],
            serde_json::json!("startup")
        );
        assert_eq!(
            inventory["control"]["alpha"]["provenance"],
            serde_json::json!("control")
        );
        assert_eq!(
            control.get(Some("missing")),
            Err(ControlError::UnknownTunnel("missing".to_owned()))
        );
    }

    // --- Plan 351 Gate 2: the control surface --------------------------------------------------

    /// A structurally valid `.b33` address, built through the protocol constructor.
    fn plan351_encrypted_address() -> String {
        i2pr_proto::EncryptedServiceAddress::new(7, 11, [0x77; 32], false, false)
            .expect("address")
            .to_text()
            .expect("address text")
    }

    /// Gate 2: the consumer lookup secret is accepted on a client **only** in the one shape where
    /// it has meaning, and refused everywhere else.
    ///
    /// The rule has three outcomes and all three are asserted, because the two refusals fail for
    /// different reasons and a row that only checked one would let the other regress:
    ///
    /// - a `.b33` target with `delay_open` -> accepted (this is the one legal combination);
    /// - an ordinary `.b32` target -> refused, because the secret derives a *blinded storage key*
    ///   and there is no blinded key in an ordinary lookup, so the value would be inert;
    /// - a `.b33` without `delay_open` -> refused by Gate 1's spec-level rule, which is the
    ///   containment: an eager client cannot isolate its own failure.
    ///
    /// Without the second row the secret would be accepted as a silent no-op, which is the worst
    /// of the three outcomes: the operator believes the lookup is protected and it is not.
    #[test]
    fn plan351_consumer_lookup_secret_requires_an_encrypted_target() {
        let address = plan351_encrypted_address();

        // (a) the legal combination.
        let mut options = client_options(&address, 0);
        options.insert("delay_open".to_owned(), "true".to_owned());
        options.insert("leaseset_password".to_owned(), "correct horse".to_owned());
        let spec = normalize_definition("enc", TunnelType::Client, &options, true)
            .and_then(|definition| build_control_spec(&definition))
            .expect("a delay-open client with an encrypted target is the legal shape");
        assert!(
            spec.destination
                .as_ref()
                .and_then(|reference| reference.encrypted_service())
                .is_some(),
            "the committed spec must carry the encrypted reference"
        );
        assert!(spec.timeouts.delay_open);

        // (b) an ordinary target: the secret is inert, so it is refused.
        let ordinary = format!("{}.b32.i2p", i2pr_proto::base32_encode(&[0x62; 32]));
        let mut options = client_options(&ordinary, 0);
        options.insert("delay_open".to_owned(), "true".to_owned());
        options.insert("leaseset_password".to_owned(), "correct horse".to_owned());
        let error = normalize_definition("plain", TunnelType::Client, &options, true)
            .expect_err("a consumer secret on an ordinary target must be refused");
        assert!(
            matches!(error, ControlError::ContradictoryOptions { .. }),
            "expected the pairing rule, got {error:?}"
        );

        // (c) an encrypted target without delay open: refused by Gate 1 at the spec layer.
        let mut options = client_options(&address, 0);
        options.insert("leaseset_password".to_owned(), "correct horse".to_owned());
        assert!(
            normalize_definition("eager", TunnelType::Client, &options, true).is_err(),
            "an encrypted target on an eager client must be refused even with a valid secret"
        );
    }

    /// The publisher-side secret keys keep their existing meaning on a server, and the two
    /// client-inapplicable keys stay refused there.
    ///
    /// Plan 351 widened exactly one key's reach (`leaseset_password`, as a consumer). A widening
    /// is the kind of change that quietly carries others along with it, so the untouched keys are
    /// asserted rather than assumed.
    #[test]
    fn plan351_only_the_lookup_secret_widened() {
        for secret in ["encrypt_lease_set", "leaseset_client_auth"] {
            let mut options = client_options(&plan351_encrypted_address(), 0);
            options.insert("delay_open".to_owned(), "true".to_owned());
            options.insert(secret.to_owned(), "x".to_owned());
            let error = normalize_definition("narrow", TunnelType::Client, &options, true)
                .expect_err("publisher-only keys stay refused on a client");
            assert!(
                matches!(error, ControlError::UnsupportedOption(ref option) if option == secret),
                "expected UnsupportedOption({secret}), got {error:?}"
            );
        }
    }

    // ------------------------------------------------------------------
    // Plan 342: the outproxy block at the control boundary.
    // ------------------------------------------------------------------

    /// A real router-bound store, so a sealing test cannot pass against a
    /// stub that stores plaintext.
    fn plan342_store() -> Arc<dyn RouterSecretOwner> {
        let mut rng = i2pr_crypto::OsRng;
        let bundle = i2pr_crypto::RouterIdentityBundle::generate(&mut rng).expect("bundle");
        Arc::new(
            crate::outbound_secret::RouterBoundOutboundSecrets::from_router_identity(&bundle)
                .expect("router-bound store"),
        )
    }

    /// A real Base32 destination for the outproxy list.
    fn plan342_b32(byte: u8) -> String {
        format!(
            "{}.b32.i2p",
            i2pr_service_tunnels::encode_b32_label(&[byte; 32])
        )
    }

    /// The complete seven-field block on an `httpclient`, optionally with a
    /// credential. Uses the real control-client spelling (`target_destination`
    /// plus a loopback listener) so the row exercises the same option map a
    /// TunnelManager `create` produces.
    fn plan342_http_options(credential: Option<(&str, &str)>) -> BTreeMap<String, String> {
        let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
        options.insert(
            "proxy_list".to_owned(),
            format!("{},staging.example.i2p", plan342_b32(0x5a)),
        );
        options.insert("use_outproxy_plugin".to_owned(), "true".to_owned());
        options.insert("outproxy_type".to_owned(), "http".to_owned());
        options.insert("ssl_proxies".to_owned(), "staging.example.i2p".to_owned());
        match credential {
            None => {
                options.insert("outproxy_auth".to_owned(), "false".to_owned());
                options.insert("outproxy_username".to_owned(), String::new());
                options.insert("outproxy_password".to_owned(), String::new());
            }
            Some((user, password)) => {
                options.insert("outproxy_auth".to_owned(), "true".to_owned());
                options.insert("outproxy_username".to_owned(), user.to_owned());
                options.insert("outproxy_password".to_owned(), password.to_owned());
            }
        }
        options
    }

    /// The complete block reaches a definition and a spec, and the policy
    /// rides on the HTTP client options.
    #[test]
    fn plan342_complete_block_reaches_the_spec() {
        let options = plan342_http_options(None);
        let definition = normalize_definition_with_filter_root(
            "outproxy",
            TunnelType::HttpClient,
            &options,
            false,
            None,
            &NoOutboundSecrets,
        )
        .expect("the complete block normalizes");
        let spec = build_control_spec(&definition).expect("the block maps to a spec");
        let outproxy = spec
            .http_options
            .as_ref()
            .and_then(|http| http.outproxy.as_ref())
            .expect("the route policy rides on the http-client options");
        assert_eq!(outproxy.list.len(), 2);
        assert_eq!(outproxy.tunnelled.len(), 1);
        assert!(!outproxy.present_credential);
        assert!(outproxy.list.select(0).is_some());
    }

    /// Every prefix of the block is refused, and the refusal names what is
    /// missing.
    ///
    /// The "every prefix" framing is the point: a single-key or six-key test
    /// would pass while some intermediate shape slipped through, and an
    /// intermediate shape is exactly what produces a tunnel that looks
    /// egress-capable and is not.
    #[test]
    fn plan342_no_proper_prefix_of_the_block_is_accepted() {
        let full = plan342_http_options(None);
        let keys: Vec<String> = full.keys().cloned().collect();
        // Only the outproxy keys, and only for lengths below the full set.
        let block_keys: Vec<String> = keys
            .iter()
            .filter(|key| crate::outproxy_options::OUTPROXY_BLOCK_KEYS.contains(&key.as_str()))
            .cloned()
            .collect();
        assert_eq!(block_keys.len(), 7);
        for take in 1..block_keys.len() {
            let mut options = client_options(&format!("{}.b32.i2p", "a".repeat(52)), 0);
            for key in &block_keys[..take] {
                options.insert(key.clone(), full.get(key).cloned().expect("fixture value"));
            }
            let error = normalize_definition("partial", TunnelType::HttpClient, &options, false)
                .expect_err("a partial block must be refused");
            match error {
                ControlError::OutproxyBlockRejected(reason) => {
                    assert!(reason.contains("all-or-none"), "{reason}");
                    let missing: Vec<&str> =
                        block_keys[take..].iter().map(String::as_str).collect();
                    for key in missing {
                        assert!(reason.contains(key), "{reason} should name missing {key}");
                    }
                }
                other => panic!("unexpected error for {take} keys: {other:?}"),
            }
        }
    }

    /// A configured credential is sealed into the stored definition, and the
    /// plaintext never reaches the persisted options.
    #[test]
    fn plan342_outproxy_password_is_sealed_before_storage() {
        let store = plan342_store();
        let options = plan342_http_options(Some(("operator", "s3cret!")));
        let definition = normalize_definition_with_filter_root(
            "sealed",
            TunnelType::HttpClient,
            &options,
            false,
            None,
            store.as_ref(),
        )
        .expect("the complete block with a credential normalizes");

        let stored = definition
            .options
            .get("outproxy_password")
            .expect("the key is present");
        assert_ne!(stored, "s3cret!", "the plaintext must not be persisted");
        assert!(
            !format!("{definition:?}").contains("s3cret!"),
            "the plaintext leaked into the definition's Debug"
        );

        // And it round-trips: the owner that sealed it opens it back to the
        // same bytes. This is the row that makes the sealed form usable at
        // request time rather than merely opaque.
        let recovered = store
            .open(stored)
            .expect("the owner opens its own stored form");
        assert_eq!(recovered.expose_str().expect("utf-8"), "s3cret!");

        // The username stays in the clear by design — it is an identifier,
        // not a credential — but the sealed password must not be mistaken for
        // one when an operator reads the definition.
        assert_eq!(
            definition
                .options
                .get("outproxy_username")
                .map(String::as_str),
            Some("operator")
        );
    }

    /// Without an installed owner, a configured credential fails the
    /// transaction instead of being stored in the clear.
    ///
    /// This is Plan 342 invariant 3 stated as a test: the refusal must happen
    /// in normalization, which is before the store, the manager, and any
    /// listener or destination.
    #[test]
    fn plan342_credential_without_a_secret_owner_is_refused_not_stored() {
        let options = plan342_http_options(Some(("operator", "s3cret!")));
        let error = normalize_definition_with_filter_root(
            "nosecret",
            TunnelType::HttpClient,
            &options,
            false,
            None,
            &NoOutboundSecrets,
        )
        .expect_err("no owner means no credential");
        match error {
            ControlError::OutproxyBlockRejected(reason) => {
                assert!(reason.contains("no outbound secret owner"), "{reason}");
                assert!(reason.contains("was not created"), "{reason}");
            }
            other => panic!("unexpected error: {other:?}"),
        }

        // The same block with no credential needs no owner and is accepted:
        // the fail-closed default must not make the whole feature unusable on
        // a control plane without an identity.
        normalize_definition_with_filter_root(
            "nosecret",
            TunnelType::HttpClient,
            &plan342_http_options(None),
            false,
            None,
            &NoOutboundSecrets,
        )
        .expect("a credential-free block needs no owner");
    }

    /// A complete block on a kind with no clearnet request target is refused.
    #[test]
    fn plan342_block_is_refused_on_kinds_without_a_request_target() {
        // `client` and `ircclient` both take a `target_destination`, and both
        // are refused — not for a missing destination but because neither has
        // a clearnet request target an outproxy could carry.
        for tunnel_type in [TunnelType::Client, TunnelType::IrcClient] {
            let options = plan342_http_options(None);
            let error = normalize_definition("wrongkind", tunnel_type, &options, false)
                .expect_err("a kind with no clearnet target refuses the block");
            assert!(
                matches!(&error, ControlError::OutproxyBlockRejected(reason) if reason.contains("no clearnet request target")),
                "unexpected error for {tunnel_type:?}: {error:?}"
            );
        }
    }

    /// A clearnet outproxy entry never reaches a definition, whatever the
    /// other six fields say.
    #[test]
    fn plan342_clearnet_outproxy_entry_is_refused_at_the_boundary() {
        for entry in ["proxy.example.com", "10.0.0.1", "proxy.example.com:8888"] {
            let mut options = plan342_http_options(None);
            options.insert("proxy_list".to_owned(), entry.to_owned());
            let error = normalize_definition("clearnet", TunnelType::HttpClient, &options, false)
                .expect_err("a clearnet outproxy must be refused");
            assert!(
                matches!(&error, ControlError::InvalidOption { option, .. } if option == "ProxyList"),
                "unexpected error for {entry}: {error:?}"
            );
        }
    }

    /// `OutproxyType` stays a closed vocabulary at the control boundary: no
    /// spelling reaches a path, a command, or a module.
    #[test]
    fn plan342_outproxy_type_is_a_closed_vocabulary_at_the_boundary() {
        for value in [
            "/usr/bin/curl",
            "sh -c 'id'",
            "HTTP_CONNECT_PLUGIN",
            "socks6",
        ] {
            let mut options = plan342_http_options(None);
            options.insert("outproxy_type".to_owned(), value.to_owned());
            let error = normalize_definition("badtype", TunnelType::HttpClient, &options, false)
                .expect_err("an out of vocabulary type must be refused");
            assert!(
                matches!(&error, ControlError::InvalidOption { option, .. } if option == "OutproxyType"),
                "unexpected error for {value}: {error:?}"
            );
        }
        for value in ["http", "HTTP", "socks5", "socks4a"] {
            let mut options = plan342_http_options(None);
            options.insert("outproxy_type".to_owned(), value.to_owned());
            normalize_definition("goodtype", TunnelType::HttpClient, &options, false)
                .unwrap_or_else(|error| panic!("{value} should parse: {error:?}"));
        }
    }

    /// A `get` response reports that a credential is configured and never
    /// carries its value — including the sealed form, which Java's
    /// `rawConfig` would return in the clear.
    #[test]
    fn plan342_get_redacts_the_outproxy_credential() {
        let store = plan342_store();
        let data_dir = tempfile::tempdir().expect("temp dir");
        let state = test_control(data_dir.path());
        let mut options = plan342_http_options(Some(("operator", "s3cret!")));
        options.insert("listen_port".to_owned(), "0".to_owned());
        // Normalization uses the router-bound store; the state under test only
        // has to hold the definition, so its own owner is irrelevant here.
        let definition = normalize_definition_with_filter_root(
            "redacted",
            TunnelType::HttpClient,
            &options,
            false,
            None,
            store.as_ref(),
        )
        .expect("normalizes");
        let response = state.control_response("redacted", &definition);
        let reported = response
            .get("options")
            .and_then(serde_json::Value::as_object)
            .expect("options object");
        assert_eq!(
            reported
                .get("outproxy_password")
                .and_then(serde_json::Value::as_str),
            Some("[redacted]")
        );
        let serialized = response.to_string();
        assert!(
            !serialized.contains("s3cret!"),
            "credential leaked into get"
        );
        // The username is an identifier and stays visible, so an operator can
        // tell *which* account the outproxy expects.
        assert_eq!(
            reported
                .get("outproxy_username")
                .and_then(serde_json::Value::as_str),
            Some("operator")
        );
    }

    /// Plan 380, correcting forward: an edit that changes nothing about a sealed credential must
    /// leave the sealed form **byte-identical**, for both sealed-credential domains.
    ///
    /// This is the row Plan 342 and Plan 376 could not write. Both exercised generation round
    /// trips and restarts, neither of which re-runs the seal step; neither edited an outproxy
    /// tunnel at all. A probe during Plan 380 showed what an edit actually did to the outproxy
    /// credential: the stored form was sealed a second time, so opening it once returned the
    /// previous stored form as *text*, which the provider then presented to the outproxy as the
    /// password. The credential was destroyed by an edit, silently.
    ///
    /// The assertion is byte equality rather than "still opens", because a re-seal produces a form
    /// that still opens — it just opens to the wrong thing.
    #[test]
    fn plan380_an_unrelated_edit_leaves_the_sealed_credential_unchanged() {
        let store = plan342_store();

        // Domain one: the Plan 342 outproxy credential.
        let outproxy_options = plan342_http_options(Some(("operator", "s3cret!")));
        let outproxy = normalize_definition_with_filter_root(
            "editoutproxy",
            TunnelType::HttpClient,
            &outproxy_options,
            false,
            None,
            store.as_ref(),
        )
        .expect("normalizes");
        let sealed_outproxy = outproxy
            .options
            .get("outproxy_password")
            .expect("a credential was sealed")
            .clone();

        // Domain two: the Plan 380 ELS2 consumer credential.
        // A real `.b33`, not a well-shaped string: the pairing rule parses it, so a synthetic
        // label would be refused before the seal step ever ran and this row would prove nothing.
        let target = i2pr_proto::EncryptedServiceAddress::new(
            i2pr_proto::B32_UNBLINDED_SIGTYPE_RED25519,
            i2pr_proto::B32_BLINDED_SIGTYPE,
            [0x3c; 32],
            true,
            true,
        )
        .expect("an authorized service address")
        .to_text()
        .expect("address text");
        let credential_options = BTreeMap::from([
            ("target_destination".to_owned(), target.clone()),
            ("delay_open".to_owned(), "true".to_owned()),
            (
                crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION.to_owned(),
                format!("psk:{}", "5b".repeat(32)),
            ),
        ]);
        let credential = normalize_definition_with_filter_root(
            "editcredential",
            TunnelType::Client,
            &credential_options,
            false,
            None,
            store.as_ref(),
        )
        .expect("normalizes");
        let sealed_credential = credential
            .options
            .get(crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION)
            .expect("a credential was sealed")
            .clone();

        // `edit` merges the stored options with the request's and re-normalizes the whole
        // candidate, which is the only place the seal step runs twice. A description change is the
        // smallest such edit that is still a real one.
        for (name, tunnel_type, options, key, sealed) in [
            (
                "editoutproxy",
                TunnelType::HttpClient,
                outproxy.options.clone(),
                "outproxy_password",
                sealed_outproxy.clone(),
            ),
            (
                "editcredential",
                TunnelType::Client,
                credential.options.clone(),
                crate::encrypted_target_credential::ELS2_CREDENTIAL_OPTION,
                sealed_credential.clone(),
            ),
        ] {
            let mut merged = options;
            merged.insert("description".to_owned(), "changed".to_owned());
            let edited = normalize_definition_with_filter_root(
                name,
                tunnel_type,
                &merged,
                false,
                None,
                store.as_ref(),
            )
            .unwrap_or_else(|error| panic!("{name}: an unrelated edit must not fail: {error}"));
            assert_eq!(
                edited.options.get(key),
                Some(&sealed),
                "{name}: an unrelated edit must not re-seal {key}; a fresh nonce would change the \
                 stored bytes for a definition whose credential did not change"
            );
        }

        // And the outproxy credential still yields the operator's password, not a stored form.
        let opened = store.open(&sealed_outproxy).expect("opens");
        let reopened = opened.expose_str().expect("utf-8");
        assert_eq!(
            reopened, "s3cret!",
            "one open of a once-sealed form must be the plaintext, never another stored form"
        );
    }

    /// A stored definition reloads with a complete block, which is the row
    /// that proves the sealed password survives a restart.
    #[test]
    fn plan342_sealed_block_survives_a_generation_round_trip() {
        let store = plan342_store();
        let options = plan342_http_options(Some(("operator", "s3cret!")));
        let definition = normalize_definition_with_filter_root(
            "roundtrip",
            TunnelType::HttpClient,
            &options,
            false,
            None,
            store.as_ref(),
        )
        .expect("normalizes");
        let store_dir = tempfile::tempdir().expect("temp dir");
        let control_store = ControlStore::open(store_dir.path()).expect("control store");
        let mut definitions = BTreeMap::new();
        definitions.insert("roundtrip".to_owned(), definition.clone());
        let staged = control_store.stage(&definitions).expect("staged");
        control_store.publish(staged).expect("published");
        let loaded = control_store.load().expect("reloaded");
        let reloaded = loaded
            .definitions
            .into_iter()
            .find(|candidate| candidate.name == "roundtrip")
            .expect("the definition survived publication");
        assert_eq!(
            reloaded.options.get("outproxy_password"),
            definition.options.get("outproxy_password"),
            "the sealed form must round-trip byte-identically. This row covers the *file* round \
             trip only: a generation reload does not re-run the seal step. That the same property \
             holds across an `edit` — which merges options and therefore does re-run it — is a \
             separate row, `plan380_an_unrelated_edit_leaves_the_sealed_credential_unchanged`, \
             written by Plan 380 after it found that the claim did not hold."
        );
        // And it still builds: a stored definition is re-validated by the same
        // block rule, so a generation written under a different rule fails
        // closed instead of silently losing its route.
        let spec = build_control_spec(&reloaded).expect("a reloaded block still maps");
        assert!(
            spec.http_options
                .as_ref()
                .and_then(|http| http.outproxy.as_ref())
                .is_some()
        );
    }

    /// Plan 342 — the whole chain that makes a clearnet request reachable, in
    /// one row: definition → reconciliation → provider registry → the tunnel's
    /// request-target policy.
    ///
    /// The row exists because the pieces were individually present and the
    /// chain was not. `sync_outproxy_providers` installed the provider, the
    /// request paths classified their target, and the unit rows for both
    /// passed -- and a clearnet authority could still never arrive at the
    /// classifier, because every request-target grammar in the tree required a
    /// `.i2p` suffix and refused before it. `ServiceTunnelManager::target_policy`
    /// is the link, and this row is what holds it in place.
    ///
    /// The removal half matters as much as the install half: a provider dropped
    /// from the registry must take the policy back to strict, so the *next*
    /// request is refused at the parser instead of being parsed and then
    /// refused at the classifier.
    #[test]
    fn plan342_provider_registry_drives_the_request_target_policy() {
        let directory = TempDir::new().expect("temp dir");
        let control = test_control(directory.path());
        let manager = Arc::clone(&control.manager);

        // Nothing configured yet: strict, so a clearnet authority is refused
        // by the parser.
        assert_eq!(
            manager.target_policy("tunnel-a"),
            i2pr_service_tunnels::target_policy::TargetPolicy::I2pOnly,
            "a tunnel with no outproxy provider must parse strictly"
        );

        // A complete block, published the way TunnelManager would.
        let definition = normalize_definition_with_filter_root(
            "tunnel-a",
            TunnelType::ConnectClient,
            &plan342_http_options(None),
            false,
            None,
            plan342_store().as_ref(),
        )
        .expect("the complete block normalizes");
        lock(&control.definitions).insert("tunnel-a".to_owned(), definition);
        control.sync_outproxy_providers().expect("providers sync");

        assert!(
            manager.outproxy_provider("tunnel-a").is_some(),
            "the block must install a provider"
        );
        assert_eq!(
            manager.target_policy("tunnel-a"),
            i2pr_service_tunnels::target_policy::TargetPolicy::AllowsClearnet,
            "an installed provider is what lets a clearnet authority reach the classifier"
        );

        // Removing the definition drops the provider and the policy together.
        lock(&control.definitions).remove("tunnel-a");
        control
            .sync_outproxy_providers()
            .expect("providers sync again");
        assert!(manager.outproxy_provider("tunnel-a").is_none());
        assert_eq!(
            manager.target_policy("tunnel-a"),
            i2pr_service_tunnels::target_policy::TargetPolicy::I2pOnly,
            "removing the provider must return the parser to strict, not leave a relaxed one"
        );
    }
}
