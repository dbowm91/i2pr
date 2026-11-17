//! Plan 532's guard -- the router must answer a relay introduction when it
//! is the target.
//!
//! The tree implemented the SSU2 relay protocol's introducer side and the
//! requester side, but the target side was unwired: the transport's target
//! state machine was complete, the runtime held the table and exposed the
//! seam, and nothing in production called the seam. A router that was the
//! peer an introducer named received the Relay Intro on the
//! **introducer's** handoff, refused it as not its role, and never
//! answered.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the owner lives in the **daemon** and the mechanics stay in the
//!    **runtime**;
//! 2. the owner holds no task, no timer, no listener and no socket;
//! 3. the answer is produced only from **validated** material;
//! 4. the opt-in is **off** by default and refused without the SSU2
//!    service;
//! 5. the transport's relay state machines and the reachability model are
//!    untouched;
//! 6. the introduction is routed to the **target's** handoff and the
//!    introducer's handoff no longer carries it;
//! 7. the hole punch is staged and never sessioned, and its destination
//!    obeys the dial profile;
//! 8. the configuration surface gains no key beside the one opt-in;
//! 9. the owner is constructed and driven on the daemon's **existing**
//!    tick, so the target role is never dead code again.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, every rule has a
//! probe that removes the item it names so it **fails closed**, and every
//! rule reports a missing item rather than passing.

#![forbid(unsafe_code)]

const TRANSPORT_REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
const TRANSPORT_RELAY_MODULE: &str = "crates/i2pr-transport-ssu2/src/relay.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const RUNTIME_PEER_RELAY_MODULE: &str = "crates/i2pr-runtime/src/ssu2_peer_relay.rs";
const DAEMON_TARGET_MODULE: &str = "crates/i2pr-daemon/src/relay_target.rs";
const DAEMON_REQUESTER_MODULE: &str = "crates/i2pr-daemon/src/relay_requester.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const DAEMON_ROUTER_I2NP: &str = "crates/i2pr-daemon/src/router_i2np.rs";
const DAEMON_COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_DRAIN: &str = "crates/i2pr-daemon/src/ssu2_relay_drain.rs";

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Reads one source file from the real tree.
fn source_of(relative: &str) -> String {
    let path = workspace_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    })
}

