//! Plan 535's guard -- the router must keep its own bound address counted as
//! reachability evidence.
//!
//! Plan 530 made the router record its **own** bound sockets as the
//! explicit-bind corroboration class. That class is a **standing** fact -- the
//! socket is bound and stays bound -- but the transport's reachability model
//! holds every signal as a *timed observation*, so the class **lapses** after
//! the observation lifetime and nothing re-asserts it. Two of the three
//! classes `Reachable` needs are refreshed by their peers; the explicit bind
//! is refreshed by nobody, so after one observation lifetime the router's
//! corroboration count can never exceed two again and `Reachable` is
//! unreachable for ever.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the re-assertion is gated on the **same** opt-in the recorder is, so a
//!    runtime that did not ask for explicit-bind corroboration re-asserts
//!    nothing, and the recorder captures the standing fact the re-assertion
//!    re-asserts;
//! 2. the observation lifetime is read back from the **tracker's own policy**
//!    and is never restated in the runtime;
//! 3. the re-assertion rides the runtime's **existing** maintenance pass and
//!    adds **no** timer;
//! 4. the rule re-asserts **strictly before** the record can lapse;
//! 5. the re-assertion touches the router's **own** tracker: it writes no peer
//!    profile, claims no corroborating class for any peer, and records no
//!    reachability signal about anything but this router's own socket;
//! 6. the transport's reachability **model** -- its vocabulary, its
//!    corroboration classes, both floors, its observation lifetime and its
//!    tracker's public surface -- is untouched;
//! 7. **no** configuration key is added, in the daemon's surface or in the
//!    daemon's mapping;
//! 8. every re-assertion is **counted**, and the counter carries counts only.
//!
//! Every rule has an inverted probe that fires it against an **in-memory copy**
//! of the source, so no file on disk is written, and every rule **fails
//! closed** when the item it names is absent.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TRANSPORT_REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const DAEMON_ROUTER_I2NP: &str = "crates/i2pr-daemon/src/router_i2np.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const PREDECESSOR_GUARD: &str =
    "crates/i2pr-daemon/tests/ssu2_explicit_bind_corroboration_boundary.rs";

/// The transport's tracker's public surface, exactly as Plan 530 left it.
///
/// Plan 535 adds **no** method to the tracker: the re-assertion *records* into
/// the tracker through the method the transport already offers.
const EXPECTED_TRACKER_METHODS: [&str; 8] = [
    "new",
    "state",
    "policy",
    "signal_count",
    "record",
    "poll_expiry",
    "snapshot",
    "as_transport_observation",
];

/// The gate the recorder and the re-assertion must **share**.
const OPT_IN_GATE: &str = "!self.shared.config.explicit_bind_corroboration";

/// The gate's opening line, used by the inverted probes.
const OPT_IN_GATE_LINE: &str = "if !self.shared.config.explicit_bind_corroboration {";

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

/// Extracts the item's **whole** text: its signature and its body.
fn item_text(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[start..close].to_owned())
}

