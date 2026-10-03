//! Plan 292: exact tunnel-type x option disposition matrix.
//!
//! The frozen inventory ([`crate::tunnel_options::TUNNEL_OPTIONS`]) states
//! which of the 46 wire keys applies to which of the 12 tunnel types (336
//! applicable cells). This module records exactly one Plan 292 disposition
//! per applicable cell:
//!
//! - [`CellDisposition::Apply`]: a named runtime/persistence owner consumes
//!   the key for the kind. The owner string names the consuming
//!   struct/field/sweep, never a storage mirror.
//! - [`CellDisposition::NotApplicable`]: the kind has no consuming layer
//!   for the key even though the Proposal mask group covers it. The
//!   refinement rule is uniform: within an applicable mask group, a kind
//!   without the consuming layer (TCP endpoint, HTTP presentation,
//!   streaming stack, UDP media path) is not-applicable; every other cell
//!   must be apply, blocked, or corrective. Raw TCP servers have no HTTP
//!   presentation layer; stream kinds have no TCP endpoints; Streamr kinds
//!   ride the datagram path, not the streaming stack.
//! - [`CellDisposition::BlockedPrimitive`]: applicable, but the required
//!   primitive is genuinely absent and owned by Plan 293 (signature,
//!   LeaseSet security, outproxy provider). No other plan may own a
//!   blocked cell.
//! - [`CellDisposition::CorrectivePending`]: applicable, but the required
//!   primitive needs a new corrective plan (296: pool shaping residuals,
//!   multihoming, reply bundling; 297: local TLS identity). The plan
//!   number is the owning corrective.
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

/// Plan 292 disposition of one applicable (type, option) cell.
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
    /// Applicable but the primitive is absent; owned by Plan 293.
    BlockedPrimitive {
        /// Owning plan (always 293).
        plan: u16,
        /// Missing primitive identity.
        primitive: &'static str,
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
    /// Plan 292 disposition.
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
        // 6 use_ssl: server TLS needs an explicit local trust policy first.
        6 => CellDisposition::CorrectivePending {
            plan: 297,
            reason: "use_ssl local TLS identity and trust policy",
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
        // 12 tunnel_backup_quantity / 13 tunnel_variance: no pool primitive.
        12 | 13 => CellDisposition::CorrectivePending {
            plan: 296,
            reason: "pool backup-quantity and length-variance semantics",
        },
        // 14 inbound_length / 15 outbound_length / 16 inbound_quantity /
        // 17 outbound_quantity: per-direction pool projection; differing
        // per-direction lengths are rejected (single length_hops).
        14 | 15 | 16 | 17 => CellDisposition::Apply {
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
        21 | 22 | 23 | 24 => CellDisposition::Apply {
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
        28 | 29 | 30 => CellDisposition::Apply {
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
        // 34 multihoming: session reply-info primitive absent.
        34 => CellDisposition::CorrectivePending {
            plan: 296,
            reason: "multihoming reply-info and target-selection primitive",
        },
        // 35 reply_bundling: no garlic reply-bundling primitive.
        35 => CellDisposition::CorrectivePending {
            plan: 296,
            reason: "garlic reply-bundling primitive",
        },
        // 36 streamr_subscribe_interval / 37 streamr_expiry /
        // 38 streamr_max_subscribers / 39 streamr_payload_limit.
        36 | 37 | 38 | 39 => CellDisposition::Apply {
            owner: "StreamrOptions subscribe and fanout bounds",
        },
        // 40 sig_type: algorithm-agile destination identity (Plan 293).
        40 => CellDisposition::BlockedPrimitive {
            plan: 293,
            primitive: "dynamic destination SigType",
        },
        // 41 encrypt_lease_set / 42 leaseset_password /
        // 43 leaseset_blinding_secret / 44 leaseset_client_auth.
        41 | 42 | 43 | 44 => CellDisposition::BlockedPrimitive {
            plan: 293,
            primitive: "encrypted/blinded LeaseSet security and client authorization",
        },
        // 45 use_outproxy_plugin: no safe I2P-routed provider exists.
        45 => CellDisposition::BlockedPrimitive {
            plan: 293,
            primitive: "outproxy provider semantics",
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
        disposition: CellDisposition::NotApplicable {
            reason: "unfilled",
        },
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

/// Counts cells with the given disposition shape.
const fn count_cells(
    is_apply: bool,
    is_not_applicable: bool,
    blocked_plan: u16,
    corrective_plan: u16,
) -> usize {
    let mut n: usize = 0;
    let mut i: usize = 0;
    while i < MATRIX_CELLS {
        let matches = match MATRIX[i].disposition {
            CellDisposition::Apply { .. } => is_apply,
            CellDisposition::NotApplicable { .. } => is_not_applicable,
            CellDisposition::BlockedPrimitive { plan, .. } => plan == blocked_plan,
            CellDisposition::CorrectivePending { plan, .. } => plan == corrective_plan,
        };
        if matches {
            n += 1;
        }
        i += 1;
    }
    n
}

/// Apply cells: every one needs a named runtime owner (Plan 292 target).
pub const APPLY_CELLS: usize = count_cells(true, false, 0, 0);
/// Refined not-applicable cells (kind lacks the consuming layer).
pub const NOT_APPLICABLE_CELLS: usize = count_cells(false, true, 0, 0);
/// Plan 293 residual cells (signature, LeaseSet, outproxy provider).
pub const BLOCKED_293_CELLS: usize = count_cells(false, false, 293, 0);
/// Plan 296 residual cells (pool shaping, multihoming, reply bundling).
pub const CORRECTIVE_296_CELLS: usize = count_cells(false, false, 0, 296);
/// Plan 297 residual cells (local TLS identity).
pub const CORRECTIVE_297_CELLS: usize = count_cells(false, false, 0, 297);

/// Finds the cell for a (type, option) pair; `None` outside the mask.
pub fn find_cell(type_index: usize, option_index: usize) -> Option<MatrixCell> {
    if type_index >= 12 || option_index >= TUNNEL_OPTIONS.len() {
        return None;
    }
    if !TUNNEL_OPTIONS[option_index].applies_to(type_index) {
        return None;
    }
    for cell in MATRIX {
        if cell.type_index == type_index && cell.option_index == option_index {
            return Some(cell);
        }
    }
    None
}

/// Looks up the disposition for a wire key on a type index.
///
/// Returns `None` for unknown keys or pairs outside the inventory
/// mask. The daemon control boundary uses this to name the owning
/// plan when rejecting blocked or corrective-pending keys.
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