/// Removes comments from Rust source text.
///
/// The scanner must not report a doc comment that merely *names* a
/// construct, so the text is reduced to its code before the tokens are
/// matched.
fn strip_comments(source: &str) -> String {
    let bytes: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut index = 0usize;
    let mut in_string = false;
    let mut in_char = false;
    let mut in_line_comment = false;
    let mut block_depth = 0usize;
    while index < bytes.len() {
        let current = bytes[index];
        let next = bytes.get(index + 1).copied();
        if in_line_comment {
            if current == '\n' {
                in_line_comment = false;
                out.push('\n');
            }
            index += 1;
            continue;
        }
        if block_depth > 0 {
            if current == '/' && next == Some('*') {
                block_depth += 1;
                index += 2;
                continue;
            }
            if current == '*' && next == Some('/') {
                block_depth -= 1;
                index += 2;
                continue;
            }
            if current == '\n' {
                out.push('\n');
            }
            index += 1;
            continue;
        }
        if in_string {
            out.push(current);
            if current == '\\' {
                if let Some(escaped) = next {
                    out.push(escaped);
                }
                index += 2;
                continue;
            }
            if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if in_char {
            out.push(current);
            if current == '\\' {
                if let Some(escaped) = next {
                    out.push(escaped);
                }
                index += 2;
                continue;
            }
            if current == '\'' {
                in_char = false;
            }
            index += 1;
            continue;
        }
        if current == '/' && next == Some('/') {
            in_line_comment = true;
            index += 2;
            continue;
        }
        if current == '/' && next == Some('*') {
            block_depth = 1;
            index += 2;
            continue;
        }
        if current == '"' {
            in_string = true;
            out.push(current);
            index += 1;
            continue;
        }
        if current == '\'' {
            in_char = true;
            out.push(current);
            index += 1;
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

/// Removes `#[cfg(test)]` blocks from source text.
///
/// A test-only helper must never satisfy a production rule.
fn strip_cfg_test_blocks(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut index = 0usize;
    while index < code.len() {
        let rest = &code[index..];
        if rest.starts_with("#[cfg(test)]") {
            let after = index + "#[cfg(test)]".len();
            let Some(open) = code[after..].find('{') else {
                break;
            };
            let open = after + open;
            let Some((_, close)) = brace_balanced_block(code, open) else {
                break;
            };
            out.push(' ');
            index = close;
            continue;
        }
        let character = rest.chars().next().expect("character");
        out.push(character);
        index += character.len_utf8();
    }
    out
}

/// The code view of a source file: no comments and no test-only blocks.
fn code_view(source: &str) -> String {
    strip_cfg_test_blocks(&strip_comments(source))
}

/// Collapses every run of whitespace into a single space.
///
/// A rule that reads a call site must not depend on where the formatter
/// happened to break the line.
fn squeeze(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_space = false;
    for character in source.chars() {
        if character.is_whitespace() {
            in_space = true;
            continue;
        }
        if in_space {
            out.push(' ');
            in_space = false;
        }
        out.push(character);
    }
    if in_space {
        out.push(' ');
    }
    out
}

/// Removes every whitespace character.
///
/// A rule that reads a call site must not depend on where the formatter
/// happened to break the line, and a broken call is `self`, `.runtime`
/// and `.start_relay_request(` on three lines.
fn compact(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Returns the offset of a **definition** whose text is `needle`.
fn find_definition(code: &str, needle: &str) -> Option<usize> {
    let mut offset = 0usize;
    while let Some(position) = code[offset..].find(needle) {
        let start = offset + position;
        let after = start + needle.len();
        offset = after;
        let continues_identifier = code[after..]
            .chars()
            .next()
            .is_some_and(|character| character.is_alphanumeric() || character == '_');
        if continues_identifier {
            continue;
        }
        return Some(start);
    }
    None
}

/// Returns the byte range of the brace-balanced block that opens at or after
/// `start`, as `(open, close)`; `close` is one past the closing brace.
fn brace_balanced_block(code: &str, start: usize) -> Option<(usize, usize)> {
    let bytes = code.as_bytes();
    let open = code[start..].find('{')? + start;
    let mut depth = 0usize;
    let mut cursor = open;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => {
                cursor = skip_string_literal(bytes, cursor);
                continue;
            }
            b'\'' => {
                if let Some(end) = skip_char_literal(bytes, cursor) {
                    cursor = end;
                    continue;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, cursor + 1));
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

/// Returns the offset just past the string literal that starts at `start`.
fn skip_string_literal(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            b'"' => return cursor + 1,
            _ => cursor += 1,
        }
    }
    cursor
}

/// Returns the offset just past the character literal that starts at
/// `start`, or `None` when the quote opens a lifetime instead.
fn skip_char_literal(bytes: &[u8], start: usize) -> Option<usize> {
    let is_char_literal = match bytes.get(start + 1) {
        Some(b'\\') => true,
        Some(_) => bytes.get(start + 2) == Some(&b'\''),
        None => false,
    };
    if !is_char_literal {
        return None;
    }
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            b'\'' => return Some(cursor + 1),
            _ => cursor += 1,
        }
    }
    Some(cursor)
}

/// Extracts the body of the item whose **definition** is anchored at
/// `signature`.
fn item_body(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (open, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

/// Extracts the whole item whose **definition** is anchored at
/// `signature`: the signature and the body together.
///
/// [`item_body`] returns the body alone. A rule about what an item
/// *takes* -- its parameters, its fields -- needs the whole item.
fn item_text(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[start..close].to_owned())
}

/// Extracts the body of the **match arm** whose text is `needle`.
///
/// An arm is not a definition, so it has its own scanner: the arm's block
/// is the first brace-balanced block at or after the needle.
fn arm_body(code: &str, needle: &str) -> Option<String> {
    let start = code.find(needle)?;
    let (open, close) = brace_balanced_block(code, start + needle.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

/// The sources the rules read.
///
/// Every rule and every probe works on one of these, so a probe mutates
/// exactly one source and nothing else.
struct Sources {
    daemon_target: String,
    daemon_requester: String,
    daemon_config: String,
    daemon_router_i2np: String,
    daemon_composition: String,
    daemon_drain: String,
    runtime: String,
    runtime_peer_relay: String,
    transport_relay: String,
    transport_reachability: String,
}

impl Sources {
    /// Reads every source from the real tree.
    fn from_tree() -> Self {
        Self {
            daemon_target: source_of(DAEMON_TARGET_MODULE),
            daemon_requester: source_of(DAEMON_REQUESTER_MODULE),
            daemon_config: source_of(DAEMON_CONFIG),
            daemon_router_i2np: source_of(DAEMON_ROUTER_I2NP),
            daemon_composition: source_of(DAEMON_COMPOSITION),
            daemon_drain: source_of(DAEMON_DRAIN),
            runtime: source_of(RUNTIME_MODULE),
            runtime_peer_relay: source_of(RUNTIME_PEER_RELAY_MODULE),
            transport_relay: source_of(TRANSPORT_RELAY_MODULE),
            transport_reachability: source_of(TRANSPORT_REACHABILITY_MODULE),
        }
    }
}

// ---- 1. the owner is the daemon's, the mechanics are the runtime's ----

fn rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(
    sources: &Sources,
) -> Option<String> {
    let owner_code = code_view(&sources.daemon_target);

    let Some(structure) = item_body(&owner_code, "pub struct RelayTargetOwner") else {
        return Some("the daemon's target owner is absent".to_owned());
    };
    if !squeeze(&structure).contains("runtime: i2pr_runtime::Ssu2RuntimeService") {
        return Some("the owner must hold the runtime that owns the sessions".to_owned());
    }
    if owner_code.contains("pub struct RelayTarget {") {
        return Some("the daemon must not re-implement the transport's target table".to_owned());
    }
    // The daemon keeps no introduction table of its own.
    if owner_code.contains("pending_intros") || owner_code.contains("BTreeMap<u32,") {
        return Some("the daemon must keep no second introduction table".to_owned());
    }

    let Some(drive) = item_body(&owner_code, "pub fn drive_relay_target(") else {
        return Some("the owner's drive step is absent".to_owned());
    };
    let drive = compact(&drive);
    if !drive.contains("owner.runtime.on_target_relay_intro(") {
        return Some(
            "the owner must verify the introduction through the runtime's seam".to_owned(),
        );
    }
    if !drive.contains("read_requester_material(") {
        return Some("the owner must read the requester from the validated store".to_owned());
    }

    let runtime_code = code_view(&sources.runtime);
    let Some(seam) = item_body(&runtime_code, "pub fn on_target_relay_intro(") else {
        return Some("the runtime's target seam is absent".to_owned());
    };
    if !compact(&seam).contains("self.shared.peer_relay.on_relay_intro_with_signer(") {
        return Some("the runtime's seam must drive the peer-relay service".to_owned());
    }

    let peer_relay_code = code_view(&sources.runtime_peer_relay);
    let Some(service) = item_body(&peer_relay_code, "pub fn on_relay_intro_with_signer(") else {
        return Some("the peer-relay service's target seam is absent".to_owned());
    };
    if !compact(&service).contains(".on_intro(") {
        return Some("the peer-relay service must drive the transport's target table".to_owned());
    }
    None
}

// ---- 2. the owner adds no infrastructure ------------------------------

fn rule_the_owner_adds_no_infrastructure(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_target);
    // The rule names the owner: an empty module is not a passing owner.
    if !code.contains("pub struct RelayTargetOwner {") {
        return Some("the daemon's target owner is absent".to_owned());
    }
    for forbidden in [
        "tokio::",
        "UdpSocket",
        "TcpListener",
        "TcpStream",
        "spawn(",
        "sleep(",
        "interval(",
        "std::thread",
    ] {
        if code.contains(forbidden) {
            return Some(format!("the owner must not add {forbidden}"));
        }
    }
    // A router that answers an introduction is reachable **through the
    // relay**, not directly, so the answer gains no corroborating class
    // and writes no reachability signal of its own.
    if code.contains("ReachabilitySignal::") {
        return Some("the owner must not write a reachability signal".to_owned());
    }
    // The tracker needle is **split on purpose**. The m12 boundary
    // checker scans every crate for the tracker call outside the
    // runtime's own owner, and this guard's own source must not read as
    // a call site. The rule the guard enforces is unchanged: a daemon
    // that reaches into the tracker is still reported.
    let tracker_call = concat!(".reachability", ".record(");
    if code.contains(tracker_call) {
        return Some("the owner must not touch the reachability tracker".to_owned());
    }
    None
}

// ---- 3. the answer is produced only from validated material ----------

fn rule_the_answer_is_produced_only_from_validated_material(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_target);

    let Some(read) = item_body(&code, "pub fn read_requester_material(") else {
        return Some("the requester's material read is absent".to_owned());
    };
    let read = compact(&read);
    if !read.contains("store.get(") {
        return Some("the requester must be read from the validated store".to_owned());
    }
    if !read.contains("i2pr_runtime::Ssu2RouterAddress::parse(") {
        return Some("the address must be parsed with the transport's strict parser".to_owned());
    }
    // The daemon never parses unvalidated bytes.
    for forbidden in [
        "RouterInfo::decode",
        "decode_to_router_info",
        "from_bytes(&bytes)",
    ] {
        if code.contains(forbidden) {
            return Some(format!(
                "the owner must not parse unvalidated bytes ({forbidden})"
            ));
        }
    }

    let Some(answer) = item_body(&code, "fn answer_introduction(") else {
        return Some("the answer step is absent".to_owned());
    };
    let answer = compact(&answer);
    if !answer.contains("relay_response_preimage(") {
        return Some("the accept must be signed over the specification's preimage".to_owned());
    }
    if !answer.contains("owner.signer.sign_relay_response(") {
        return Some("the accept must be signed with the router's own key".to_owned());
    }
    // The preimage names the **introducer** that forwarded the
    // introduction, because the introducer forwards the accept verbatim.
    if !answer.contains("&introducer,") {
        return Some("the accept must be bound to the introducer".to_owned());
    }
    // The hole punch is sealed under the **requester's** introduction key.
    if !answer.contains("&material.intro_key,") {
        return Some("the hole punch must be sealed under the requester's key".to_owned());
    }

    // One implementation of the router's own signing: the target's accept
    // is signed through the same seam the requester uses, and the seam
    // signs with the identity bundle.
    let Some(seam) = item_text(&code, "impl RelayTargetSigner for") else {
        return Some("the target's signer seam is absent".to_owned());
    };
    if !compact(&seam).contains("crate::relay_requester::LocalIdentitySigner") {
        return Some("the target must share the router's own signing seam".to_owned());
    }
    let requester_code = code_view(&sources.daemon_requester);
    let Some(sign) = item_body(&requester_code, "pub fn sign(") else {
        return Some("the router's own signing seam is absent".to_owned());
    };
    if !compact(&sign).contains("self.bundle.signing_key().sign(") {
        return Some("the router's own signing seam must use the identity bundle".to_owned());
    }
    None
}

// ---- 4. the opt-in defaults off and needs the service ----------------

fn rule_the_opt_in_defaults_off_and_needs_the_service(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_config);

    let Some(default) = item_body(&code, "const fn default_ssu2_relay_target() -> bool") else {
        return Some("the target's default is absent".to_owned());
    };
    if !squeeze(&default).contains("false") {
        return Some("the target must be off by default".to_owned());
    }
    if !code.contains("\"ssu2.relay_target\"") {
        return Some("the refusal must name the target's key".to_owned());
    }
    if !squeeze(&code).contains("if raw.relay_target && !raw.enabled") {
        return Some("the target must be refused without the SSU2 service".to_owned());
    }

    let mapping = code_view(&sources.daemon_router_i2np);
    let Some(runtime_config) = item_body(&mapping, "pub fn ssu2_runtime_config_from(") else {
        return Some("the daemon's SSU2 runtime mapping is absent".to_owned());
    };
    if !squeeze(&runtime_config).contains("relay_target: config.relay_target,") {
        return Some("the daemon must map the target's opt-in into the runtime".to_owned());
    }
    None
}

// ---- 5. the transport's state machines and the model are untouched ---

fn rule_the_transport_is_untouched(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.transport_relay);
    let Some(intro) = item_body(&code, "pub fn on_intro(") else {
        return Some("the transport's target verification is absent".to_owned());
    };
    let intro = squeeze(&intro);
    // The transport's refusal order is the transport's own: freshness,
    // then the forwarded signature, then the replay suppression, then the
    // global ceiling.
    let freshness = intro.find("check_relay_freshness(");
    let signature = intro.find("verify_relay_request(");
    let replay = intro.find("self.seen.contains(&key)");
    let global = intro.find("MAX_RELAY_REQUESTS_GLOBAL");
    let (Some(freshness), Some(signature), Some(replay), Some(global)) =
        (freshness, signature, replay, global)
    else {
        return Some("the transport's target refusals are absent".to_owned());
    };
    if !(freshness < signature && signature < replay && replay < global) {
        return Some("the transport's target refusal order is changed".to_owned());
    }
    if !code.contains("pub fn build_hole_punch(") || !code.contains("pub fn parse_hole_punch(") {
        return Some("the transport's hole-punch codec is absent".to_owned());
    }

    let model = code_view(&sources.transport_reachability);
    if !model.contains("pub const MIN_CORROBORATION_FLOOR: usize = 2;") {
        return Some("the corroboration floor is moved".to_owned());
    }
    if !model.contains("Self::RelayFirewalledSignal { .. } => false,") {
        return Some("the relay-firewalled signal must never support reachability".to_owned());
    }
    if !model.contains("Self::RelayFirewalledSignal { .. } => true,") {
        return Some("the relay-firewalled signal must contradict reachability".to_owned());
    }
    if !model.contains("Self::RelayFirewalledSignal { .. } => 4,") {
        return Some("the relay-firewalled signal's corroboration class is moved".to_owned());
    }
    None
}

// ---- 6. the introduction reaches the target's handoff -----------------

fn rule_the_introduction_reaches_the_targets_handoff(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.runtime);

    let Some(arm) = arm_body(&code, "SessionEvent::RelayIntro(block) =>") else {
        return Some("the runtime's relay-introduction route is absent".to_owned());
    };
    let arm = squeeze(&arm);
    if !arm.contains("if self.shared.config.relay_target {") {
        return Some("the route must be gated on the target's opt-in".to_owned());
    }
    if !arm.contains("target.push(Ssu2InboundTarget {") {
        return Some("the route must deliver the introduction to the target's handoff".to_owned());
    }
    if !arm.contains("relay.push(Ssu2InboundRelay {") {
        return Some("the introducer's handoff must stop carrying the introduction".to_owned());
    }
    if !arm.contains("event: Ssu2RelayEvent::Intro(block),") {
        return Some("the introducer's handoff must stop carrying the introduction".to_owned());
    }

    // The datagram path must carry the target's own bounded batch.
    let Some(datagram) = item_text(&code, "fn handle_active_datagram(") else {
        return Some("the runtime's datagram path is absent".to_owned());
    };
    if !squeeze(&datagram).contains("target: &mut Vec<Ssu2InboundTarget>,") {
        return Some("the datagram path must carry the target's own batch".to_owned());
    }

    // The target's handoff is a **pull** seam with the relay handoff's
    // own ceiling.
    let Some(handle) = item_body(&code, "pub fn try_next_target_event(") else {
        return Some("the target's pull seam is absent".to_owned());
    };
    if !compact(&handle).contains("self.target.try_recv().ok()") {
        return Some("the target's pull seam must read the target's own handoff".to_owned());
    }
    None
}

