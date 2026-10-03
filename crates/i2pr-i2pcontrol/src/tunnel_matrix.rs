//! Plan 292: exact tunnel-type x option disposition matrix.
//!
//! The frozen inventory ([`crate::tunnel_options::TUNNEL_OPTIONS`]) states
//! which of the 46 wire keys applies to which of the 12 tunnel types (336
//! applicable cells). This module records exactly one disposition per
//! applicable cell. Plan 292 assigned the initial dispositions; Plan 293
//! resolved every `BlockedPrimitive` cell into either an apply owner or,
//! where the primitive is demonstrably absent under project guardrails,
//! an explicit incompatibility carried by Plan 295:
//!
//! - [`CellDisposition::Apply`]: a named runtime/persistence owner consumes
//!   the key for the kind. The owner string names the consuming
//!   struct/field/sweep, never a storage mirror.
//! - [`CellDisposition::NotApplicable`]: the kind has no consuming layer
//!   for the key even though the Proposal mask group covers it. The
//!   refinement rule is uniform: within an applicable mask group, a kind
//!   without the consuming layer (TCP endpoint, HTTP presentation,
//!   streaming stack, UDP media path) is not-applicable; every other cell
//!   must be apply, incompatible, or corrective. Raw TCP servers have no HTTP
//!   presentation layer; stream kinds have no TCP endpoints; Streamr kinds
//!   ride the datagram path, not the streaming stack.
//! - [`CellDisposition::ExplicitIncompatibility`]: applicable, but the key
//!   names a capability i2pr explicitly does not provide (dynamic
//!   destination SigType, encrypted/blinded LeaseSet security and client
//!   authorization, outproxy provider). The limitation string names the
//!   missing owner. Supplying the key fails before allocation with the
//!   limitation; omitting it selects the ordinary i2pr behavior. Plan 295
//!   carries these limitations into the final support claim.
//! - [`CellDisposition::CorrectivePending`]: applicable, but the required
//!   primitive needs a new corrective plan. All Plan 296 residuals
//!   closed into apply owners (pool shaping, multihoming, reply
//!   bundling) as did the Plan 297 `use_ssl` local-TLS-identity
//!   cell; the variant remains for future correctives.
//!
//! Semantic grounding: Proposal 170 revision 2026-05-20 plus the Java
//! PR6 `TunnelManager` reference (`ClientTunnelCreator` management,
//! proxy, and filtering semantics; `ServiceTunnelCreator` server and
//! access-list semantics) and the Plan 291 Streamr freeze
//! (`specs/protocols/12-repliable-datagrams-streamr.md`). Donor
//! extensions without a pinned reference value (`interactive`,
//! `idle_timeout`, `address_helper`, `jump_list`, `remote_udp_host`,
//! per-direction shaping) carry explicit i2pr interpretations documented
//! in `specs/protocols/13-tunnel-option-matrix.md`; the mechanism follows
//! the donor, local numeric defaults are i2pr policy.

use crate::tunnel_options::TUNNEL_OPTIONS;

/// Plan 292 disposition of one applicable (type, option) cell, with Plan
/// 293 determinations applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellDisposition {
    /// A named runtime/persistence owner consumes the key for the kind.
    Apply {
        /// Consuming struct/field/sweep (never a storage mirror).
        owner: &'static str,
    },
    /// The kind has no consuming layer for the key (see refinement rule).
    NotApplicable {
        /// Why this kind cannot consume the key.
        reason: &'static str,
    },
    /// Applicable but explicitly unsupported; owned by Plan 293.
    ExplicitIncompatibility {
        /// Missing-owner limitation (named, carried by Plan 295).
        limitation: &'static str,
    },
    /// Applicable but needs a new corrective plan (296 or 297).
    CorrectivePending {
        /// Owning corrective plan.
        plan: u16,
        /// Missing primitive identity.
        reason: &'static str,
    },
}

/// One applicable matrix cell: canonical type index, option index, disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MatrixCell {
    /// Index into [`crate::tunnel::TUNNEL_TYPES`].
    pub type_index: usize,
    /// Index into [`crate::tunnel_options::TUNNEL_OPTIONS`].
    pub option_index: usize,
    /// Matrix disposition (Plan 292 assignment, Plan 293 determinations applied).
    pub disposition: CellDisposition,
}