/// Extracts the argument list of the call whose **name** is `needle`.
///
/// The needle must end with the opening parenthesis, so the parenthesis it
/// names is the one the argument list opens.
fn call_arguments(code: &str, needle: &str) -> Option<String> {
    if !needle.ends_with('(') {
        return None;
    }
    let after = code.find(needle)? + needle.len();
    let open = after - 1;
    let bytes = code.as_bytes();
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
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(code[open + 1..cursor].to_owned());
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

/// Returns the public method names of the `impl` block anchored at
/// `signature`.
fn impl_public_methods(code: &str, signature: &str) -> Option<Vec<String>> {
    let start = find_definition(code, signature)?;
    let (open, close) = brace_balanced_block(code, start + signature.len())?;
    let block = &code[open..close];
    let mut names = Vec::new();
    for prefix in ["pub fn ", "pub const fn ", "pub async fn "] {
        let mut rest = block;
        while let Some(position) = rest.find(prefix) {
            rest = &rest[position + prefix.len()..];
            let name: String = rest
                .chars()
                .take_while(|character| character.is_alphanumeric() || *character == '_')
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
        }
    }
    Some(names)
}

/// Returns every public function's name and body in the source.
fn public_functions(code: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for prefix in ["pub fn ", "pub async fn ", "pub const fn "] {
        let mut rest = code;
        while let Some(position) = rest.find(prefix) {
            rest = &rest[position + prefix.len()..];
            let name: String = rest
                .chars()
                .take_while(|character| character.is_alphanumeric() || *character == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            if let Some(body) = item_body(code, &format!("{prefix}{name}")) {
                found.push((name, body));
            }
        }
    }
    found
}

/// Returns the reachability signal variants the text records.
fn recorded_signal_variants(text: &str) -> Vec<String> {
    let needle = "ReachabilitySignal::";
    let mut variants = Vec::new();
    let mut rest = text;
    while let Some(position) = rest.find(needle) {
        rest = &rest[position + needle.len()..];
        let name: String = rest
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        variants.push(name);
    }
    variants
}

// ---- 1. the re-assertion is gated on the same opt-in --------------------
fn check_the_re_assertion_is_gated_on_the_same_opt_in(runtime: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(recorder) = item_body(&code, "fn note_bound_endpoints_as_explicit_bind") else {
        return Some("the runtime's bind-time explicit-bind recorder is absent".to_owned());
    };
    let Some(refresh) = item_body(&code, "fn refresh_explicit_bind_locked") else {
        return Some("the runtime's explicit-bind re-assertion is absent".to_owned());
    };

    if !recorder.contains(OPT_IN_GATE) {
        return Some("the recorder must be gated on the explicit-bind opt-in".to_owned());
    }
    if !refresh.contains(OPT_IN_GATE) {
        return Some(
            "the re-assertion must be gated on the same opt-in the recorder is".to_owned(),
        );
    }

    // The recorder must also **capture** the standing fact, so the
    // re-assertion has something to re-assert: the families it may name and
    // the time the fact was last asserted.
    for needle in [
        "state.explicit_bind_families = families.iter().copied().collect();",
        "state.explicit_bind_recorded_at = Some(now);",
    ] {
        if !recorder.contains(needle) {
            return Some(format!(
                "the recorder must capture the standing fact the re-assertion re-asserts: {needle}"
            ));
        }
    }
    None
}

// ---- 2. the lifetime comes from the tracker's own policy ----------------
fn check_the_lifetime_comes_from_the_trackers_policy(runtime: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(refresh) = item_body(&code, "fn refresh_explicit_bind_locked") else {
        return Some("the runtime's explicit-bind re-assertion is absent".to_owned());
    };
    if !refresh.contains("state.reachability.policy().observation_ttl()") {
        return Some(
            "the re-assertion must read the lifetime from the tracker's own policy".to_owned(),
        );
    }
    // The lifetime must never be restated inside the runtime's own rule.
    for needle in [
        "DEFAULT_OBSERVATION_TTL",
        "Duration::from_secs(",
        "Duration::from_millis(",
        "Duration::from_mins(",
    ] {
        if refresh.contains(needle) {
            return Some(format!(
                "the re-assertion must not restate the observation lifetime: {needle}"
            ));
        }
    }

    // And the policy the tracker holds must be built from the runtime's own
    // parameter, not from a literal the runtime repeats.
    let Some(arguments) = call_arguments(&code, "ReachabilityPolicy::new(") else {
        return Some("the runtime's reachability policy is absent".to_owned());
    };
    if !arguments.contains("config.reachability.observation_ttl") {
        return Some(
            "the runtime's reachability policy must be built from its own observation lifetime"
                .to_owned(),
        );
    }
    if arguments.contains("DEFAULT_OBSERVATION_TTL") {
        return Some(
            "the runtime's reachability policy must not restate the observation lifetime"
                .to_owned(),
        );
    }
    None
}

// ---- 3. the re-assertion rides the existing maintenance -----------------
fn check_the_re_assertion_rides_the_existing_maintenance(runtime: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(refresh) = item_body(&code, "fn refresh_explicit_bind_locked") else {
        return Some("the runtime's explicit-bind re-assertion is absent".to_owned());
    };
    let Some(maintenance) = item_body(&code, "fn drive_timeouts_locked") else {
        return Some("the runtime's existing maintenance pass is absent".to_owned());
    };
    if !maintenance.contains("self.refresh_explicit_bind_locked(state);") {
        return Some(
            "the re-assertion must ride the runtime's existing maintenance pass".to_owned(),
        );
    }

    // Exactly **one** call site in the whole runtime: a second call site
    // would be a second timer. The receiver is not matched, so a second
    // call on any receiver is caught.
    let call = ".refresh_explicit_bind_locked(";
    let call_sites = code.matches(call).count();
    if call_sites != 1 {
        return Some(format!(
            "the re-assertion must have exactly one call site, so no second timer exists: {call_sites}"
        ));
    }

    // And the re-assertion itself adds no timer, no task and no socket.
    for needle in [
        "interval(",
        "sleep(",
        "spawn",
        "tokio::",
        "UdpSocket",
        "TcpListener",
        "std::net",
    ] {
        if refresh.contains(needle) {
            return Some(format!(
                "the re-assertion must add no infrastructure of its own: {needle}"
            ));
        }
    }
    None
}

// ---- 4. the rule re-asserts strictly before the record can lapse --------
fn check_the_rule_re_asserts_before_the_lapse(runtime: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(rule) = item_body(&code, "pub fn explicit_bind_refresh_due") else {
        return Some("the runtime's re-assertion rule is absent".to_owned());
    };
    for needle in [
        "let Some(recorded_at) = recorded_at else {",
        "return true;",
        "let half = observation_ttl / 2;",
        "now.saturating_sub(recorded_at) >= half",
    ] {
        if !rule.contains(needle) {
            return Some(format!("the rule must keep its stated shape: {needle}"));
        }
    }

    // The property that shape gives: half a positive lifetime is strictly
    // less than the lifetime, so the re-assertion is always due **before** the
    // record can lapse, and never after it.
    for lifetime in [
        Duration::from_millis(1),
        Duration::from_millis(500),
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(60),
        Duration::from_secs(1_800),
        Duration::from_secs(3_600),
    ] {
        let half = lifetime / 2;
        if half.is_zero() {
            return Some("half the observation lifetime must not be zero".to_owned());
        }
        if half >= lifetime {
            return Some(
                "half the observation lifetime must be strictly less than the lifetime".to_owned(),
            );
        }
    }
    None
}

// ---- 5. the re-assertion touches only the router's own tracker ----------
fn check_the_re_assertion_touches_only_the_routers_own_tracker(runtime: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(refresh) = item_body(&code, "fn refresh_explicit_bind_locked") else {
        return Some("the runtime's explicit-bind re-assertion is absent".to_owned());
    };

    // The only reachability signal it records is this router's own bind.
    let variants = recorded_signal_variants(&refresh);
    if variants.is_empty() {
        return Some("the re-assertion must record the router's own class".to_owned());
    }
    for variant in variants {
        if variant != "LocalConfiguredBind" {
            return Some(format!(
                "the re-assertion must record no reachability signal but its own: {variant}"
            ));
        }
    }

    // It writes no peer profile, and claims no corroborating class for any
    // peer: a local condition must never be written into a peer's profile.
    for needle in [
        "profile",
        "Profile",
        "peer",
        "router_hash",
        "RouterIdentity",
    ] {
        if refresh.contains(needle) {
            return Some(format!(
                "the re-assertion must write no peer profile and claim no peer: {needle}"
            ));
        }
    }

    // And it records into the router's **own** tracker.
    if !refresh.contains("state.reachability") {
        return Some("the re-assertion must record into the router's own tracker".to_owned());
    }
    None
}

// ---- 6. the transport's model is untouched ------------------------------
fn check_the_transports_model_is_untouched(reachability: &str) -> Option<String> {
    let code = code_view(reachability);

    if !code.contains("pub const MIN_CORROBORATION_FLOOR: usize = 2;") {
        return Some(
            "the corroboration floor must stay at two distinct supporting classes".to_owned(),
        );
    }
    if !code.contains("pub const MIN_CONTRADICTION_FLOOR: usize = 1;") {
        return Some(
            "the contradiction floor must stay at one distinct contradicting class".to_owned(),
        );
    }
    if !code.contains("pub const DEFAULT_OBSERVATION_TTL: Duration = Duration::from_secs(30 * 60);")
    {
        return Some("the transport's own observation lifetime must stay as it is".to_owned());
    }

    // The signal vocabulary, read as the **whole** enum, so a variant cannot
    // be renamed or removed without the rule noticing.
    let Some(signals) = item_text(&code, "pub enum ReachabilitySignal") else {
        return Some("the transport's reachability signal vocabulary is absent".to_owned());
    };
    for needle in [
        "LocalConfiguredBind {",
        "AuthenticatedPeerObservedExternalAddress {",
        "ValidatedPath {",
        "PeerTestResult {",
        "RelayFirewalledSignal {",
    ] {
        if !signals.contains(needle) {
            return Some(format!(
                "the transport's signal vocabulary must stay as it is: {needle}"
            ));
        }
    }

    // And the corroboration classes, read as the **whole** function, so a
    // class cannot be renumbered without the rule noticing.
    let Some(classes) = item_text(&code, "const fn corroboration_class(self) -> u8") else {
        return Some("the transport's corroboration classes are absent".to_owned());
    };
    for needle in [
        "Self::LocalConfiguredBind { .. } => 0,",
        "Self::AuthenticatedPeerObservedExternalAddress { .. } => 1,",
        "Self::ValidatedPath { .. } => 2,",
        "Self::PeerTestResult { .. } => 3,",
        "Self::RelayFirewalledSignal { .. } => 4,",
    ] {
        if !classes.contains(needle) {
            return Some(format!(
                "the transport's corroboration classes must stay as they are: {needle}"
            ));
        }
    }

    // The tracker gains **no** method: its public surface is exactly the
    // eight Plan 530 left.
    let Some(methods) = impl_public_methods(&code, "impl ReachabilityTracker") else {
        return Some("the transport's reachability tracker is absent".to_owned());
    };
    let mut actual: Vec<&str> = methods.iter().map(String::as_str).collect();
    let mut expected: Vec<&str> = EXPECTED_TRACKER_METHODS.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    if actual != expected {
        return Some(format!(
            "the transport's tracker must gain no method: {actual:?} instead of {expected:?}"
        ));
    }
    None
}

// ---- 7. no configuration key is added -----------------------------------
fn check_no_configuration_key_is_added(router_i2np: &str, config: &str) -> Option<String> {
    let surface = code_view(config);
    let Some(raw) = item_body(&surface, "struct RawSsu2Config") else {
        return Some("the SSU2 configuration surface is absent".to_owned());
    };
    for needle in ["explicit_bind", "observation_ttl", "reachability"] {
        if raw.contains(needle) {
            return Some(format!(
                "the daemon's configuration surface must gain no key for the re-assertion: {needle}"
            ));
        }
    }
    if raw.contains("HashMap") || raw.contains("#[serde(flatten)]") {
        return Some("the configuration must gain no new free-form key".to_owned());
    }

    let composition = code_view(router_i2np);
    let Some(mapping) = item_body(&composition, "pub fn ssu2_runtime_config_from(") else {
        return Some("the daemon's SSU2 runtime mapping is absent".to_owned());
    };
    // The mapping names the runtime's **default** policy and nothing else:
    // the daemon's surface gains no lifetime of its own.
    if !mapping.contains("reachability: i2pr_runtime::Ssu2RuntimeReachability::default(),") {
        return Some(
            "the daemon's mapping must name the runtime's own default reachability policy"
                .to_owned(),
        );
    }
    if mapping.matches("reachability:").count() != 1 {
        return Some("the mapping must name the reachability policy exactly once".to_owned());
    }
    for needle in ["observation_ttl", "Ssu2RuntimeReachability {"] {
        if mapping.contains(needle) {
            return Some(format!(
                "the daemon's mapping must name no observation lifetime of its own: {needle}"
            ));
        }
    }
    None
}

// ---- 8. every re-assertion is counted -----------------------------------
fn check_every_re_assertion_is_counted(runtime: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(refresh) = item_body(&code, "fn refresh_explicit_bind_locked") else {
        return Some("the runtime's explicit-bind re-assertion is absent".to_owned());
    };
    if !refresh.contains("explicit_bind_refreshes") {
        return Some("every re-assertion must be counted".to_owned());
    }
    if !refresh.contains("fetch_add(1,") {
        return Some("the re-assertion must increment its counter by one".to_owned());
    }

    // The counter is a **count**: a plain integer in the snapshot and an
    // atomic in the runtime's own counters.
    let Some(snapshot) = item_text(&code, "pub struct Ssu2Snapshot") else {
        return Some("the runtime's snapshot is absent".to_owned());
    };
    if !snapshot.contains("pub explicit_bind_refreshes: u64,") {
        return Some(
            "the snapshot must publish the re-assertion count as a plain count".to_owned(),
        );
    }
    let Some(counters) = item_text(&code, "struct ServiceCounters") else {
        return Some("the runtime's counters are absent".to_owned());
    };
    if !counters.contains("explicit_bind_refreshes: AtomicU64,") {
        return Some("the runtime's counter must be a count and nothing else".to_owned());
    }

    // Nothing publishes the counter but the snapshot, and nothing mutates it
    // from the outside: a count that could be set would not be a count.
    for (name, body) in public_functions(&code) {
        if body.contains("explicit_bind_refreshes") && name != "snapshot" {
            return Some(format!(
                "only the snapshot may publish the re-assertion count: {name}"
            ));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules as tests.
// ----------------------------------------------------------------------

#[test]
fn the_re_assertion_is_gated_on_the_same_opt_in() {
    let reason = check_the_re_assertion_is_gated_on_the_same_opt_in(&source_of(RUNTIME_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_lifetime_comes_from_the_trackers_policy() {
    let reason = check_the_lifetime_comes_from_the_trackers_policy(&source_of(RUNTIME_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_re_assertion_rides_the_existing_maintenance() {
    let reason = check_the_re_assertion_rides_the_existing_maintenance(&source_of(RUNTIME_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_rule_re_asserts_before_the_lapse() {
    let reason = check_the_rule_re_asserts_before_the_lapse(&source_of(RUNTIME_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_re_assertion_touches_only_the_routers_own_tracker() {
    let reason =
        check_the_re_assertion_touches_only_the_routers_own_tracker(&source_of(RUNTIME_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_transports_model_is_untouched() {
    let reason = check_the_transports_model_is_untouched(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn no_configuration_key_is_added() {
    let reason = check_no_configuration_key_is_added(
        &source_of(DAEMON_ROUTER_I2NP),
        &source_of(DAEMON_CONFIG),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn every_re_assertion_is_counted() {
    let reason = check_every_re_assertion_is_counted(&source_of(RUNTIME_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

/// Plan 530's guard stays in force.
///
/// This plan changes the runtime's recorder's **neighbour**, not the
/// recorder, so the predecessor's guard must still be present and must still
/// carry its own rules.
#[test]
fn the_predecessors_guard_stays_in_force() {
    let source = source_of(PREDECESSOR_GUARD);
    for needle in [
        "fn the_recorder_lives_in_the_runtime",
        "fn the_recorder_is_gated",
        "fn the_reachability_model_is_untouched",
        "fn no_new_key_and_the_existing_mapping",
        "fn the_controlled_path_is_unchanged",
        "fn the_daemon_records_no_reachability_signal",
        "fn the_recorder_adds_no_infrastructure",
    ] {
        assert!(
            source.contains(needle),
            "Plan 530's guard must stay in force: {needle}"
        );
    }
}

// ----------------------------------------------------------------------
// The inverted probes. Each one mutates an in-memory copy, so no file on
// disk is written.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_an_ungated_re_assertion() {
    let mut runtime = source_of(RUNTIME_MODULE);
    // The recorder's gate and the re-assertion's gate are the same text, so
    // the probe must target the **last** occurrence: the re-assertion's.
    let (head, tail) = runtime
        .rsplit_once(OPT_IN_GATE_LINE)
        .expect("the re-assertion's gate");
    runtime = format!("{head}if false {{{tail}");
    let reason = check_the_re_assertion_is_gated_on_the_same_opt_in(&runtime);
    assert!(
        reason.is_some(),
        "a re-assertion that ignores the opt-in must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_recorder_that_captures_nothing() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace(
        "            state.explicit_bind_families = families.iter().copied().collect();\n",
        "",
    );
    let reason = check_the_re_assertion_is_gated_on_the_same_opt_in(&runtime);
    assert!(
        reason.is_some(),
        "a recorder that captures no standing fact must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_restated_lifetime() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replacen(
        "state.reachability.policy().observation_ttl()",
        "Duration::from_secs(1800)",
        1,
    );
    let reason = check_the_lifetime_comes_from_the_trackers_policy(&runtime);
    assert!(
        reason.is_some(),
        "a restated observation lifetime must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_second_call_site() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime.push_str(
        "\nfn plan535_probe(runtime: &Ssu2RuntimeService, state: &mut ServiceState) {\n    runtime.refresh_explicit_bind_locked(state);\n}\n",
    );
    let reason = check_the_re_assertion_rides_the_existing_maintenance(&runtime);
    assert!(
        reason.is_some(),
        "a second call site must fail the rule: a second call site is a second timer"
    );
}

#[test]
fn the_guard_reports_a_re_assertion_that_adds_a_timer() {
    let mut runtime = source_of(RUNTIME_MODULE);
    let (head, tail) = runtime
        .rsplit_once(OPT_IN_GATE_LINE)
        .expect("the re-assertion's gate");
    runtime = format!(
        "{head}let _timer = tokio::time::interval(Duration::from_secs(1));\n        {tail}"
    );
    let reason = check_the_re_assertion_rides_the_existing_maintenance(&runtime);
    assert!(
        reason.is_some(),
        "a re-assertion that adds a timer must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_rule_that_waits_for_the_lapse() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace(
        "let half = observation_ttl / 2;",
        "let half = observation_ttl / 1;",
    );
    let reason = check_the_rule_re_asserts_before_the_lapse(&runtime);
    assert!(
        reason.is_some(),
        "a rule that waits for the lapse must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_re_assertion_that_claims_another_signal() {
    let mut runtime = source_of(RUNTIME_MODULE);
    let (head, tail) = runtime
        .rsplit_once(OPT_IN_GATE_LINE)
        .expect("the re-assertion's gate");
    runtime = format!(
        "{head}let _other = ReachabilitySignal::PeerTestResult {{ family: AddressFamily::Ipv4, outcome: i2pr_transport::PeerTestOutcomeKind::Confirmed }};\n        {tail}"
    );
    let reason = check_the_re_assertion_touches_only_the_routers_own_tracker(&runtime);
    assert!(
        reason.is_some(),
        "a re-assertion that claims another signal must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_re_assertion_that_writes_a_peer_profile() {
    let mut runtime = source_of(RUNTIME_MODULE);
    let (head, tail) = runtime
        .rsplit_once(OPT_IN_GATE_LINE)
        .expect("the re-assertion's gate");
    runtime = format!("{head}let _peer_profile = true;\n        {tail}");
    let reason = check_the_re_assertion_touches_only_the_routers_own_tracker(&runtime);
    assert!(
        reason.is_some(),
        "a re-assertion that writes a peer profile must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_changed_transport_model() {
    let mut reachability = source_of(TRANSPORT_REACHABILITY_MODULE);
    reachability = reachability.replace(
        "pub const MIN_CORROBORATION_FLOOR: usize = 2;",
        "pub const MIN_CORROBORATION_FLOOR: usize = 1;",
    );
    let reason = check_the_transports_model_is_untouched(&reachability);
    assert!(
        reason.is_some(),
        "a lowered corroboration floor must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_tracker_that_gains_a_method() {
    let mut reachability = source_of(TRANSPORT_REACHABILITY_MODULE);
    reachability = reachability.replace(
        "impl ReachabilityTracker {",
        "impl ReachabilityTracker {\n    pub fn plan535_probe(&self) {}\n",
    );
    let reason = check_the_transports_model_is_untouched(&reachability);
    assert!(
        reason.is_some(),
        "a tracker that gains a method must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_new_configuration_key() {
    let mut config = source_of(DAEMON_CONFIG);
    config = config.replace(
        "struct RawSsu2Config {",
        "struct RawSsu2Config {\n    observation_ttl: u64,",
    );
    let reason = check_no_configuration_key_is_added(&source_of(DAEMON_ROUTER_I2NP), &config);
    assert!(
        reason.is_some(),
        "a new configuration key must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_mapping_that_names_its_own_lifetime() {
    let mut router_i2np = source_of(DAEMON_ROUTER_I2NP);
    router_i2np = router_i2np.replace(
        "reachability: i2pr_runtime::Ssu2RuntimeReachability::default(),",
        "reachability: i2pr_runtime::Ssu2RuntimeReachability { observation_ttl: Duration::from_secs(1800) },",
    );
    let reason = check_no_configuration_key_is_added(&router_i2np, &source_of(DAEMON_CONFIG));
    assert!(
        reason.is_some(),
        "a mapping that names its own lifetime must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_uncounted_re_assertion() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace(
        "                .explicit_bind_refreshes\n                .fetch_add(1, Ordering::Relaxed);\n",
        "",
    );
    let reason = check_every_re_assertion_is_counted(&runtime);
    assert!(
        reason.is_some(),
        "a re-assertion that is not counted must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_counter_that_is_not_a_count() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace(
        "pub explicit_bind_refreshes: u64,",
        "pub explicit_bind_refreshes: String,",
    );
    let reason = check_every_re_assertion_is_counted(&runtime);
    assert!(
        reason.is_some(),
        "a counter that carries more than a count must fail the rule"
    );
}

// ----------------------------------------------------------------------
// The fail-closed probes. Every rule fails closed when the item it names
// is absent.
// ----------------------------------------------------------------------

#[test]
fn the_guard_fails_closed_on_every_absent_item() {
    let empty = String::new();
    assert!(check_the_re_assertion_is_gated_on_the_same_opt_in(&empty).is_some());
    assert!(check_the_lifetime_comes_from_the_trackers_policy(&empty).is_some());
    assert!(check_the_re_assertion_rides_the_existing_maintenance(&empty).is_some());
    assert!(check_the_rule_re_asserts_before_the_lapse(&empty).is_some());
    assert!(check_the_re_assertion_touches_only_the_routers_own_tracker(&empty).is_some());
    assert!(check_the_transports_model_is_untouched(&empty).is_some());
    assert!(check_no_configuration_key_is_added(&empty, &empty).is_some());
    assert!(check_every_re_assertion_is_counted(&empty).is_some());
}

#[test]
fn the_guard_fails_closed_on_an_absent_re_assertion() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace("fn refresh_explicit_bind_locked", "fn some_other_name");
    assert!(check_the_re_assertion_is_gated_on_the_same_opt_in(&runtime).is_some());
    assert!(check_the_lifetime_comes_from_the_trackers_policy(&runtime).is_some());
    assert!(check_the_re_assertion_rides_the_existing_maintenance(&runtime).is_some());
    assert!(check_the_re_assertion_touches_only_the_routers_own_tracker(&runtime).is_some());
    assert!(check_every_re_assertion_is_counted(&runtime).is_some());
}

#[test]
fn the_guard_fails_closed_on_an_absent_rule() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace("pub fn explicit_bind_refresh_due", "pub fn some_other_rule");
    assert!(check_the_rule_re_asserts_before_the_lapse(&runtime).is_some());
}

#[test]
fn the_guard_fails_closed_on_an_absent_recorder() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace(
        "fn note_bound_endpoints_as_explicit_bind",
        "fn some_other_recorder",
    );
    assert!(check_the_re_assertion_is_gated_on_the_same_opt_in(&runtime).is_some());
}

#[test]
fn the_guard_fails_closed_on_an_absent_maintenance_pass() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace("fn drive_timeouts_locked", "fn some_other_pass");
    assert!(check_the_re_assertion_rides_the_existing_maintenance(&runtime).is_some());
}
