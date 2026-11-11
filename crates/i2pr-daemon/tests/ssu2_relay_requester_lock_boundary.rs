//! Plan 537's guard -- the relay requester must ask a peer for a relay
//! **without holding the router's NetDB lock**, and its refusal to ask must
//! never be **skipped silently**.
//!
//! Plan 531 built the requester role: the router that cannot reach a peer
//! directly asks the peer's published introducer to relay toward it. But the
//! composition root drove that outbound step **inside the bootstrap's mutex**
//! -- the router's NetDB lock -- for the whole step: the step scans the
//! **entire** validated store for candidates, reads up to sixteen candidates'
//! published introducer sets, **signs** the RelayRequest with the router's own
//! key, writes the transport's requester table and queues the request onto the
//! introducer's session.
//!
//! Worse, the last two of those take the **transport's own mutexes** while the
//! NetDB lock is held: the peer-relay service's requester table and the
//! runtime's state lock. The router's NetDB lock therefore **wraps** the
//! runtime's state lock, where the runtime's own order is sockets -> state.
//!
//! And the step was **gated on the daemon holding a `Bootstrap`** at all: a
//! router without one did not ask, did not refuse and did not count, so a
//! request that could have been made left no trace.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the outbound step is **not** gated on the bootstrap, and it holds no
//!    NetDB lock across the request;
//! 2. the production source's reads **release** the lock before they return,
//!    and the signature and the transport's writes happen after the reads
//!    return;
//! 3. the reads are **read-only** and mutate no store state;
//! 4. the missing store is a **typed** refusal, and it is **counted** under its
//!    own reason;
//! 5. the requester's machinery -- the preimage, the signature, the nonce, the
//!    session gate and the transport's requester table -- is **untouched**;
//! 6. the transport's requester table and its refusal order are **untouched**;
//! 7. **no** configuration key is added, **no** task, timer, listener or socket
//!    is added, and the outbound step rides the **existing** tick;
//! 8. Plan 531's lane and guard stay **byte-identical**, and Plan 531's guard
//!    still finds the strings it looks for.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, every rule has a
//! probe that removes the item it names so it **fails closed**, and every rule
//! reports a missing item rather than passing.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const DAEMON_REQUESTER_MODULE: &str = "crates/i2pr-daemon/src/relay_requester.rs";
const DAEMON_COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const PEER_RELAY_MODULE: &str = "crates/i2pr-runtime/src/ssu2_peer_relay.rs";
const TRANSPORT_RELAY_MODULE: &str = "crates/i2pr-transport-ssu2/src/relay.rs";
const PREDECESSOR_LANE: &str = "crates/i2pr-daemon/tests/ssu2_relay_requester.rs";
const PREDECESSOR_GUARD: &str = "crates/i2pr-daemon/tests/ssu2_relay_requester_boundary.rs";
const PLAN536_LANE: &str = "crates/i2pr-daemon/tests/ssu2_relay_target_lock.rs";
const PLAN536_GUARD: &str = "crates/i2pr-daemon/tests/ssu2_relay_target_lock_boundary.rs";

/// Plan 531's lane, byte for byte.
///
/// The predecessor's lane is **evidence**: this plan changes the requester's
/// material **source**, not its semantics, so the predecessor's evidence must
/// remain exactly as it was.
const PREDECESSOR_LANE_SHA256: &str =
    "c59714846086ba5a209002e15c0b63f8bc2ccd3a8437d6a5860ddf1cda4de64d";

/// Plan 531's guard, byte for byte.
const PREDECESSOR_GUARD_SHA256: &str =
    "36bd51fb125cf1c4ba5724f0cc9a4483a476a8c4fce9f9753abe45e177c56cb8";

/// Plan 536's lane, byte for byte.
///
/// Plan 537 touches the same composition root as Plan 536, so Plan 536's
/// evidence must remain exactly as it was too.
const PLAN536_LANE_SHA256: &str =
    "911b19d357d9eecaab10bcd5026f4098ae195a54c7a44ec389e80a01b72d4d43";