/// Number of applicable cells (sum of inventory mask populations).
pub const MATRIX_CELLS: usize = {
    let mut n: usize = 0;
    let mut i: usize = 0;
    while i < TUNNEL_OPTIONS.len() {
        n += TUNNEL_OPTIONS[i].applies_mask.count_ones() as usize;
        i += 1;
    }
    n
};

/// Disposition for one applicable `(option_index, type_index)` pair.
///
/// Callers must only invoke this for pairs inside the option mask; the
/// `(_, _)` fallthrough is unreachable for in-mask callers and maps to a
/// defensive not-applicable rather than an apply.
const fn cell_disposition(option_index: usize, type_index: usize) -> CellDisposition {
    match option_index {
        // 0 target_host / 1 target_port: server-side TCP target endpoint.
        0 | 1 => match type_index {
            1 | 5 | 8 | 9 => CellDisposition::Apply {
                owner: "ServiceTunnelSpec.target (server TCP target)",
            },
            _ => CellDisposition::NotApplicable {
                reason: "client kinds address I2P destinations and streamr kinds use UDP; no TCP target",
            },
        },
        // 2 listen_host / 3 listen_port: client-side TCP listener endpoint.
        2 | 3 => match type_index {
            0 | 2 | 3 | 4 | 6 | 7 | 9 => CellDisposition::Apply {
                owner: "ServiceTunnelSpec.listener (client TCP listener)",
            },
            _ => CellDisposition::NotApplicable {
                reason: "server halves expose no TCP listener and streamr kinds use UDP",
            },
        },
        // 4 target_destination: client I2P destination (all masked kinds).
        4 => CellDisposition::Apply {
            owner: "ServiceTunnelSpec.destination (client I2P destination)",
        },
        // 5 target_i2p_port: only the streamr subscriber uses an I2P port;
        // streaming kinds always use port 0.
        5 => match type_index {
            10 => CellDisposition::Apply {
                owner: "StreamrOptions.target_i2p_port (subscribe destination port)",
            },
            _ => CellDisposition::NotApplicable {
                reason: "streaming kinds use I2P port 0; no per-tunnel port",
            },
        },
        // 6 use_ssl: Plan 297 server TLS owner. The flag negotiates
        // TLS to the configured loopback target under the daemon's
        // explicit identity/trust policy; verification failure fails
        // the connection with no plaintext fallback.
        6 => CellDisposition::Apply {
            owner: "ServiceTunnelSpec.use_ssl into server TLS target dial",
        },
        // 7 local_udp_host / 8 local_udp_port: streamr UDP endpoint.
        7 | 8 => CellDisposition::Apply {
            owner: "StreamrOptions.local_udp (loopback UDP endpoint)",
        },
        // 9 remote_udp_host: subscriber media-sink redirect; the socket
        // binds local_udp_host with an ephemeral port and sends media to
        // remote_udp_host:local_udp_port (loopback-confined).
        9 => CellDisposition::Apply {
            owner: "StreamrOptions.remote_sink (subscriber media redirect)",
        },
        // 10 tunnel_length / 11 tunnel_quantity: symmetric pool defaults.
        10 | 11 => CellDisposition::Apply {
            owner: "ServiceTunnelSpec.shaping into DestinationConfig projection",
        },
        // 12 tunnel_backup_quantity / 13 tunnel_variance: Plan 296
        // standby and sampler owners. Backup raises each direction's
        // effective pool target (standby held ready, usability on the
        // base target); variance samples each build's hop length
        // within the pool hop policy.
        12 | 13 => CellDisposition::Apply {
            owner: "TunnelShaping backup/variance into DestinationConfig standby and sampler",
        },
        // 14 inbound_length / 15 outbound_length / 16 inbound_quantity /
        // 17 outbound_quantity: per-direction pool projection; differing
        // per-direction lengths are rejected (single length_hops).
        14..=17 => CellDisposition::Apply {
            owner: "ServiceTunnelSpec.shaping into DestinationConfig projection",
        },
        // 18 profile / 19 interactive: interactive selects the small-window
        // StreamingConfig; streamr rides datagrams, not streaming windows.
        18 | 19 => match type_index {
            10 | 11 => CellDisposition::NotApplicable {
                reason: "streamr media path uses datagrams, not streaming windows",
            },
            _ => CellDisposition::Apply {
                owner: "StreamingConfig interactive selection (streaming windows)",
            },
        },
        // 20 start_on_load: persisted running intent.
        20 => CellDisposition::Apply {
            owner: "ControlDefinition.start_on_load (running intent)",
        },
        // 21 idle_timeout / 22 close_on_idle / 23 new_dest_on_idle /
        // 24 reduce_on_idle: manager idle sweep (milliseconds deadline;
        // flags require a timeout and vice versa).
        21..=24 => CellDisposition::Apply {
            owner: "ServiceTunnelManager idle sweep (last-activity deadline)",
        },
        // 25 max_streams: concurrent stream ceiling.
        25 => CellDisposition::Apply {
            owner: "ServiceTunnelSpec.max_connections (stream ceiling)",
        },
        // 26 proxy_username / 27 proxy_password: listener authentication;
        // both required together, verifiers stored (never plaintext).
        26 | 27 => CellDisposition::Apply {
            owner: "listener ProxyCredentials enforcement (407 / RFC 1929)",
        },
        // 28 access_list / 29 white_list / 30 black_list: inbound peer
        // destination-hash filter (access_list unions white_list).
        28..=30 => CellDisposition::Apply {
            owner: "server inbound peer allow/deny filter",
        },
        // 31 address_helper / 32 jump_list: HTTP presentation policy gates;
        // raw TCP servers have no HTTP layer to gate.
        31 | 32 => match type_index {
            8 | 9 => CellDisposition::Apply {
                owner: "HttpServerPolicy request gates (helper and jump classes)",
            },
            _ => CellDisposition::NotApplicable {
                reason: "raw TCP server has no HTTP presentation layer",
            },
        },
        // 33 unique_local_address: per-peer deterministic loopback source
        // bind on server-to-target dials.
        33 => CellDisposition::Apply {
            owner: "per-peer loopback source bind (server target dial)",
        },
        // 34 multihoming: Plan 296 target-selection owner. The flag
        // selects across the configured server target list per
        // connection (round-robin with sequential failover) instead
        // of the first target only; it requires two configured
        // targets. No session reply-info flag is introduced: the
        // single documented semantic is target selection.
        34 => CellDisposition::Apply {
            owner: "ServiceTunnelSpec.multihoming into server dial target selection",
        },
        // 35 reply_bundling: Plan 296 garlic reply-bundling owner.
        // The destination delivery path may carry multiple
        // same-remote application payloads as multiple data cloves
        // in one New Session Reply; unset keeps one payload per
        // garlic message.
        35 => CellDisposition::Apply {
            owner: "DestinationConfig reply_bundling into bundled-reply delivery",
        },
        // 36 streamr_subscribe_interval / 37 streamr_expiry /
        // 38 streamr_max_subscribers / 39 streamr_payload_limit.
        36..=39 => CellDisposition::Apply {
            owner: "StreamrOptions subscribe and fanout bounds",
        },
        // 40 sig_type: Plan 293 determination. i2pr destination identity
        // is Ed25519-only (ADR 0004); no key-generation owner exists for
        // any other SigType, and accepting the lone supported value would
        // be inert (it selects nothing). Any supplied value fails before
        // allocation; omission selects ordinary Ed25519 destinations.
        40 => CellDisposition::ExplicitIncompatibility {
            limitation: "dynamic destination SigType has no key-generation owner (Ed25519-only)",
        },
        // 41 encrypt_lease_set / 42 leaseset_password /
        // 43 leaseset_blinding_secret / 44 leaseset_client_auth: Plan 293
        // determination. No blinded/encrypted LeaseSet publication owner,
        // no type-5 framing owner, and no client-authorization verifier
        // exist; any supplied value fails before allocation, including
        // explicit disable (omit the field for ordinary publication).
        41..=44 => CellDisposition::ExplicitIncompatibility {
            limitation: "encrypted/blinded LeaseSet security and client authorization have no publication owner",
        },
        // 45 use_outproxy_plugin: Plan 293 determination. No safe
        // I2P-routed outproxy provider exists, and a provider would need
        // a general clearnet subsystem the guardrails forbid.
        45 => CellDisposition::ExplicitIncompatibility {
            limitation: "outproxy provider semantics have no I2P-routed provider",
        },
        // Unreachable for in-mask callers; never an apply.
        _ => CellDisposition::NotApplicable {
            reason: "unknown option index",
        },
    }
}

