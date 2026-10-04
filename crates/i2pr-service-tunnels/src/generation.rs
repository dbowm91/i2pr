//! Plan 180 §3/§5 runtime-neutral generation diff model.
//!
//! Typed diff classifications between two `ServiceTunnelSet`
//! (`crate::config::ServiceTunnelSet`) configurations. The
//! runtime-neutral module never owns sockets, tasks, timers, or
//! generation state; it only classifies what changed between a
//! committed generation and a candidate one so the daemon-owned
//! `ServiceTunnelManager` can stage, swap, or drain deterministically.
//!
//! Classification rules (Plan 180 §5):
//!
//! - unchanged spec -> retain listener/destination where practical;
//! - resource-limit change that is safe to swap in place
//!   ([`DiffClass::MutableInPlace`]) only when semantics are
//!   explicitly bounded;
//! - listener bind/port change -> replace listener;
//! - target destination change -> replace client connection
//!   target/runtime binding;
//! - server identity path or destination policy change -> replace
//!   destination;
//! - protocol kind change -> full replacement;
//! - disabled/removed -> drain/remove.
//!
//! Do not infer [`DiffClass::MutableInPlace`] merely to reduce
//! churn. Default to replacement when ownership is ambiguous.
//!
//! The diff is deterministic, structural, and carries no secrets,
//! payload bytes, or runtime resources.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use crate::config::{DestinationGroupKey, ServiceTunnelKind, ServiceTunnelSpec};

/// Typed classification of how one service spec differs from a
/// prior committed generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffClass {
    /// Spec is structurally identical to the committed generation;
    /// the listener/destination can be retained unchanged.
    Unchanged,
    /// Only bounded resource/deadline limits changed; future
    /// admissions pick up the new limits but the listener and
    /// destination stay live.
    MutableInPlace,
    /// The listener bind endpoint changed; the old listener stops
    /// accepting and a new listener is staged. Existing
    /// connections drain under the deadline.
    ReplaceListener,
    /// The client destination reference or per-service resource
    /// ceiling changed in a way that requires a new destination
    /// runtime. The old destination drains under the deadline.
    ReplaceDestination,
    /// The service was removed; existing connections drain under
    /// the deadline and the runtime is torn down.
    Remove,
    /// The service is new in the candidate generation.
    Add,
}

/// One diff entry paired with the spec identity so the daemon can
/// stage replacements in order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceDiff {
    /// Service identifier.
    pub id: String,
    /// Classification of the change.
    pub class: DiffClass,
    /// The candidate spec the manager is staging toward (None when
    /// [`DiffClass::Remove`]).
    pub next: Option<ServiceTunnelSpec>,
}

impl ServiceDiff {
    /// Returns the spec id without allocation.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns `true` when the entry is structural noise (no
    /// runtime work required).
    pub fn is_no_op(&self) -> bool {
        matches!(self.class, DiffClass::Unchanged | DiffClass::MutableInPlace)
    }
}

/// Diffs a candidate `ServiceTunnelSet` against a committed one
/// and returns one [`ServiceDiff`] per candidate spec in candidate
/// order. Services present in the committed set but missing from
/// the candidate are emitted as [`DiffClass::Remove`] in committed
/// order, appended after the candidate entries.
pub fn diff_sets(
    committed: &[ServiceTunnelSpec],
    candidate: &[ServiceTunnelSpec],
) -> Vec<ServiceDiff> {
    let previous_groups = group_members(committed);
    let candidate_groups = group_members(candidate);
    let mut out = Vec::with_capacity(committed.len() + candidate.len());
    let mut committed_by_id: std::collections::HashMap<&str, &ServiceTunnelSpec> =
        std::collections::HashMap::with_capacity(committed.len());
    for spec in committed {
        committed_by_id.insert(spec.id.as_str(), spec);
    }
    for next_spec in candidate {
        let id = next_spec.id.as_str().to_owned();
        match committed_by_id.remove(id.as_str()) {
            None => out.push(ServiceDiff {
                id,
                class: DiffClass::Add,
                next: Some(next_spec.clone()),
            }),
            Some(prev) => {
                let group_key = next_spec.policy.group_key(&next_spec.id);
                let old_group_key = prev.policy.group_key(&prev.id);
                let group_changed = old_group_key != group_key
                    || previous_groups.get(&old_group_key) != candidate_groups.get(&group_key);
                let class = if group_changed {
                    DiffClass::ReplaceDestination
                } else {
                    diff_spec(prev, next_spec)
                };
                out.push(ServiceDiff {
                    id,
                    class,
                    next: Some(next_spec.clone()),
                });
            }
        }
    }
    for (id, _prev) in committed_by_id {
        out.push(ServiceDiff {
            id: id.to_owned(),
            class: DiffClass::Remove,
            next: None,
        });
    }
    out
}