/// Plan 536's guard, byte for byte.
const PLAN536_GUARD_SHA256: &str =
    "ea2ec6f7a6874f896a6c05a7f8c32352fb6320f90b4b35a34fda925087fe594a";

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
/// A rule about a block must not be satisfied by a sibling block: the relay
/// target's drain keeps its own shape, and the requester's outbound step must
/// not inherit a gate. The **condition** is part of the statement, because a
/// gate is a condition and not a body.
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
    daemon_requester: String,
    daemon_composition: String,
    daemon_config: String,
    runtime: String,
    peer_relay: String,
    transport_relay: String,
    predecessor_lane: String,
    predecessor_guard: String,
    plan536_lane: String,
    plan536_guard: String,
}

impl Sources {
    /// Reads every source from the real tree.
    fn from_tree() -> Self {
        Self {
            daemon_requester: source_of(DAEMON_REQUESTER_MODULE),
            daemon_composition: source_of(DAEMON_COMPOSITION),
            daemon_config: source_of(DAEMON_CONFIG),
            runtime: source_of(RUNTIME_MODULE),
            peer_relay: source_of(PEER_RELAY_MODULE),
            transport_relay: source_of(TRANSPORT_RELAY_MODULE),
            predecessor_lane: source_of(PREDECESSOR_LANE),
            predecessor_guard: source_of(PREDECESSOR_GUARD),
            plan536_lane: source_of(PLAN536_LANE),
            plan536_guard: source_of(PLAN536_GUARD),
        }
    }
}

/// The production material source's three reads.
///
/// The source is the seam, and each of its reads is short and
/// self-contained. The guard reads each of them by its own signature, so a
/// renamed method is reported rather than silently skipped.
const PRODUCTION_READS: [&str; 3] = [
    "fn peer_pool(&self) -> Result<Vec<PeerId>, RelayRequesterRefusal>",
    "fn published_introducers(\n        &self,\n        peer: PeerId,\n    ) -> Result<PublishedIntroducerSet, RelayRequesterRefusal>",
    "fn introducer_endpoint(\n        &self,\n        introducer: [u8; 32],\n    ) -> Result<IntroducerEndpoint, RelayRequesterRefusal>",
];

// ---- 1. the outbound step is not gated on the bootstrap -----------------

fn rule_the_outbound_step_is_not_gated_on_the_bootstrap(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_composition);

    let Some(tick) = item_body(&code, "_ = relay_tick.tick() =>") else {
        return Some("the daemon's relay tick is absent".to_owned());
    };
    let Some(statement) =
        enclosing_statement(&tick, "crate::relay_requester::drive_relay_request(")
    else {
        return Some("the requester's outbound step is absent from the relay tick".to_owned());
    };
    let Some(open) = statement.find('{') else {
        return Some("the requester's outbound step is not inside a block".to_owned());
    };
    // The **condition** is the gate and the **body** is the work.
    let condition = squeeze(&statement[..open]);
    let body = squeeze(&statement[open..]);

    // The outbound step runs whenever the requester role exists, so its
    // condition names no bootstrap at all.
    if condition.contains("bootstrap") {
        return Some("the requester's outbound step must not be gated on the bootstrap".to_owned());
    }
    // The step holds **no** NetDB lock across the request.
    if body.contains("bootstrap.lock()") {
        return Some(
            "the requester's outbound step must not hold the router's NetDB lock".to_owned(),
        );
    }
    // The material is read through the production source.
    if !body.contains("crate::relay_requester::ValidatedStoreRequesterSource::new(") {
        return Some(
            "the requester's outbound step must read through the production material source"
                .to_owned(),
        );
    }
    // The source is built over the router's bootstrap **by reference**; the
    // mutex is taken inside the source, for one read at a time, and released
    // before the read returns.
    if !body.contains("bootstrap.as_ref()") {
        return Some("the production source must be built over the router's bootstrap".to_owned());
    }
    None
}

// ---- 2. the source releases the lock before the request -----------------