/// Full applicable matrix in (option, type) order.
pub const MATRIX: [MatrixCell; MATRIX_CELLS] = build_matrix();

/// Builds [`MATRIX`] from inventory masks and [`cell_disposition`].
const fn build_matrix() -> [MatrixCell; MATRIX_CELLS] {
    let mut out = [MatrixCell {
        type_index: 0,
        option_index: 0,
        disposition: CellDisposition::NotApplicable { reason: "unfilled" },
    }; MATRIX_CELLS];
    let mut at: usize = 0;
    let mut option_index: usize = 0;
    while option_index < TUNNEL_OPTIONS.len() {
        let mut type_index: usize = 0;
        while type_index < 12 {
            if TUNNEL_OPTIONS[option_index].applies_to(type_index) {
                out[at] = MatrixCell {
                    type_index,
                    option_index,
                    disposition: cell_disposition(option_index, type_index),
                };
                at += 1;
            }
            type_index += 1;
        }
        option_index += 1;
    }
    out
}

/// Counts cells with [`CellDisposition::Apply`].
const fn count_apply() -> usize {
    let mut n: usize = 0;
    let mut i: usize = 0;
    while i < MATRIX_CELLS {
        if matches!(MATRIX[i].disposition, CellDisposition::Apply { .. }) {
            n += 1;
        }
        i += 1;
    }
    n
}