fn group_members(specs: &[ServiceTunnelSpec]) -> HashMap<DestinationGroupKey, Vec<String>> {
    let mut groups = HashMap::<DestinationGroupKey, Vec<String>>::new();
    for spec in specs {
        groups
            .entry(spec.policy.group_key(&spec.id))
            .or_default()
            .push(spec.id.as_str().to_owned());
    }
    for members in groups.values_mut() {
        members.sort();
    }
    groups
}

/// Classifies a single spec change. Public so unit tests can
/// exercise classification rules independently.
pub fn diff_spec(prev: &ServiceTunnelSpec, next: &ServiceTunnelSpec) -> DiffClass {
    if prev.kind != next.kind {
        return DiffClass::ReplaceDestination;
    }
    if prev.enabled != next.enabled {
        // Disabling is treated as Remove; enabling a previously
        // disabled entry is Add (diff_sets will never compare a
        // disabled entry in the committed set, so this branch
        // only fires for transitions between enable/disable
        // within a single generation, which is a replace).
        return DiffClass::ReplaceDestination;
    }
    if prev.kind.is_server() {
        if prev.target != next.target
            || prev.targets != next.targets
            || prev.policy != next.policy
            || prev.inbound_port != next.inbound_port
        {
            return DiffClass::ReplaceDestination;
        }
    } else if prev.listener != next.listener
        || prev.destination != next.destination
        || prev.policy != next.policy
        || prev.inbound_port != next.inbound_port
    {
        return DiffClass::ReplaceDestination;
    }
    if prev.shaping != next.shaping {
        // Pool sizing changed: the destination runtime is constructed
        // with its config, so shaping edits replace the destination
        // runtime (pools rebuild under the same identity; active
        // streams follow the existing replace drain path).
        return DiffClass::ReplaceDestination;
    }
    if prev.http_options != next.http_options
        || prev.socks5_options != next.socks5_options
        || prev.connect_options != next.connect_options
        || prev.streamr_options != next.streamr_options
        || prev.access != next.access
    {
        // Credential, cadence, and peer-policy edits rebuild: live
        // listeners and loops capture their options at supervisor
        // start, so stale credentials, cadence, or policies must
        // never linger on a running runtime. (Side effect: profile
        // tweaks in these structs now take effect via rebuild
        // instead of waiting for a restart.)
        return DiffClass::ReplaceDestination;
    }
    if prev.streaming_interactive != next.streaming_interactive {
        // The streaming managers are constructed with their window
        // configuration, so profile edits replace the destination
        // runtime the same way shaping edits do.
        return DiffClass::ReplaceDestination;
    }
    if prev.max_connections != next.max_connections
        || prev.max_buffered_bytes_per_direction != next.max_buffered_bytes_per_direction
        || prev.timeouts != next.timeouts
        || prev.irc_options != next.irc_options
        || prev.idle != next.idle
        || prev.unique_local_address != next.unique_local_address
        || prev.http_policy != next.http_policy
        || prev.multihoming != next.multihoming
        || prev.reply_bundling != next.reply_bundling
        || prev.use_ssl != next.use_ssl
    {
        // Resource/deadline/profile-only differences are safe to
        // swap in place; nothing has been wired that depends on
        // these values being immutable. Plan 292: the idle sweep
        // reads the committed spec each tick, so idle edits take
        // effect without rebuilding the runtime. The server
        // target dial and the HTTP presentation filter likewise
        // read the committed dial/presentation behavior per
        // connection and per request. Plan 296: multihoming target
        // selection reads the committed flag and target list per
        // connection, and the outbound sweep reads the committed
        // reply-bundling flag per sweep, so both edits take effect
        // without rebuilding the runtime. Plan 297: the server TLS
        // dial reads the committed use_ssl flag per connection
        // against the daemon TLS policy, so those edits take effect
        // without rebuilding the runtime either.
        return DiffClass::MutableInPlace;
    }
    DiffClass::Unchanged
}