fn rule_the_source_releases_the_lock_before_the_request(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_requester);

    // The request is produced **through the source**, not through a store
    // the driver holds.
    let Some(drive) = item_text(&code, "pub fn drive_relay_request") else {
        return Some("the requester's drive step is absent".to_owned());
    };
    if !squeeze(&drive).contains("source: &dyn RelayRequesterMaterialSource") {
        return Some("the requester must read through the material source".to_owned());
    }

    let Some(body) = item_body(&code, "pub fn drive_relay_request") else {
        return Some("the requester's drive step is absent".to_owned());
    };
    let body = compact(&body);
    // The driver reads through the source, and it reads **before** it signs
    // and before it starts the request.
    let pool = body.find("source.peer_pool()");
    let published = body.find("source.published_introducers(");
    let endpoint = body.find("source.introducer_endpoint(");
    let build = body.find("owner.build_request(");
    let start = body.find("owner.start_request(");
    let (Some(pool), Some(published), Some(endpoint), Some(build), Some(start)) =
        (pool, published, endpoint, build, start)
    else {
        return Some("the requester must read the material before it signs".to_owned());
    };
    if !(pool < published && published < endpoint && endpoint < build && build < start) {
        return Some(
            "the material must be read before the request is signed and started".to_owned(),
        );
    }
    // The request path takes no lock of its own.
    if body.contains("bootstrap.lock()") || body.contains(".store()") {
        return Some("the request must take no lock of its own".to_owned());
    }

    // The production source: the guard is a **local**, and it does not
    // escape the read. A local guard is dropped when the function returns,
    // so the caller resumes with the lock free.
    let Some(implementation) = item_body(
        &code,
        "impl RelayRequesterMaterialSource for ValidatedStoreRequesterSource",
    ) else {
        return Some("the production material source is absent".to_owned());
    };
    for signature in PRODUCTION_READS {
        let Some(read) = item_body(&implementation, signature) else {
            return Some(format!(
                "the production source's read is absent: {signature}"
            ));
        };
        let read = compact(&read);
        if !read.contains("bootstrap.lock()") {
            return Some(format!(
                "the production source must read the store under the router's lock: {signature}"
            ));
        }
        let binds_guard = read.contains("letOk(guard)=bootstrap.lock()")
            || read.contains("letOk(mutguard)=bootstrap.lock()");
        if !binds_guard {
            return Some(format!(
                "the production source must bind the guard to a local: {signature}"
            ));
        }
        // The guard must not escape the read: nothing may name it after the
        // read it belongs to.
        let Some(last) = read.rfind("guard.store()") else {
            return Some(format!(
                "the production source must read through the guard: {signature}"
            ));
        };
        if read[last..].matches("guard").count() > 1 {
            return Some(format!(
                "the guard must not escape the material read: {signature}"
            ));
        }
    }
    None
}

// ---- 3. the reads are read-only -----------------------------------------

fn rule_the_reads_are_read_only(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_requester);

    let Some(implementation) = item_body(
        &code,
        "impl RelayRequesterMaterialSource for ValidatedStoreRequesterSource",
    ) else {
        return Some("the production material source is absent".to_owned());
    };
    let Some(store_implementation) = item_body(
        &code,
        "impl RelayRequesterMaterialSource for i2pr_netdb::RouterInfoStore",
    ) else {
        return Some("the store's material source is absent".to_owned());
    };

    for (label, body) in [
        ("the production source", implementation),
        ("the store's source", store_implementation),
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

    // The two free reads are the reads the tree already had: one record, one
    // strict address parse, two derived values, and no mutation.
    let Some(published) = item_body(&code, "pub fn read_published_introducers(") else {
        return Some("the published introducer read is absent".to_owned());
    };
    let published = compact(&published);
    if !published.contains("store.get(") {
        return Some("the published set must be read from the validated store".to_owned());
    }
    if !published.contains("i2pr_runtime::Ssu2RouterAddress::parse(") {
        return Some("the address must be parsed with the transport's strict parser".to_owned());
    }
    if published.matches("store.get(").count() != 1 {
        return Some("the published read must touch exactly one named peer".to_owned());
    }

    let Some(resolve) = item_body(&code, "pub fn read_introducer_endpoint(") else {
        return Some("the introducer resolution is absent".to_owned());
    };
    let resolve = compact(&resolve);
    if !resolve.contains("store.get(") {
        return Some("the introducer must be resolved from the validated store".to_owned());
    }
    if resolve.matches("store.get(").count() != 1 {
        return Some("the introducer read must touch exactly one named peer".to_owned());
    }
    None
}