// ---- 7. the hole punch is staged, never sessioned ---------------------

fn rule_the_hole_punch_is_staged_and_never_sessioned(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.runtime);

    let Some(stage) = item_body(&code, "pub fn stage_relay_hole_punch(") else {
        return Some("the runtime's outbound hole-punch seam is absent".to_owned());
    };
    let stage = squeeze(&stage);
    if !stage.contains("is_dial_profile_address(") {
        return Some("the hole punch's destination must obey the dial profile".to_owned());
    }
    if !stage.contains("Ssu2RelayTargetError::DestinationRefused") {
        return Some("a destination outside the profile must be refused".to_owned());
    }
    if !stage.contains("build_hole_punch(") {
        return Some("the hole punch must be built with the transport's own codec".to_owned());
    }
    if !stage.contains("stage_class_locked(") {
        return Some("the hole punch must go through the bounded staging queue".to_owned());
    }
    if !stage.contains("BandwidthTrafficClass::Control") {
        return Some("the hole punch must be staged as control traffic".to_owned());
    }
    // The hole punch travels **out of session**: it never rides one.
    for forbidden in [
        "queue_relay_response(",
        "queue_relay_intro(",
        "queue_relay_request(",
        "session_",
    ] {
        if stage.contains(forbidden) {
            return Some(format!(
                "the hole punch must never ride a session ({forbidden})"
            ));
        }
    }

    // The profile is the union of the two dial profiles the runtime
    // offers, so the target role cannot widen the router's reach.
    let Some(profile) = item_body(&code, "fn is_dial_profile_address") else {
        return Some("the dial profile check is absent".to_owned());
    };
    let profile = squeeze(&profile);
    if !profile.contains("ip.is_loopback() || ip.is_private()") {
        return Some(
            "the dial profile must accept the loopback and the private IPv4 range".to_owned(),
        );
    }
    if !profile.contains("::V6(ip) => ip.is_loopback()") {
        return Some("the dial profile must accept the loopback IPv6 range".to_owned());
    }
    None
}

