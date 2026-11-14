//! Plan 536's guard -- the relay target role must answer a relay
//! introduction **without holding the router's NetDB lock**, and its drain
//! must never be **skipped silently**.
//!
//! Plan 532 built the target role: the router that an introducer names
//! verifies the introduction through the transport's own target state
//! machine, signs the accept and stages the hole punch. But the composition
//! root drained that handoff **inside the bootstrap's mutex** -- the router's
//! NetDB lock -- for the whole pass, and the pass is bounded at sixteen
//! answers, each of which signs an accept. And the drain was gated on the
//! daemon **holding** a `Bootstrap` at all: a router without one did not
//! drain, did not refuse and did not count, so an introduction that arrived
//! and could not be answered left no trace.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the target's drain is **not** gated on the bootstrap, and it holds no
//!    NetDB lock across the answer;
//! 2. the production source's read **releases** the lock before it returns,
//!    and the answer is produced after the read returns;
//! 3. the read is **read-only** and mutates no store state;
//! 4. the missing store is a **typed** refusal, and it is **counted** under
//!    its own reason;
//! 5. the target role's machinery -- the transport seam, the signature, the
//!    hole punch and the dial profile -- is **untouched**;
//! 6. the transport's target table and its refusal order are **untouched**;
//! 7. **no** configuration key is added, **no** task, timer, listener or
//!    socket is added, and the drain rides the **existing** tick;
//! 8. Plan 532's lane and guard stay **byte-identical**, and Plan 532's
//!    guard still finds the strings it looks for.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, every rule has a
//! probe that removes the item it names so it **fails closed**, and every
//! rule reports a missing item rather than passing.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const DAEMON_TARGET_MODULE: &str = "crates/i2pr-daemon/src/relay_target.rs";
const DAEMON_DRAIN: &str = "crates/i2pr-daemon/src/ssu2_relay_drain.rs";
const DAEMON_COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const TRANSPORT_RELAY_MODULE: &str = "crates/i2pr-transport-ssu2/src/relay.rs";
const PREDECESSOR_LANE: &str = "crates/i2pr-daemon/tests/ssu2_relay_target.rs";
const PREDECESSOR_GUARD: &str = "crates/i2pr-daemon/tests/ssu2_relay_target_boundary.rs";

/// Plan 532's lane, byte for byte.
///
/// The predecessor's lane is **evidence**: this plan changes the target
/// role's material **source**, not its semantics, so the predecessor's
/// evidence must remain exactly as it was.
const PREDECESSOR_LANE_SHA256: &str =
    "e5e619b89ed6ddcdc858f66e0b65f5f617e5f5b523176b1ed070c42674efb54e";

/// Plan 532's guard, byte for byte.
const PREDECESSOR_GUARD_SHA256: &str =
    "4823f8f37e3c48111171281cc391962e2901eeeb35d581f8f9a7f34a4f949edc";

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

/// The SHA-256 of some bytes, in lower-case hexadecimal.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
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
fn item_text(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[start..close].to_owned())
}

/// Extracts the **statement** that contains `needle`: the condition that
/// opens the innermost enclosing block and the block's own body.
///
/// A rule about a block must not be satisfied by a sibling block: the
/// requester's outbound step keeps its own bootstrap gate, and the target's
/// drain must not inherit one. The **condition** is part of the statement,
/// because a gate is a condition and not a body.
fn enclosing_statement(code: &str, needle: &str) -> Option<String> {
    let at = code.find(needle)?;
    let mut opener = None;
    for (index, character) in code[..at].char_indices().rev() {
        if character == '{'
            && let Some((_, close)) = brace_balanced_block(code, index)
            && close > at
        {
            opener = Some(index);
            break;
        }
    }
    let opener = opener?;
    let (_, close) = brace_balanced_block(code, opener)?;
    let head = &code[..opener];
    // The statement starts after the previous `;`, `}` or `{`.
    let start = head
        .rfind([';', '}', '{'])
        .map(|index| index + 1)
        .unwrap_or(0);
    Some(code[start..close].to_owned())
}