// ---- 4. the missing store is a typed, counted refusal -------------------

fn rule_the_missing_store_is_a_typed_and_counted_refusal(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_requester);

    let Some(refusal) = item_body(&code, "pub enum RelayRequesterRefusal") else {
        return Some("the requester's refusal vocabulary is absent".to_owned());
    };
    if !refusal.contains("StoreUnavailable") {
        return Some("an absent store must have its own typed refusal".to_owned());
    }

    let Some(counters) = item_body(&code, "pub struct RelayRequesterCounters") else {
        return Some("the requester's counters are absent".to_owned());
    };
    if !squeeze(&counters).contains("pub refusals_store_unavailable: u64,") {
        return Some("the refusal must have its own counter".to_owned());
    }

    let Some(refuse) = item_body(&code, "pub fn refuse(&mut self") else {
        return Some("the requester's refusal recorder is absent".to_owned());
    };
    if !compact(&refuse).contains(
        "RelayRequesterRefusal::StoreUnavailable=>{self.counters.refusals_store_unavailable+=1;}",
    ) {
        return Some("the owner must count the refusal under its own counter".to_owned());
    }

    // The source refuses with that reason for **both** local conditions: an
    // absent bootstrap and a poisoned lock -- in every one of its reads.
    let Some(implementation) = item_body(
        &code,
        "impl RelayRequesterMaterialSource for ValidatedStoreRequesterSource",
    ) else {
        return Some("the production material source is absent".to_owned());
    };
    for signature in PRODUCTION_READS {
        let Some(read) = item_body(&implementation, signature) else {
            return Some(format!(
                "the production source's read is absent: {signature}"
            ));
        };
        let typed = read
            .matches("RelayRequesterRefusal::StoreUnavailable")
            .count();
        if typed < 2 {
            return Some(format!(
                "an absent store and a poisoned lock must both refuse with the typed reason: {signature}"
            ));
        }
    }
    None
}

// ---- 5. the requester's machinery is untouched --------------------------

fn rule_the_requesters_machinery_is_untouched(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_requester);

    // The request is signed over the specification's preimage, with the
    // router's own key.
    let Some(build) = item_body(&code, "pub fn build_request(") else {
        return Some("the request builder is absent".to_owned());
    };
    let build = compact(&build);
    if !build.contains("i2pr_runtime::relay_request_preimage(") {
        return Some("the request must be signed over the specification's preimage".to_owned());
    }
    if !build.contains(".sign_relay_request(&preimage)") {
        return Some("the request must be signed with the router's own key".to_owned());
    }

    // The nonce is fresh and nonzero, and the request rides a session.
    let Some(nonce) = item_body(&code, "pub fn draw_nonce") else {
        return Some("the nonce draw is absent".to_owned());
    };
    let nonce = compact(&nonce);
    if !nonce.contains("ifnonce==0||self.live.contains_key(&nonce)") {
        return Some("the nonce must be fresh and nonzero".to_owned());
    }

    let Some(drive) = item_body(&code, "pub fn drive_relay_request") else {
        return Some("the requester's drive step is absent".to_owned());
    };
    let drive = compact(&drive);
    if !drive.contains("owner.draw_nonce(rng)") {
        return Some("the request must draw a fresh nonce".to_owned());
    }
    if !drive.contains("live.contains(&candidate.introducer)") {
        return Some("the request must ride a session the router holds".to_owned());
    }

    // The runtime's seams are the runtime's own: the requester table and the
    // carriage.
    let runtime = code_view(&sources.runtime);
    let Some(start) = item_body(&runtime, "pub fn start_relay_request") else {
        return Some("the runtime's relay-request seam is absent".to_owned());
    };
    if !compact(&start).contains("self.shared.peer_relay.start_relay_request(") {
        return Some("the request must start through the runtime's own seam".to_owned());
    }
    let Some(carriage) = item_body(&runtime, "pub fn queue_relay_request") else {
        return Some("the runtime's carriage seam is absent".to_owned());
    };
    if !compact(&carriage)
        .contains("self.queue_relay_block(peer,|session|session.queue_relay_request(block))")
    {
        return Some("the request must be carried through the runtime's own seam".to_owned());
    }

    // The peer-relay service's requester table is behind its own mutex, and
    // the mutex is what the NetDB lock used to wrap.
    let peer_relay = code_view(&sources.peer_relay);
    let Some(table) = item_body(&peer_relay, "pub fn start_relay_request") else {
        return Some("the peer-relay service's requester seam is absent".to_owned());
    };
    if !compact(&table).contains("self.requester.lock()") {
        return Some("the transport's requester table must keep its own mutex".to_owned());
    }
    None
}

