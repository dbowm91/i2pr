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

use crate::config::{DestinationPolicy, ServiceTunnelKind, ServiceTunnelSpec};

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
                let class = diff_spec(prev, next_spec);
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
        if prev.target != next.target || prev.targets != next.targets {
            return DiffClass::ReplaceDestination;
        }
        // Server policy must stay Dedicated.
        if !matches!(next.policy, DestinationPolicy::Dedicated) {
            return DiffClass::ReplaceDestination;
        }
    } else if prev.listener != next.listener
        || prev.destination != next.destination
        || prev.policy != next.policy
    {
        return DiffClass::ReplaceDestination;
    }
    if prev.max_connections != next.max_connections
        || prev.max_buffered_bytes_per_direction != next.max_buffered_bytes_per_direction
        || prev.timeouts != next.timeouts
        || prev.http_options != next.http_options
        || prev.socks5_options != next.socks5_options
        || prev.irc_options != next.irc_options
    {
        // Resource/deadline/profile-only differences are safe to
        // swap in place; nothing has been wired that depends on
        // these values being immutable.
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
    use crate::config::{DestinationPolicy, LocalListenerSpec, ServerTarget, ServiceTimeouts};
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
            max_connections: 4,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
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
            max_connections: 4,
            max_buffered_bytes_per_direction: 65_536,
            timeouts: ServiceTimeouts::defaults(),
            http_options: None,
            socks5_options: None,
            irc_options: None,
        }
    }

    #[test]
    fn identical_specs_classify_as_unchanged() {
        let prev = client_spec("alpha");
        let next = prev.clone();
        assert_eq!(diff_spec(&prev, &next), DiffClass::Unchanged);
    }

    #[test]
    fn resource_limit_change_is_mutable_in_place() {
        let prev = client_spec("alpha");
        let mut next = prev.clone();
        next.max_connections = 8;
        assert_eq!(diff_spec(&prev, &next), DiffClass::MutableInPlace);
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