/// The sources the rules read.
///
/// Every rule and every probe works on one of these, so a probe mutates
/// exactly one source and nothing else.
struct Sources {
    daemon_target: String,
    daemon_drain: String,
    daemon_composition: String,
    daemon_config: String,
    runtime: String,
    transport_relay: String,
    predecessor_lane: String,
    predecessor_guard: String,
}

impl Sources {
    /// Reads every source from the real tree.
    fn from_tree() -> Self {
        Self {
            daemon_target: source_of(DAEMON_TARGET_MODULE),
            daemon_drain: source_of(DAEMON_DRAIN),
            daemon_composition: source_of(DAEMON_COMPOSITION),
            daemon_config: source_of(DAEMON_CONFIG),
            runtime: source_of(RUNTIME_MODULE),
            transport_relay: source_of(TRANSPORT_RELAY_MODULE),
            predecessor_lane: source_of(PREDECESSOR_LANE),
            predecessor_guard: source_of(PREDECESSOR_GUARD),
        }
    }
}

// ---- 1. the target's drain is not gated on the bootstrap ----------------

fn rule_the_target_drain_is_not_gated_on_the_bootstrap(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_composition);

    let Some(tick) = item_body(&code, "_ = relay_tick.tick() =>") else {
        return Some("the daemon's relay tick is absent".to_owned());
    };
    let Some(statement) =
        enclosing_statement(&tick, "crate::ssu2_relay_drain::drain_target_events(")
    else {
        return Some("the target's drain is absent from the relay tick".to_owned());
    };
    let Some(open) = statement.find('{') else {
        return Some("the target's drain is not inside a block".to_owned());
    };
    // The **condition** is the gate and the **body** is the work.
    let condition = squeeze(&statement[..open]);
    let body = squeeze(&statement[open..]);

    // The drain runs whenever the target role exists, so its condition
    // names no bootstrap at all. The requester's outbound step keeps its
    // bootstrap gate -- asking a peer for a relay genuinely needs the
    // router's peer population -- and that gate is in another statement,
    // so it does not count against this rule.
    if condition.contains("bootstrap") {
        return Some("the target's drain must not be gated on the bootstrap".to_owned());
    }
    // The drain holds **no** NetDB lock across the answer.
    if body.contains("bootstrap.lock()") {
        return Some("the target's drain must not hold the router's NetDB lock".to_owned());
    }
    if !body.contains("crate::relay_target::ValidatedStoreMaterialSource::new(") {
        return Some(
            "the target's drain must read through the production material source".to_owned(),
        );
    }
    // The source is built over the router's bootstrap **by reference**; the
    // mutex is taken inside the source, for one named peer, and released
    // before the read returns.
    if !body.contains("bootstrap.as_ref()") {
        return Some("the production source must be built over the router's bootstrap".to_owned());
    }
    None
}

// ---- 2. the source releases the lock before the answer -----------------

fn rule_the_source_releases_the_lock_before_the_answer(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_target);

    // The answer is produced **through the source**, not through a store the
    // answer holds.
    let Some(drive) = item_text(&code, "pub fn drive_relay_target") else {
        return Some("the target's drive step is absent".to_owned());
    };
    if !squeeze(&drive).contains("source: &dyn RelayTargetMaterialSource") {
        return Some("the target must read through the material source".to_owned());
    }

    let Some(body) = item_body(&code, "pub fn drive_relay_target") else {
        return Some("the target's drive step is absent".to_owned());
    };
    let body = compact(&body);
    let read = body.find("read_requester_material(");
    let seam = body.find("owner.runtime.on_target_relay_intro(");
    let (Some(read), Some(seam)) = (read, seam) else {
        return Some("the target must read the requester and verify the introduction".to_owned());
    };
    if read >= seam {
        return Some("the material must be read before the introduction is verified".to_owned());
    }
    // The answer path takes no lock of its own.
    if body.contains("bootstrap.lock()") || body.contains(".store()") {
        return Some("the answer must take no lock of its own".to_owned());
    }

    // The production source: the guard is a **local**, and it does not
    // escape the read. A local guard is dropped when the function returns,
    // so the caller resumes with the lock free.
    let Some(source) = item_body(
        &code,
        "impl RelayTargetMaterialSource for ValidatedStoreMaterialSource",
    ) else {
        return Some("the production material source is absent".to_owned());
    };
    let source = compact(&source);
    if !source.contains("bootstrap.lock()") {
        return Some(
            "the production source must read the store under the router's lock".to_owned(),
        );
    }
    let binds_guard = source.contains("letOk(guard)=bootstrap.lock()")
        || source.contains("letOk(mutguard)=bootstrap.lock()");
    if !binds_guard {
        return Some("the production source must bind the guard to a local".to_owned());
    }
    let call = "read_requester_material(guard.store(),requester)";
    let Some(at) = source.find(call) else {
        return Some("the production source must read the requester's material".to_owned());
    };
    if source[at + call.len()..].contains("guard") {
        return Some("the guard must not escape the material read".to_owned());
    }
    None
}