// ---- 6. the transport's requester table is untouched ---------------------

fn rule_the_transports_requester_table_is_untouched(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.transport_relay);

    let Some(start) = item_body(&code, "pub fn start(") else {
        return Some("the transport's requester table is absent".to_owned());
    };
    let start = squeeze(&start);
    // The transport's refusal order is the transport's own: a zero nonce or
    // tag, then the expiry sweep, then a duplicate correlation, then the
    // global ceiling, then the per-peer quota.
    let zero = start.find("nonce == 0 || tag == 0");
    let expire = start.find("self.expire_locked(now_ms)");
    let duplicate = start.find("self.entries.contains_key(&nonce)");
    let global = start.find("MAX_RELAY_REQUESTS_GLOBAL");
    let peer = start.find("MAX_RELAY_REQUESTS_PER_PEER");
    let insert = start.find("self.entries.insert(");
    let (Some(zero), Some(expire), Some(duplicate), Some(global), Some(peer), Some(insert)) =
        (zero, expire, duplicate, global, peer, insert)
    else {
        return Some("the transport's requester refusals are absent".to_owned());
    };
    if !(zero < expire
        && expire < duplicate
        && duplicate < global
        && global < peer
        && peer < insert)
    {
        return Some("the transport's requester refusal order is changed".to_owned());
    }
    if !code.contains("pub fn cancel(") {
        return Some("the transport's requester table must keep its cancellation".to_owned());
    }
    None
}

// ---- 7. no new surface, and the step rides the existing tick ------------

fn rule_no_new_surface_and_the_step_rides_the_existing_tick(sources: &Sources) -> Option<String> {
    // The configuration surface gains no key.
    let config = code_view(&sources.daemon_config);
    let Some(surface) = item_body(&config, "struct RawSsu2Config") else {
        return Some("the SSU2 configuration surface is absent".to_owned());
    };
    let requester_keys = surface
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.contains("relay_requester") && !line.starts_with("#["))
        .count();
    if requester_keys != 1 {
        return Some("the requester's configuration must be exactly one key".to_owned());
    }
    if surface.contains("HashMap") || surface.contains("#[serde(flatten)]") {
        return Some("the configuration must gain no free-form key".to_owned());
    }

    // The requester's owner adds no infrastructure.
    let owner = code_view(&sources.daemon_requester);
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

    // The outbound step rides the **existing** tick. Exactly one relay tick is
    // created and exactly one arm ticks it, so the requester gained no timer
    // of its own.
    let composition = code_view(&sources.daemon_composition);
    let ticks = composition.matches("relay_tick.tick()").count();
    if ticks != 1 {
        return Some(format!(
            "the requester must ride the one existing relay tick, so it adds no timer: {ticks}"
        ));
    }
    let Some(tick) = item_body(&composition, "_ = relay_tick.tick() =>") else {
        return Some("the daemon's relay tick is absent".to_owned());
    };
    if !squeeze(&tick).contains("crate::relay_requester::drive_relay_request(") {
        return Some("the existing tick must drive the requester's outbound step".to_owned());
    }
    // Exactly **one** call site: a second one would be a second timer.
    let drives = composition
        .matches("crate::relay_requester::drive_relay_request(")
        .count();
    if drives != 1 {
        return Some(format!(
            "the requester must have exactly one outbound call site, so no second timer exists: {drives}"
        ));
    }
    None
}