/// Counts cells with [`CellDisposition::NotApplicable`].
const fn count_not_applicable() -> usize {
    let mut n: usize = 0;
    let mut i: usize = 0;
    while i < MATRIX_CELLS {
        if matches!(MATRIX[i].disposition, CellDisposition::NotApplicable { .. }) {
            n += 1;
        }
        i += 1;
    }
    n
}

/// Counts cells with [`CellDisposition::ExplicitIncompatibility`].
const fn count_incompatible() -> usize {
    let mut n: usize = 0;
    let mut i: usize = 0;
    while i < MATRIX_CELLS {
        if matches!(
            MATRIX[i].disposition,
            CellDisposition::ExplicitIncompatibility { .. }
        ) {
            n += 1;
        }
        i += 1;
    }
    n
}

/// Counts cells with [`CellDisposition::CorrectivePending`] for one plan.
const fn count_corrective(plan: u16) -> usize {
    let mut n: usize = 0;
    let mut i: usize = 0;
    while i < MATRIX_CELLS {
        if let CellDisposition::CorrectivePending {
            plan: cell_plan, ..
        } = MATRIX[i].disposition
            && cell_plan == plan
        {
            n += 1;
        }
        i += 1;
    }
    n
}

/// Apply cells: every one needs a named runtime owner (Plan 292 target).
pub const APPLY_CELLS: usize = count_apply();
/// Refined not-applicable cells (kind lacks the consuming layer).
pub const NOT_APPLICABLE_CELLS: usize = count_not_applicable();
/// Plan 293 explicit incompatibilities (signature, LeaseSet, outproxy
/// provider): applicable keys that fail before allocation with a named
/// limitation carried by Plan 295.
pub const INCOMPATIBLE_CELLS: usize = count_incompatible();
/// Plan 296 residual cells (pool shaping, multihoming, reply bundling).
/// Closed: every residual now has a named apply owner.
pub const CORRECTIVE_296_CELLS: usize = count_corrective(296);
/// Plan 297 residual cells (local TLS identity). Closed: the
/// server `use_ssl` cells now have a named apply owner.
pub const CORRECTIVE_297_CELLS: usize = count_corrective(297);

/// Finds the cell for a (type, option) pair; `None` outside the mask.
pub fn find_cell(type_index: usize, option_index: usize) -> Option<MatrixCell> {
    if type_index >= 12 || option_index >= TUNNEL_OPTIONS.len() {
        return None;
    }
    if !TUNNEL_OPTIONS[option_index].applies_to(type_index) {
        return None;
    }
    MATRIX
        .iter()
        .find(|cell| cell.type_index == type_index && cell.option_index == option_index)
        .copied()
}

/// Looks up the disposition for a wire key on a type index.
///
/// Returns `None` for unknown keys or pairs outside the inventory
/// mask. The daemon control boundary uses this to name the limitation
/// when rejecting incompatible or corrective-pending keys.
pub fn disposition_for(type_index: usize, option_name: &str) -> Option<CellDisposition> {
    let mut option_index: usize = 0;
    while option_index < TUNNEL_OPTIONS.len() {
        if TUNNEL_OPTIONS[option_index].name == option_name {
            return find_cell(type_index, option_index).map(|cell| cell.disposition);
        }
        option_index += 1;
    }
    None
}