// ---- 3. the read is read-only ------------------------------------------

fn rule_the_read_is_read_only(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_target);

    let Some(read) = item_body(&code, "pub fn read_requester_material") else {
        return Some("the requester's material read is absent".to_owned());
    };
    let Some(source) = item_body(
        &code,
        "impl RelayTargetMaterialSource for ValidatedStoreMaterialSource",
    ) else {
        return Some("the production material source is absent".to_owned());
    };

    for (label, body) in [
        ("the material read", read.clone()),
        ("the production source", source),
    ] {
        for forbidden in [
            "store_mut(",
            ".insert(",
            ".prune(",
            ".remove(",
            ".retain(",
            ".replace_local(",
            ".ingest_",
        ] {
            if body.contains(forbidden) {
                return Some(format!(
                    "{label} must not mutate the router's NetDB ({forbidden})"
                ));
            }
        }
    }

    // The read touches **one** named peer and derives two values from it.
    let read = compact(&read);
    if !read.contains("store.get(&key)") {
        return Some("the requester must be read from the validated store".to_owned());
    }
    if !read.contains("i2pr_runtime::Ssu2RouterAddress::parse(") {
        return Some("the address must be parsed with the transport's strict parser".to_owned());
    }
    // One record in, two values out: the read never asks for a second
    // peer.
    if read.matches("store.get(").count() != 1 {
        return Some("the read must touch exactly one named peer".to_owned());
    }
    None
}

// ---- 4. the missing store is a typed, counted refusal ------------------

fn rule_the_missing_store_is_a_typed_and_counted_refusal(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_target);

    let Some(refusal) = item_body(&code, "pub enum RelayTargetRefusal") else {
        return Some("the target's refusal vocabulary is absent".to_owned());
    };
    if !refusal.contains("StoreUnavailable") {
        return Some("an absent store must have its own typed refusal".to_owned());
    }

    let Some(reason) = item_body(&code, "pub const fn reason(self) -> &'static str") else {
        return Some("the refusal's reason keys are absent".to_owned());
    };
    if !squeeze(&reason).contains("Self::StoreUnavailable => \"store-unavailable\",") {
        return Some("the refusal must be counted under its own reason key".to_owned());
    }

    let Some(counters) = item_body(&code, "pub struct RelayTargetCounters") else {
        return Some("the target's counters are absent".to_owned());
    };
    if !squeeze(&counters).contains("pub refusals_store_unavailable: u64,") {
        return Some("the refusal must have its own counter".to_owned());
    }

    let Some(refuse) = item_body(&code, "pub fn refuse") else {
        return Some("the target's refusal recorder is absent".to_owned());
    };
    if !compact(&refuse).contains(
        "RelayTargetRefusal::StoreUnavailable=>{self.counters.refusals_store_unavailable+=1;}",
    ) {
        return Some("the owner must count the refusal under its own counter".to_owned());
    }

    // The source refuses with that reason for **both** local conditions:
    // an absent bootstrap and a poisoned lock.
    let Some(source) = item_body(
        &code,
        "impl RelayTargetMaterialSource for ValidatedStoreMaterialSource",
    ) else {
        return Some("the production material source is absent".to_owned());
    };
    let source = compact(&source);
    let typed = source
        .matches("RelayTargetRefusal::StoreUnavailable")
        .count();
    if typed < 2 {
        return Some(
            "an absent store and a poisoned lock must both refuse with the typed reason".to_owned(),
        );
    }
    None
}