/// Convenience: returns the kind string for log-only diagnostics
/// (the runtime-neutral module owns no I/O so this is a pure
/// formatter).
pub fn kind_string(kind: ServiceTunnelKind) -> &'static str {
    kind.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        DestinationPolicy, IdlePolicy, LocalListenerSpec, ServerTarget, ServiceTimeouts,
        TunnelShaping,
    };
    use crate::destination::DestinationRef;

    mod replace_byte {
        pub fn set(value: &mut String, index: usize, byte: u8) {
            let mut bytes = value.as_bytes().to_vec();
            if let Some(slot) = bytes.get_mut(index) {
                *slot = byte;
            }
            *value = String::from_utf8(bytes).expect("utf-8");
        }
    }

    fn canonical_b32() -> String {
        format!("{}{}.b32.i2p", "a".repeat(52), "")
    }

    fn client_spec(id: &str) -> ServiceTunnelSpec {
        ServiceTunnelSpec {
            id: crate::config::ServiceTunnelId::parse(id).expect("id"),
            kind: ServiceTunnelKind::GenericClient,
            enabled: true,
            listener: Some(LocalListenerSpec::parse_socket("127.0.0.1:0").expect("listener")),
            target: None,
            targets: Vec::new(),
            destination: Some(DestinationRef::parse(&canonical_b32()).expect("dest")),
            policy: DestinationPolicy::Dedicated,
            inbound_port: None,
            max_connections: 4,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        }
    }

    fn server_spec(id: &str) -> ServiceTunnelSpec {
        ServiceTunnelSpec {
            id: crate::config::ServiceTunnelId::parse(id).expect("id"),
            kind: ServiceTunnelKind::GenericServer,
            enabled: true,
            listener: None,
            target: Some(ServerTarget::LoopbackTcp("127.0.0.1:0".parse().unwrap())),
            targets: Vec::new(),
            destination: None,
            policy: DestinationPolicy::Dedicated,
            inbound_port: None,
            max_connections: 4,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            shaping: TunnelShaping::balanced(),
            streaming_interactive: false,
            idle: IdlePolicy::disabled(),
            access: crate::access::ServerAccessPolicy::default(),
            unique_local_address: false,
            multihoming: false,
            reply_bundling: false,
            use_ssl: false,
            http_policy: crate::http::HttpServerPolicy::default(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
            connect_options: None,
            streamr_options: None,
        }
    }

    #[test]
    fn identical_specs_classify_as_unchanged() {
        let prev = client_spec("alpha");
        let next = prev.clone();
        assert_eq!(diff_spec(&prev, &next), DiffClass::Unchanged);
    }

    #[test]
    fn shaping_change_is_replace_destination() {
        // Plan 292: pool sizing is baked into the destination
        // runtime at construction, so any shaping edit replaces
        // the destination (pools rebuild under the same identity).
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.shaping = TunnelShaping::try_new(4, 4, 2, 0, 0).expect("shaping");
        assert_eq!(diff_spec(&prev, &next), DiffClass::ReplaceDestination);
        let mut length_only = prev.clone();
        length_only.shaping = TunnelShaping::try_new(2, 2, 3, 0, 0).expect("shaping");
        assert_eq!(
            diff_spec(&prev, &length_only),
            DiffClass::ReplaceDestination
        );
    }

    #[test]
    fn profile_change_is_replace_destination() {
        // Plan 292: the streaming managers are constructed with
        // their window configuration, so profile edits replace the
        // destination runtime like shaping edits do.
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.streaming_interactive = true;
        assert_eq!(diff_spec(&prev, &next), DiffClass::ReplaceDestination);
    }

    #[test]
    fn idle_change_is_mutable_in_place() {
        // Plan 292: the sweep reads the committed spec each tick,
        // so idle edits take effect without rebuilding the runtime.
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.idle = IdlePolicy::try_new(Some(60_000), true, false, false).expect("idle");
        assert_eq!(diff_spec(&prev, &next), DiffClass::MutableInPlace);
    }

    #[test]
    fn resource_limit_change_is_mutable_in_place() {
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.max_connections = 8;
        assert_eq!(diff_spec(&prev, &next), DiffClass::MutableInPlace);
    }

    #[test]
    fn dial_and_presentation_changes_are_mutable_in_place() {
        // Plan 292: the server target dial reads the committed
        // unique-local flag per connection and the HTTP filter
        // reads the committed presentation policy per request, so
        // both edits take effect without rebuilding the runtime.
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.unique_local_address = true;
        assert_eq!(diff_spec(&prev, &next), DiffClass::MutableInPlace);
        let mut gated = prev.clone();
        gated.http_policy = crate::http::HttpServerPolicy {
            address_helper: false,
            ..crate::http::HttpServerPolicy::default()
        };
        assert_eq!(diff_spec(&prev, &gated), DiffClass::MutableInPlace);
        let mut host_override = prev.clone();
        host_override.http_policy.spoofed_host = Some("site.example.i2p".to_owned());
        assert_eq!(
            diff_spec(&prev, &host_override),
            DiffClass::MutableInPlace,
            "per-connection HTTP host policy updates without destination churn"
        );
    }

    #[test]
    fn shaping_and_delivery_flag_edits_classify() {
        // Plan 296: backup/variance ride the shaping struct (any
        // shaping edit replaces the destination); multihoming and
        // reply bundling read the committed spec per
        // connection/sweep, so those edits are mutable in place.
        let prev = client_spec("alpha");
        let mut shaped = prev.clone();
        shaped.shaping = TunnelShaping::try_new(2, 2, 2, 1, 1).expect("shaping");
        assert_eq!(diff_spec(&prev, &shaped), DiffClass::ReplaceDestination);
        let mut multi = prev.clone();
        multi.multihoming = true;
        assert_eq!(diff_spec(&prev, &multi), DiffClass::MutableInPlace);
        let mut bundled = prev.clone();
        bundled.reply_bundling = true;
        assert_eq!(diff_spec(&prev, &bundled), DiffClass::MutableInPlace);
        let mut tls = prev.clone();
        tls.use_ssl = true;
        assert_eq!(diff_spec(&prev, &tls), DiffClass::MutableInPlace);
    }

    #[test]
    fn streamr_sink_change_is_replace_destination() {
        // Plan 292: the subscriber loop captures its UDP sink at
        // supervisor start, so sink edits rebuild the runtime.
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.streamr_options = Some(crate::streamr::StreamrOptions {
            remote_sink: Some("127.0.0.1:5009".parse().expect("sink")),
            ..crate::streamr::StreamrOptions::default()
        });
        assert_eq!(diff_spec(&prev, &next), DiffClass::ReplaceDestination);
    }

    #[test]
    fn listener_change_is_replace_destination() {
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.listener = Some(LocalListenerSpec::parse_socket("127.0.0.1:9999").expect("listener"));
        assert_eq!(diff_spec(&prev, &next), DiffClass::ReplaceDestination);
    }

    #[test]
    fn destination_change_is_replace_destination() {
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        // Build a syntactically-valid distinct b32 destination by
        // mutating one label byte inside the canonical alphabet.
        let mut b = canonical_b32();
        replace_byte::set(&mut b, 1, b'b');
        next.destination = Some(DestinationRef::parse(&b).expect("dest"));
        assert_eq!(diff_spec(&prev, &next), DiffClass::ReplaceDestination);
    }

    #[test]
    fn server_target_change_is_replace_destination() {
        let prev = server_spec("alpha-server");
        let mut next = prev.clone();
        next.target = Some(ServerTarget::LoopbackTcp("127.0.0.1:9999".parse().unwrap()));
        assert_eq!(diff_spec(&prev, &next), DiffClass::ReplaceDestination);
    }

    #[test]
    fn kind_change_is_replace_destination() {
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.kind = ServiceTunnelKind::HttpClient;
        assert_eq!(diff_spec(&prev, &next), DiffClass::ReplaceDestination);
    }

    #[test]
    fn added_service_yields_add_diff() {
        let next = client_spec("alpha");
        let diff = diff_sets(&[], std::slice::from_ref(&next));
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].class, DiffClass::Add);
        assert_eq!(diff[0].id, "alpha");
    }

    #[test]
    fn adding_group_member_replaces_existing_group_owner_atomically() {
        let mut first = client_spec("alpha");
        let group = crate::config::DestinationGroupId::parse("shared").expect("group");
        first.policy = DestinationPolicy::SharedGroup(group.clone());
        let mut second = client_spec("beta");
        second.policy = DestinationPolicy::SharedGroup(group);
        let diff = diff_sets(std::slice::from_ref(&first), &[first.clone(), second]);
        assert_eq!(diff[0].class, DiffClass::ReplaceDestination);
        assert_eq!(diff[1].class, DiffClass::Add);
    }

    #[test]
    fn removing_group_member_replaces_remaining_group_owner_atomically() {
        let group = crate::config::DestinationGroupId::parse("shared").expect("group");
        let mut first = client_spec("alpha");
        first.policy = DestinationPolicy::SharedGroup(group.clone());
        let mut second = client_spec("beta");
        second.policy = DestinationPolicy::SharedGroup(group);
        let diff = diff_sets(&[first, second], &[client_spec("alpha")]);
        assert_eq!(diff[0].class, DiffClass::ReplaceDestination);
        assert_eq!(diff[1].class, DiffClass::Remove);
    }

    #[test]
    fn removed_service_yields_remove_diff() {
        let prev = client_spec("alpha");
        let diff = diff_sets(std::slice::from_ref(&prev), &[]);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].class, DiffClass::Remove);
        assert_eq!(diff[0].id, "alpha");
        assert!(diff[0].next.is_none());
    }

    #[test]
    fn unchanged_spec_in_full_diff_is_classified_unchanged() {
        let prev = client_spec("alpha");
        let next = prev.clone();
        let diff = diff_sets(std::slice::from_ref(&prev), std::slice::from_ref(&next));
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].class, DiffClass::Unchanged);
    }

    #[test]
    fn is_no_op_recognizes_structural_noise() {
        let d = ServiceDiff {
            id: "alpha".to_owned(),
            class: DiffClass::Unchanged,
            next: None,
        };
        assert!(d.is_no_op());
        let d2 = ServiceDiff {
            id: "alpha".to_owned(),
            class: DiffClass::MutableInPlace,
            next: None,
        };
        assert!(d2.is_no_op());
        let d3 = ServiceDiff {
            id: "alpha".to_owned(),
            class: DiffClass::Add,
            next: None,
        };
        assert!(!d3.is_no_op());
    }
}