// ---- 8. no configuration key beside the one opt-in --------------------

fn rule_no_other_configuration_key(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_config);
    let Some(surface) = item_body(&code, "struct RawSsu2Config") else {
        return Some("the SSU2 configuration surface is absent".to_owned());
    };
    if surface.contains("HashMap") || surface.contains("#[serde(flatten)]") {
        return Some("the configuration must gain no free-form key".to_owned());
    }
    let target_keys = surface
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.contains("relay_target") && !line.starts_with("#["))
        .count();
    if target_keys != 1 {
        return Some("the target's configuration must be exactly one key".to_owned());
    }

    let owner_code = code_view(&sources.daemon_target);
    for forbidden in ["std::env", "config::"] {
        if owner_code.contains(forbidden) {
            return Some(format!("the owner must not read {forbidden}"));
        }
    }
    None
}

// ---- 9. the owner is constructed and driven on the existing tick -------

fn rule_the_owner_is_wired_to_the_existing_tick(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_composition);
    if !code.contains("crate::relay_target::RelayTargetOwner::new(") {
        return Some("the daemon must construct the target owner".to_owned());
    }
    if !code.contains("ssu2_config.relay_target") {
        return Some("the owner must be gated on the operator's opt-in".to_owned());
    }

    let Some(tick) = item_body(&code, "_ = relay_tick.tick() =>") else {
        return Some("the daemon's relay tick is absent".to_owned());
    };
    let tick = squeeze(&tick);
    if !tick.contains("crate::ssu2_relay_drain::drain_target_events(") {
        return Some("the existing tick must drain the target's inbound handoff".to_owned());
    }

    let drain_code = code_view(&sources.daemon_drain);
    let Some(drainer) = item_body(&drain_code, "pub fn drain_target_events(") else {
        return Some("the target's drain is absent".to_owned());
    };
    let drainer = compact(&drainer);
    if !drainer.contains("crate::relay_target::drive_relay_target(") {
        return Some("the drain must hand the introduction to the target owner".to_owned());
    }
    if !drainer.contains("for_in0..MAX_RELAY_DRAIN_PER_PASS{") {
        return Some("the drain must stay bounded".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
// ----------------------------------------------------------------------

#[test]
fn the_owner_is_the_daemons_and_the_mechanics_the_runtimes() {
    let reason =
        rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_owner_adds_no_infrastructure() {
    let reason = rule_the_owner_adds_no_infrastructure(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_answer_is_produced_only_from_validated_material() {
    let reason = rule_the_answer_is_produced_only_from_validated_material(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_opt_in_defaults_off_and_needs_the_service() {
    let reason = rule_the_opt_in_defaults_off_and_needs_the_service(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_transports_state_machines_and_model_are_untouched() {
    let reason = rule_the_transport_is_untouched(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_introduction_reaches_the_targets_handoff() {
    let reason = rule_the_introduction_reaches_the_targets_handoff(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_hole_punch_is_staged_and_never_sessioned() {
    let reason = rule_the_hole_punch_is_staged_and_never_sessioned(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn no_configuration_key_besides_the_one_opt_in() {
    let reason = rule_no_other_configuration_key(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_owner_is_wired_to_the_existing_tick() {
    let reason = rule_the_owner_is_wired_to_the_existing_tick(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

// ----------------------------------------------------------------------
// The inverted probes. Each one mutates an in-memory copy, so no file on
// disk is written.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_an_owner_that_does_not_use_the_runtime_seam() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "owner.runtime.on_target_relay_intro(",
        "owner.local_target.on_intro(",
    );
    let reason = rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&sources);
    assert!(
        reason.is_some(),
        "an owner that re-implements the verification must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_daemon_side_target_table() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub struct RelayTargetOwner {",
        "pub struct RelayTarget { entries: BTreeMap<u32, u32> }\npub struct RelayTargetOwner {",
    );
    let reason = rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&sources);
    assert!(
        reason.is_some(),
        "a daemon-side target table must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_runtime_that_does_not_drive_the_peer_relay_service() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "self.shared.peer_relay.on_relay_intro_with_signer(",
        "self.shared.own_table.on_intro_with_signer(",
    );
    let reason = rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&sources);
    assert!(
        reason.is_some(),
        "a runtime that skips the peer-relay service must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_owns_a_socket() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub struct RelayTargetOwner {",
        "use tokio::net::UdpSocket;\npub struct RelayTargetOwner {",
    );
    let reason = rule_the_owner_adds_no_infrastructure(&sources);
    assert!(
        reason.is_some(),
        "an owner that owns a socket must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_writes_a_reachability_signal() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub fn refuse(",
        "pub fn refuse_probe(&self) { let _probe = i2pr_transport::ReachabilitySignal::RelayFirewalledSignal { family: i2pr_transport::AddressFamily::Ipv4 }; }\n    pub fn refuse(",
    );
    let reason = rule_the_owner_adds_no_infrastructure(&sources);
    assert!(
        reason.is_some(),
        "an owner that writes a reachability signal must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_spawns_a_task() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources
        .daemon_target
        .replace("pub fn new(", "fn spawn() {}\n    pub fn new(");
    let reason = rule_the_owner_adds_no_infrastructure(&sources);
    assert!(
        reason.is_some(),
        "an owner that spawns a task must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_read_that_leaves_the_validated_store() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources
        .daemon_target
        .replace("store.get(&key)", "self.anything.get(&key)");
    let reason = rule_the_answer_is_produced_only_from_validated_material(&sources);
    assert!(
        reason.is_some(),
        "a read that leaves the validated store must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_parses_unvalidated_bytes() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub fn read_requester_material(",
        "pub fn read_requester_material(\n        store: &i2pr_netdb::RouterInfoStore,\n        requester: [u8; 32],\n    ) -> Result<RequesterMaterial, RelayTargetRefusal> {\n        let _probe = i2pr_proto::RouterInfo::decode(&bytes);\n        unreachable!()\n    }\n\n    pub fn read_requester_material_unused(",
    );
    let reason = rule_the_answer_is_produced_only_from_validated_material(&sources);
    assert!(
        reason.is_some(),
        "an owner that parses unvalidated bytes must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_accept_that_is_not_signed_over_the_preimage() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources
        .daemon_target
        .replace("relay_response_preimage(", "some_other_preimage(");
    let reason = rule_the_answer_is_produced_only_from_validated_material(&sources);
    assert!(
        reason.is_some(),
        "an accept signed over another preimage must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_accept_that_is_not_bound_to_the_introducer() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources
        .daemon_target
        .replace("&introducer,\n        nonce,", "&target,\n        nonce,");
    let reason = rule_the_answer_is_produced_only_from_validated_material(&sources);
    assert!(
        reason.is_some(),
        "an accept that is not bound to the introducer must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_hole_punch_sealed_under_the_wrong_key() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources
        .daemon_target
        .replace("&material.intro_key,", "&own_intro_key,");
    let reason = rule_the_answer_is_produced_only_from_validated_material(&sources);
    assert!(
        reason.is_some(),
        "a hole punch sealed under the wrong key must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_target_that_signs_with_its_own_seam() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "impl RelayTargetSigner for crate::relay_requester::LocalIdentitySigner {",
        "impl RelayTargetSigner for crate::relay_target::TargetOwnSigner {",
    );
    let reason = rule_the_answer_is_produced_only_from_validated_material(&sources);
    assert!(
        reason.is_some(),
        "a target with its own signing seam must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_opt_in_that_defaults_on() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "const fn default_ssu2_relay_target() -> bool {\n    false\n}",
        "const fn default_ssu2_relay_target() -> bool {\n    true\n}",
    );
    let reason = rule_the_opt_in_defaults_off_and_needs_the_service(&sources);
    assert!(
        reason.is_some(),
        "an opt-in that defaults on must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_opt_in_that_runs_without_the_service() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "if raw.relay_target && !raw.enabled {",
        "if false && !raw.enabled {",
    );
    let reason = rule_the_opt_in_defaults_off_and_needs_the_service(&sources);
    assert!(
        reason.is_some(),
        "an opt-in that runs without the service must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_unmapped_opt_in() {
    let mut sources = Sources::from_tree();
    sources.daemon_router_i2np = sources
        .daemon_router_i2np
        .replace("relay_target: config.relay_target,", "relay_target: false,");
    let reason = rule_the_opt_in_defaults_off_and_needs_the_service(&sources);
    assert!(reason.is_some(), "an unmapped opt-in must fail the rule");
}

#[test]
fn the_guard_reports_a_changed_transport_refusal_order() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources.transport_relay.replace(
        "        let key = (block.nonce(), *block.alice_hash());\n        if self.seen.contains(&key) {\n            return Ok(false);\n        }\n",
        "        let key = (block.nonce(), *block.alice_hash());\n        if self.pending.len() >= MAX_RELAY_REQUESTS_GLOBAL {\n            return Err(RelayError::TooManyRequests);\n        }\n        if self.seen.contains(&key) {\n            return Ok(false);\n        }\n",
    );
    let reason = rule_the_transport_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a changed refusal order must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_moved_corroboration_class() {
    let mut sources = Sources::from_tree();
    sources.transport_reachability = sources.transport_reachability.replace(
        "Self::RelayFirewalledSignal { .. } => 4,",
        "Self::RelayFirewalledSignal { .. } => 3,",
    );
    let reason = rule_the_transport_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a moved corroboration class must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_route_that_keeps_the_introduction_on_the_introducer() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources
        .runtime
        .replace("if self.shared.config.relay_target {", "if false {");
    let reason = rule_the_introduction_reaches_the_targets_handoff(&sources);
    assert!(
        reason.is_some(),
        "a route that never reaches the target must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_route_that_keeps_both_handoffs() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "                        target.push(Ssu2InboundTarget {\n                            link_id,\n                            peer,\n                            block,\n                        });\n",
        "",
    );
    let reason = rule_the_introduction_reaches_the_targets_handoff(&sources);
    assert!(
        reason.is_some(),
        "a route that never fills the target's handoff must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_hole_punch_that_ignores_the_dial_profile() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "        if !is_dial_profile_address(&destination) {\n            return Err(Ssu2RelayTargetError::DestinationRefused);\n        }\n",
        "",
    );
    let reason = rule_the_hole_punch_is_staged_and_never_sessioned(&sources);
    assert!(
        reason.is_some(),
        "a hole punch outside the dial profile must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_hole_punch_that_rides_a_session() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "        let destination = requester_endpoint.socket_addr();\n",
        "        let _probe = self.queue_relay_response(peer, response);\n        let destination = requester_endpoint.socket_addr();\n",
    );
    let reason = rule_the_hole_punch_is_staged_and_never_sessioned(&sources);
    assert!(
        reason.is_some(),
        "a hole punch that rides a session must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_second_configuration_key() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "    relay_target: bool,",
        "    relay_target: bool,\n    relay_target_extra: bool,",
    );
    let reason = rule_no_other_configuration_key(&sources);
    assert!(
        reason.is_some(),
        "a second configuration key must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_is_never_constructed() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "crate::relay_target::RelayTargetOwner::new(",
        "crate::relay_target::UnusedOwner::new(",
    );
    let reason = rule_the_owner_is_wired_to_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "an owner that is never constructed must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_is_never_driven() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "crate::ssu2_relay_drain::drain_target_events(",
        "crate::ssu2_relay_drain::unused_drain(",
    );
    let reason = rule_the_owner_is_wired_to_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "an owner that is never driven must fail the rule"
    );
}

// ----------------------------------------------------------------------
// The fail-closed probes. Each one removes the item the rule names, so
// the rule must report the absence rather than pass.
// ----------------------------------------------------------------------

#[test]
fn the_guard_fails_closed_when_the_owner_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub struct RelayTargetOwner",
        "pub struct RelayTargetOwnerRenamed",
    );
    let reason = rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&sources);
    assert!(reason.is_some(), "an absent owner must fail the rule");
    let reason = rule_the_owner_adds_no_infrastructure(&sources);
    assert!(reason.is_some(), "an absent owner must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_material_read_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub fn read_requester_material(",
        "pub fn read_requester_material_v2(",
    );
    let reason = rule_the_answer_is_produced_only_from_validated_material(&sources);
    assert!(
        reason.is_some(),
        "an absent material read must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_opt_in_default_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "const fn default_ssu2_relay_target() -> bool",
        "const fn default_ssu2_relay_target_v2() -> bool",
    );
    let reason = rule_the_opt_in_defaults_off_and_needs_the_service(&sources);
    assert!(
        reason.is_some(),
        "an absent opt-in default must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_transport_verification_is_absent() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources
        .transport_relay
        .replace("pub fn on_intro(", "pub fn on_intro_v2(");
    let reason = rule_the_transport_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "an absent transport verification must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_route_is_absent() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "SessionEvent::RelayIntro(block) =>",
        "SessionEvent::RelayIntroUnused(block) =>",
    );
    let reason = rule_the_introduction_reaches_the_targets_handoff(&sources);
    assert!(reason.is_some(), "an absent route must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_outbound_seam_is_absent() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "pub fn stage_relay_hole_punch(",
        "pub fn stage_relay_hole_punch_v2(",
    );
    let reason = rule_the_hole_punch_is_staged_and_never_sessioned(&sources);
    assert!(
        reason.is_some(),
        "an absent outbound seam must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_configuration_surface_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources
        .daemon_config
        .replace("struct RawSsu2Config", "struct RawSsu2ConfigRenamed");
    let reason = rule_no_other_configuration_key(&sources);
    assert!(
        reason.is_some(),
        "an absent configuration surface must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_drain_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_drain = sources.daemon_drain.replace(
        "pub fn drain_target_events(",
        "pub fn drain_target_events_v2(",
    );
    let reason = rule_the_owner_is_wired_to_the_existing_tick(&sources);
    assert!(reason.is_some(), "an absent drain must fail the rule");
}