// ---- 5. the target role's machinery is untouched ------------------------

fn rule_the_target_roles_machinery_is_untouched(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_target);

    let Some(drive) = item_body(&code, "pub fn drive_relay_target") else {
        return Some("the target's drive step is absent".to_owned());
    };
    let drive = compact(&drive);
    if !drive.contains("owner.runtime.on_target_relay_intro(") {
        return Some("the target must verify through the runtime's own seam".to_owned());
    }

    let Some(answer) = item_body(&code, "fn answer_introduction") else {
        return Some("the answer step is absent".to_owned());
    };
    let answer = compact(&answer);
    if !answer.contains("i2pr_runtime::relay_response_preimage(") {
        return Some("the accept must be signed over the specification's preimage".to_owned());
    }
    if !answer.contains("owner.signer.sign_relay_response(") {
        return Some("the accept must be signed with the router's own key".to_owned());
    }
    if !answer.contains("&introducer,") {
        return Some("the accept must be bound to the introducer".to_owned());
    }
    if !answer.contains("owner.runtime.stage_relay_hole_punch(") {
        return Some("the hole punch must be staged through the runtime's own seam".to_owned());
    }
    if !answer.contains("&material.intro_key,") {
        return Some("the hole punch must be sealed under the requester's key".to_owned());
    }

    // The dial profile is still the runtime's own: the answer cannot widen
    // the router's reach.
    let runtime = code_view(&sources.runtime);
    let Some(stage) = item_body(&runtime, "pub fn stage_relay_hole_punch") else {
        return Some("the runtime's outbound hole-punch seam is absent".to_owned());
    };
    if !squeeze(&stage).contains("is_dial_profile_address(") {
        return Some("the hole punch's destination must obey the dial profile".to_owned());
    }
    None
}

// ---- 6. the transport's target table is untouched ------------------------

fn rule_the_transports_target_table_is_untouched(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.transport_relay);

    let Some(intro) = item_body(&code, "pub fn on_intro") else {
        return Some("the transport's target verification is absent".to_owned());
    };
    let intro = squeeze(&intro);
    // The transport's refusal order is the transport's own: freshness, then
    // the forwarded signature, then the replay suppression, then the global
    // ceiling.
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
    None
}

// ---- 7. no new surface, and the drain rides the existing tick ------------

fn rule_no_new_surface_and_the_drain_rides_the_existing_tick(sources: &Sources) -> Option<String> {
    // The configuration surface gains no key.
    let config = code_view(&sources.daemon_config);
    let Some(surface) = item_body(&config, "struct RawSsu2Config") else {
        return Some("the SSU2 configuration surface is absent".to_owned());
    };
    let target_keys = surface
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.contains("relay_target") && !line.starts_with("#["))
        .count();
    if target_keys != 1 {
        return Some("the target's configuration must be exactly one key".to_owned());
    }
    if surface.contains("HashMap") || surface.contains("#[serde(flatten)]") {
        return Some("the configuration must gain no free-form key".to_owned());
    }

    // The target's owner adds no infrastructure.
    let owner = code_view(&sources.daemon_target);
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
        if owner.contains(forbidden) {
            return Some(format!("the owner must not add {forbidden}"));
        }
    }

    // The drain rides the **existing** tick. Exactly one relay tick is
    // created and exactly one arm ticks it, so the target gained no timer
    // of its own.
    let composition = code_view(&sources.daemon_composition);
    let ticks = composition.matches("relay_tick.tick()").count();
    if ticks != 1 {
        return Some(format!(
            "the target must ride the one existing relay tick, so it adds no timer: {ticks}"
        ));
    }
    let Some(tick) = item_body(&composition, "_ = relay_tick.tick() =>") else {
        return Some("the daemon's relay tick is absent".to_owned());
    };
    if !squeeze(&tick).contains("crate::ssu2_relay_drain::drain_target_events(") {
        return Some("the existing tick must drain the target's inbound handoff".to_owned());
    }
    // Exactly **one** drain call site for the target: a second one would be
    // a second timer.
    let drains = composition
        .matches("crate::ssu2_relay_drain::drain_target_events(")
        .count();
    if drains != 1 {
        return Some(format!(
            "the target must have exactly one drain call site, so no second timer exists: {drains}"
        ));
    }
    None
}