// ---- 8. the predecessor's evidence is untouched --------------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    // The predecessor's files are byte for byte what they were.
    for (label, source, expected) in [
        (
            "Plan 531's lane",
            &sources.predecessor_lane,
            PREDECESSOR_LANE_SHA256,
        ),
        (
            "Plan 531's guard",
            &sources.predecessor_guard,
            PREDECESSOR_GUARD_SHA256,
        ),
        (
            "Plan 536's lane",
            &sources.plan536_lane,
            PLAN536_LANE_SHA256,
        ),
        (
            "Plan 536's guard",
            &sources.plan536_guard,
            PLAN536_GUARD_SHA256,
        ),
    ] {
        let digest = sha256_hex(source.as_bytes());
        if digest != expected {
            return Some(format!(
                "{label} must stay byte-identical: {digest} instead of {expected}"
            ));
        }
    }

    // The predecessor's guard still looks for the strings it looked for.
    let predecessor = &sources.predecessor_guard;
    for needle in [
        "pub fn read_published_introducers(",
        "pub fn read_introducer_endpoint(",
        "pub fn build_request(",
        "pub fn on_relay_response(",
        "crate::relay_requester::drive_relay_request(",
        "crate::ssu2_relay_drain::drain_requester_events(",
        "crate::relay_requester::RelayRequesterOwner::new(",
        "ssu2_config.relay_requester",
        "store.get(",
        "relay_request_preimage(",
        "signers.signing_key(",
    ] {
        if !predecessor.contains(needle) {
            return Some(format!(
                "Plan 531's guard must still look for the string it looked for: {needle}"
            ));
        }
    }

    // And the tree still satisfies those strings, so the predecessor's guard
    // would still pass against it.
    let requester = code_view(&sources.daemon_requester);
    for (signature, needles) in [
        (
            "pub fn read_published_introducers(",
            vec!["store.get(", "i2pr_runtime::Ssu2RouterAddress::parse("],
        ),
        ("pub fn read_introducer_endpoint(", vec!["store.get("]),
        ("pub fn build_request(", vec!["relay_request_preimage("]),
        ("pub fn on_relay_response(", vec!["signers.signing_key("]),
    ] {
        let Some(body) = item_body(&requester, signature) else {
            return Some(format!(
                "Plan 531's guard must still find its item: {signature}"
            ));
        };
        let body = compact(&body);
        for needle in needles {
            if !body.contains(needle) {
                return Some(format!(
                    "Plan 531's guard must still find its string in {signature}: {needle}"
                ));
            }
        }
    }
    let composition = code_view(&sources.daemon_composition);
    let Some(tick) = item_body(&composition, "_ = relay_tick.tick() =>") else {
        return Some("the daemon's relay tick is absent".to_owned());
    };
    let tick = squeeze(&tick);
    for needle in [
        "crate::relay_requester::drive_relay_request(",
        "crate::ssu2_relay_drain::drain_requester_events(",
    ] {
        if !tick.contains(needle) {
            return Some(format!(
                "Plan 531's guard must still find its string in the relay tick: {needle}"
            ));
        }
    }
    if !composition.contains("crate::relay_requester::RelayRequesterOwner::new(") {
        return Some(
            "Plan 531's guard must still find the requester owner in the composition".to_owned(),
        );
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
// ----------------------------------------------------------------------

#[test]
fn the_outbound_step_is_not_gated_on_the_bootstrap() {
    let reason = rule_the_outbound_step_is_not_gated_on_the_bootstrap(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_source_releases_the_lock_before_the_request() {
    let reason = rule_the_source_releases_the_lock_before_the_request(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_reads_are_read_only() {
    let reason = rule_the_reads_are_read_only(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_missing_store_is_a_typed_and_counted_refusal() {
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_requesters_machinery_is_untouched() {
    let reason = rule_the_requesters_machinery_is_untouched(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_transports_requester_table_is_untouched() {
    let reason = rule_the_transports_requester_table_is_untouched(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn no_new_surface_and_the_step_rides_the_existing_tick() {
    let reason = rule_no_new_surface_and_the_step_rides_the_existing_tick(&Sources::from_tree());
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
fn the_guard_reports_an_outbound_step_gated_on_the_bootstrap() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "                                    if let Some(rng) = requester_rng.as_mut() {",
        "                                    if let Some(bootstrap) = bootstrap.as_ref()\n                                        && let Ok(guard) = bootstrap.lock()\n                                        && let Some(rng) = requester_rng.as_mut()\n                                    {",
    );
    let reason = rule_the_outbound_step_is_not_gated_on_the_bootstrap(&sources);
    assert!(
        reason.is_some(),
        "an outbound step gated on the bootstrap must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_outbound_step_that_holds_the_netdb_lock() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "                                        let decision = crate::relay_requester::drive_relay_request(",
        "                                        let _held = bootstrap.as_ref().and_then(|bootstrap| bootstrap.lock().ok());\n                                        let decision = crate::relay_requester::drive_relay_request(",
    );
    let reason = rule_the_outbound_step_is_not_gated_on_the_bootstrap(&sources);
    assert!(
        reason.is_some(),
        "an outbound step that holds the NetDB lock must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_source_that_holds_the_guard_past_the_read() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "        Ok(crate::relay_requester::read_introducer_endpoint(\n            guard.store(),\n            introducer,\n        ))",
        "        let value = crate::relay_requester::read_introducer_endpoint(\n            guard.store(),\n            introducer,\n        );\n        let _held = guard;\n        Ok(value)",
    );
    let reason = rule_the_source_releases_the_lock_before_the_request(&sources);
    assert!(
        reason.is_some(),
        "a guard held past the read must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_driver_that_reads_a_store_directly() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "    source: &dyn RelayRequesterMaterialSource,",
        "    store: &i2pr_netdb::RouterInfoStore,",
    );
    let reason = rule_the_source_releases_the_lock_before_the_request(&sources);
    assert!(
        reason.is_some(),
        "a driver that reads a store directly must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_read_that_mutates_the_netdb() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "        Ok(guard\n            .store()\n            .iter()",
        "        let _probe = guard.store_mut();\n        Ok(guard\n            .store()\n            .iter()",
    );
    let reason = rule_the_reads_are_read_only(&sources);
    assert!(
        reason.is_some(),
        "a read that mutates the NetDB must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_uncounted_store_refusal() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "            RelayRequesterRefusal::StoreUnavailable => {\n                self.counters.refusals_store_unavailable += 1;\n            }\n",
        "",
    );
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(reason.is_some(), "an uncounted refusal must fail the rule");
}

#[test]
fn the_guard_reports_a_store_refusal_with_the_wrong_reason() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources
        .daemon_requester
        .replace("StoreUnavailable", "NoEligibleTarget");
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(
        reason.is_some(),
        "a refusal under the wrong reason must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_request_signed_over_another_preimage() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources
        .daemon_requester
        .replace("relay_request_preimage(", "some_other_preimage(");
    let reason = rule_the_requesters_machinery_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a request signed over another preimage must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_removed_session_gate() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources
        .daemon_requester
        .replace("live.contains(&candidate.introducer)", "true");
    let reason = rule_the_requesters_machinery_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a request that rides no session must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_changed_transport_refusal_order() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources.transport_relay.replace(
        "        if self.entries.len() >= MAX_RELAY_REQUESTS_GLOBAL {\n            return Err(RelayError::TooManyRequests);\n        }\n",
        "",
    );
    let reason = rule_the_transports_requester_table_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a changed transport refusal order must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_second_configuration_key() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "    relay_requester: bool,",
        "    relay_requester: bool,\n    relay_requester_lock: bool,",
    );
    let reason = rule_no_new_surface_and_the_step_rides_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "a second configuration key must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_owns_a_socket() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "pub struct RelayRequesterOwner {",
        "use tokio::net::UdpSocket;\npub struct RelayRequesterOwner {",
    );
    let reason = rule_no_new_surface_and_the_step_rides_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "an owner that owns a socket must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_second_outbound_call_site() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "crate::relay_requester::drive_relay_request(",
        "crate::relay_requester::drive_relay_request(\n                                            requester,\n                                            &material_source,\n                                            &live,\n                                            rng,\n                                            local_endpoint,\n                                            now_ms,\n                                            now_secs,\n                                        ),\n                                        let _again = crate::relay_requester::drive_relay_request(",
    );
    let reason = rule_no_new_surface_and_the_step_rides_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "a second outbound call site must fail the rule"
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

#[test]
fn the_guard_reports_a_changed_plan536_guard() {
    let mut sources = Sources::from_tree();
    sources
        .plan536_guard
        .push_str("\n// a change to Plan 536's guard\n");
    let reason = rule_the_predecessors_evidence_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "a changed Plan 536 guard must fail the rule"
    );
}

// ----------------------------------------------------------------------
// The fail-closed probes. Each one removes the item the rule names, so
// the rule must report the absence rather than pass.
// ----------------------------------------------------------------------

#[test]
fn the_guard_fails_closed_when_the_production_source_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "impl RelayRequesterMaterialSource for ValidatedStoreRequesterSource",
        "impl RelayRequesterMaterialSource for RenamedRequesterSource",
    );
    let reason = rule_the_source_releases_the_lock_before_the_request(&sources);
    assert!(reason.is_some(), "an absent source must fail the rule");
    let reason = rule_the_reads_are_read_only(&sources);
    assert!(reason.is_some(), "an absent source must fail the rule");
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(reason.is_some(), "an absent source must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_a_production_read_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources
        .daemon_requester
        .replace("fn peer_pool(&self)", "fn peer_pool_v2(&self)");
    let reason = rule_the_source_releases_the_lock_before_the_request(&sources);
    assert!(
        reason.is_some(),
        "an absent production read must fail the rule"
    );
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(
        reason.is_some(),
        "an absent production read must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_driver_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "pub fn drive_relay_request",
        "pub fn drive_relay_request_v2",
    );
    let reason = rule_the_source_releases_the_lock_before_the_request(&sources);
    assert!(reason.is_some(), "an absent driver must fail the rule");
    let reason = rule_the_requesters_machinery_is_untouched(&sources);
    assert!(reason.is_some(), "an absent driver must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_outbound_step_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "crate::relay_requester::drive_relay_request(",
        "crate::relay_requester::unused_drive(",
    );
    let reason = rule_the_outbound_step_is_not_gated_on_the_bootstrap(&sources);
    assert!(
        reason.is_some(),
        "an absent outbound step must fail the rule"
    );
    let reason = rule_no_new_surface_and_the_step_rides_the_existing_tick(&sources);
    assert!(
        reason.is_some(),
        "an absent outbound step must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_refusal_vocabulary_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "pub enum RelayRequesterRefusal",
        "pub enum RelayRequesterRefusalRenamed",
    );
    let reason = rule_the_missing_store_is_a_typed_and_counted_refusal(&sources);
    assert!(
        reason.is_some(),
        "an absent refusal vocabulary must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_transport_table_is_absent() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources
        .transport_relay
        .replace("pub fn start(", "pub fn start_v2(");
    let reason = rule_the_transports_requester_table_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "an absent transport table must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_runtime_seams_are_absent() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "pub fn start_relay_request",
        "pub fn start_relay_request_v2",
    );
    let reason = rule_the_requesters_machinery_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "an absent runtime seam must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_configuration_surface_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources
        .daemon_config
        .replace("struct RawSsu2Config", "struct RawSsu2ConfigRenamed");
    let reason = rule_no_new_surface_and_the_step_rides_the_existing_tick(&sources);
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