// ---- 8. the predecessor's evidence is untouched --------------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    // The predecessor's two files are byte for byte what they were.
    let lane = sha256_hex(sources.predecessor_lane.as_bytes());
    if lane != PREDECESSOR_LANE_SHA256 {
        return Some(format!(
            "Plan 532's lane must stay byte-identical: {lane} instead of {PREDECESSOR_LANE_SHA256}"
        ));
    }
    let guard = sha256_hex(sources.predecessor_guard.as_bytes());
    if guard != PREDECESSOR_GUARD_SHA256 {
        return Some(format!(
            "Plan 532's guard must stay byte-identical: {guard} instead of {PREDECESSOR_GUARD_SHA256}"
        ));
    }

    // The predecessor's guard still looks for the strings it looked for.
    let predecessor = &sources.predecessor_guard;
    for needle in [
        "read_requester_material(",
        "owner.runtime.on_target_relay_intro(",
        "store.get(",
        "i2pr_runtime::Ssu2RouterAddress::parse(",
        "crate::relay_target::drive_relay_target(",
        "for_in0..MAX_RELAY_DRAIN_PER_PASS{",
        "pub fn on_intro(",
        "check_relay_freshness(",
        "verify_relay_request(",
        "self.seen.contains(&key)",
        "MAX_RELAY_REQUESTS_GLOBAL",
        "crate::ssu2_relay_drain::drain_target_events(",
        "pub fn try_next_target_event(",
        "pub fn stage_relay_hole_punch(",
        "is_dial_profile_address(",
    ] {
        if !predecessor.contains(needle) {
            return Some(format!(
                "Plan 532's guard must still look for the string it looked for: {needle}"
            ));
        }
    }

    // And the tree still satisfies those strings, so the predecessor's
    // guard would still pass against it.
    let target = code_view(&sources.daemon_target);
    let Some(drive) = item_body(&target, "pub fn drive_relay_target") else {
        return Some("the target's drive step is absent".to_owned());
    };
    let drive = compact(&drive);
    for needle in [
        "read_requester_material(",
        "owner.runtime.on_target_relay_intro(",
    ] {
        if !drive.contains(needle) {
            return Some(format!(
                "Plan 532's guard must still find its string in the target's drive step: {needle}"
            ));
        }
    }
    let Some(read) = item_body(&target, "pub fn read_requester_material") else {
        return Some("the requester's material read is absent".to_owned());
    };
    let read = compact(&read);
    for needle in ["store.get(", "i2pr_runtime::Ssu2RouterAddress::parse("] {
        if !read.contains(needle) {
            return Some(format!(
                "Plan 532's guard must still find its string in the material read: {needle}"
            ));
        }
    }
    let drain = code_view(&sources.daemon_drain);
    let Some(drainer) = item_body(&drain, "pub fn drain_target_events") else {
        return Some("the target's drain is absent".to_owned());
    };
    let drainer = compact(&drainer);
    for needle in [
        "crate::relay_target::drive_relay_target(",
        "for_in0..MAX_RELAY_DRAIN_PER_PASS{",
    ] {
        if !drainer.contains(needle) {
            return Some(format!(
                "Plan 532's guard must still find its string in the target's drain: {needle}"
            ));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
// ----------------------------------------------------------------------

#[test]
fn the_target_drain_is_not_gated_on_the_bootstrap() {
    let reason = rule_the_target_drain_is_not_gated_on_the_bootstrap(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_source_releases_the_lock_before_the_answer() {
    let reason = rule_the_source_releases_the_lock_before_the_answer(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_read_is_read_only() {
    let reason = rule_the_read_is_read_only(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_missing_store_is_a_typed_and_counted_refusal() {
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_target_roles_machinery_is_untouched() {
    let reason = rule_the_target_roles_machinery_is_untouched(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_transports_target_table_is_untouched() {
    let reason = rule_the_transports_target_table_is_untouched(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn no_new_surface_and_the_drain_rides_the_existing_tick() {
    let reason = rule_no_new_surface_and_the_drain_rides_the_existing_tick(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_predecessors_evidence_is_untouched() {
    let reason = rule_the_predecessors_evidence_is_untouched(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

// ----------------------------------------------------------------------
// The inverted probes. Each one mutates an in-memory copy, so no file on
// disk is written.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_a_drain_gated_on_the_bootstrap() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "if let Some(target) = target.as_mut() {",
        "if let Some(target) = target.as_mut()\n                                    && let Some(bootstrap) = bootstrap.as_ref()\n                                    && let Ok(guard) = bootstrap.lock()\n                                {",
    );
    let reason = rule_the_target_drain_is_not_gated_on_the_bootstrap(&sources);
    assert!(
        reason.is_some(),
        "a drain gated on the bootstrap must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_drain_skipped_when_the_router_holds_no_store() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "if let Some(target) = target.as_mut() {",
        "if let Some(target) = target.as_mut()\n                                    && bootstrap.as_ref().is_some()\n                                {",
    );
    let reason = rule_the_target_drain_is_not_gated_on_the_bootstrap(&sources);
    assert!(
        reason.is_some(),
        "a drain skipped when the router holds no store must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_source_that_holds_the_guard_past_the_read() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "crate::relay_target::read_requester_material(guard.store(), requester)",
        "let material = crate::relay_target::read_requester_material(guard.store(), requester);\n        let _held = guard;\n        material",
    );
    let reason = rule_the_source_releases_the_lock_before_the_answer(&sources);
    assert!(
        reason.is_some(),
        "a guard held past the read must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_answer_that_reads_a_store_directly() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "source: &dyn RelayTargetMaterialSource,",
        "store: &i2pr_netdb::RouterInfoStore,",
    );
    let reason = rule_the_source_releases_the_lock_before_the_answer(&sources);
    assert!(
        reason.is_some(),
        "an answer that reads a store directly must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_answer_that_takes_the_netdb_lock() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "    owner.counters.intros_received += 1;",
        "    owner.counters.intros_received += 1;\n    let _guard = bootstrap.lock();",
    );
    let reason = rule_the_source_releases_the_lock_before_the_answer(&sources);
    assert!(
        reason.is_some(),
        "an answer that takes the NetDB lock must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_read_that_mutates_the_netdb() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "crate::relay_target::read_requester_material(guard.store(), requester)",
        "let material = crate::relay_target::read_requester_material(guard.store(), requester);\n        let _probe = guard.store_mut();\n        material",
    );
    let reason = rule_the_read_is_read_only(&sources);
    assert!(
        reason.is_some(),
        "a read that mutates the NetDB must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_uncounted_store_refusal() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "            RelayTargetRefusal::StoreUnavailable => {\n                self.counters.refusals_store_unavailable += 1;\n            }\n",
        "",
    );
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(reason.is_some(), "an uncounted refusal must fail the rule");
}

#[test]
fn the_guard_reports_a_store_refusal_with_the_wrong_reason() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "Err(RelayTargetRefusal::StoreUnavailable)",
        "Err(RelayTargetRefusal::RequesterUnknown)",
    );
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(
        reason.is_some(),
        "a refusal under the wrong reason must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_accept_signed_over_another_preimage() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources
        .daemon_target
        .replace("relay_response_preimage(", "some_other_preimage(");
    let reason = rule_the_target_roles_machinery_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "an accept signed over another preimage must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_changed_transport_refusal_order() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources.transport_relay.replace(
        "        let key = (block.nonce(), *block.alice_hash());\n        if self.seen.contains(&key) {\n            return Ok(false);\n        }\n",
        "        let key = (block.nonce(), *block.alice_hash());\n        if self.pending.len() >= MAX_RELAY_REQUESTS_GLOBAL {\n            return Err(RelayError::TooManyRequests);\n        }\n        if self.seen.contains(&key) {\n            return Ok(false);\n        }\n",
    );
    let reason = rule_the_transports_target_table_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a changed refusal order must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_second_configuration_key() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "    relay_target: bool,",
        "    relay_target: bool,\n    relay_target_lock: bool,",
    );
    let reason = rule_no_new_surface_and_the_drain_rides_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "a second configuration key must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_owns_a_socket() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub struct RelayTargetOwner {",
        "use tokio::net::UdpSocket;\npub struct RelayTargetOwner {",
    );
    let reason = rule_no_new_surface_and_the_drain_rides_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "an owner that owns a socket must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_changed_predecessor_lane() {
    let mut sources = Sources::from_tree();
    sources
        .predecessor_lane
        .push_str("\n// a change to the predecessor's lane\n");
    let reason = rule_the_predecessors_evidence_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a changed predecessor lane must fail the rule"
    );
}

// ----------------------------------------------------------------------
// The fail-closed probes. Each one removes the item the rule names, so
// the rule must report the absence rather than pass.
// ----------------------------------------------------------------------

#[test]
fn the_guard_fails_closed_when_the_target_drain_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "crate::ssu2_relay_drain::drain_target_events(",
        "crate::ssu2_relay_drain::unused_drain(",
    );
    let reason = rule_the_target_drain_is_not_gated_on_the_bootstrap(&sources);
    assert!(reason.is_some(), "an absent drain must fail the rule");
    let reason = rule_no_new_surface_and_the_drain_rides_the_existing_tick(&sources);
    assert!(reason.is_some(), "an absent drain must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_production_source_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "impl RelayTargetMaterialSource for ValidatedStoreMaterialSource",
        "impl RelayTargetMaterialSource for RenamedMaterialSource",
    );
    let reason = rule_the_source_releases_the_lock_before_the_answer(&sources);
    assert!(reason.is_some(), "an absent source must fail the rule");
    let reason = rule_the_read_is_read_only(&sources);
    assert!(reason.is_some(), "an absent source must fail the rule");
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(reason.is_some(), "an absent source must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_material_read_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub fn read_requester_material",
        "pub fn read_requester_material_v2",
    );
    let reason = rule_the_read_is_read_only(&sources);
    assert!(
        reason.is_some(),
        "an absent material read must fail the rule"
    );
    let reason = rule_the_predecessors_evidence_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "an absent material read must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_refusal_vocabulary_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "pub enum RelayTargetRefusal",
        "pub enum RelayTargetRefusalRenamed",
    );
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(
        reason.is_some(),
        "an absent refusal vocabulary must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_answer_step_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources
        .daemon_target
        .replace("fn answer_introduction", "fn answer_introduction_v2");
    let reason = rule_the_target_roles_machinery_is_untouched(&sources);
    assert!(reason.is_some(), "an absent answer step must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_transport_verification_is_absent() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources
        .transport_relay
        .replace("pub fn on_intro(", "pub fn on_intro_v2(");
    let reason = rule_the_transports_target_table_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "an absent transport verification must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_configuration_surface_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources
        .daemon_config
        .replace("struct RawSsu2Config", "struct RawSsu2ConfigRenamed");
    let reason = rule_no_new_surface_and_the_drain_rides_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "an absent configuration surface must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_predecessors_guard_is_changed() {
    let mut sources = Sources::from_tree();
    sources
        .predecessor_guard
        .push_str("\n// a change to the predecessor's guard\n");
    let reason = rule_the_predecessors_evidence_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a changed predecessor guard must fail the rule"
    );
}
