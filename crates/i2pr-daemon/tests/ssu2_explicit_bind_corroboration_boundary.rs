//! Plan 530's guard -- the router must record its own bound address.
//!
//! The transport's reachability model says `Reachable` needs **three distinct
//! supporting classes**. Two had ordinary producers: Plan 529's handshake
//! observation (class 1) and Plan 521's peer test (class 3). The third -- the
//! router's own configured bind (class 0) -- had exactly **one** recorder, and
//! the tree's only caller was the controlled floodfill activation. The daemon
//! already opted in and the transport already counted the class, so the
//! router could never publish a direct SSU2 address in ordinary operation.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the recorder lives in the **runtime**, and `start()` calls it **after**
//!    the sockets are bound;
//! 2. the recorder is **gated** on the existing `explicit_bind_corroboration`
//!    opt-in, and the transport's own gate still skips the class;
//! 3. the transport's reachability **model** -- its signal kinds, its
//!    corroboration classes and both floors -- is untouched;
//! 4. the configuration surface gains **no** key and the daemon's existing
//!    mapping is unchanged;
//! 5. the controlled activation's narrow recorder and its call site are
//!    unchanged;
//! 6. the daemon records no reachability signal of its own;
//! 7. the recorder adds no task, no timer, no listener and no socket.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, and every rule
//! **fails closed** when the item it names is absent.

#![forbid(unsafe_code)]

const TRANSPORT_REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const DAEMON_ROUTER_I2NP: &str = "crates/i2pr-daemon/src/router_i2np.rs";
const DAEMON_FLOODFILL: &str = "crates/i2pr-daemon/src/floodfill.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";

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
///
/// A rule that is about what an item *takes* -- a permit, an argument --
/// must read the signature too, which the body alone does not carry.
fn item_text(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[start..close].to_owned())
}

// ---- 1. the recorder lives in the runtime -------------------------------
fn check_the_recorder_lives_in_the_runtime(runtime: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(recorder) = item_body(&code, "fn note_bound_endpoints_as_explicit_bind") else {
        return Some("the runtime's bind-time explicit-bind recorder is absent".to_owned());
    };
    if !recorder.contains("ReachabilitySignal::LocalConfiguredBind") {
        return Some("the recorder must record the explicit-bind corroboration class".to_owned());
    }

    let Some(start) = item_body(&code, "pub async fn start(") else {
        return Some("the runtime's start is absent".to_owned());
    };
    if !start.contains("self.note_bound_endpoints_as_explicit_bind()") {
        return Some("the runtime's start must call the recorder".to_owned());
    }
    // The fact is true from the moment the bind succeeds, so the recorder
    // must run after the sockets are installed.
    let bound = start
        .find("guard.v4 = ")
        .or_else(|| start.find("guard.v6 = "));
    let call = start.find("self.note_bound_endpoints_as_explicit_bind()");
    match (bound, call) {
        (Some(bound), Some(call)) if call > bound => {}
        _ => return Some("the recorder must run after the sockets are bound".to_owned()),
    }
    None
}

// ---- 2. the recorder is gated on the existing opt-in --------------------
fn check_the_recorder_is_gated(runtime: &str, reachability: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(recorder) = item_body(&code, "fn note_bound_endpoints_as_explicit_bind") else {
        return Some("the runtime's bind-time explicit-bind recorder is absent".to_owned());
    };
    if !recorder.contains("explicit_bind_corroboration") {
        return Some("the recorder must be gated on the existing explicit-bind opt-in".to_owned());
    }
    if !recorder.contains("if !self.shared.config.explicit_bind_corroboration") {
        return Some("the recorder must refuse to record anything without the opt-in".to_owned());
    }

    // The transport's own gate must still skip the class when the policy
    // does not let an explicit configuration corroborate.
    let transport = code_view(reachability);
    if !transport.contains("!self.policy.configured_direct_allowed") {
        return Some(
            "the transport must still refuse an explicit configuration it does not allow"
                .to_owned(),
        );
    }
    None
}

// ---- 3. the transport's reachability model is untouched -----------------
fn check_the_reachability_model_is_untouched(reachability: &str) -> Option<String> {
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
    None
}

// ---- 4. no new key, the existing mapping unchanged ----------------------
fn check_no_new_key_and_the_existing_mapping(router_i2np: &str, config: &str) -> Option<String> {
    let composition = code_view(router_i2np);

    let Some(mapping) = item_body(&composition, "pub fn ssu2_runtime_config_from(") else {
        return Some("the daemon's SSU2 runtime mapping is absent".to_owned());
    };
    if !mapping.contains("explicit_bind_corroboration: true,") {
        return Some("the daemon's existing explicit-bind opt-in must stay mapped".to_owned());
    }
    if mapping.matches("explicit_bind_corroboration:").count() != 1 {
        return Some("the mapping must name the explicit-bind opt-in exactly once".to_owned());
    }

    let raw = code_view(config);
    let Some(surface) = item_body(&raw, "struct RawSsu2Config") else {
        return Some("the SSU2 configuration surface is absent".to_owned());
    };
    if surface.contains("explicit_bind") {
        return Some(
            "the configuration must gain no new explicit-bind key: the runtime's opt-in is the daemon's existing mapping"
                .to_owned(),
        );
    }
    if surface.contains("HashMap") || surface.contains("#[serde(flatten)]") {
        return Some("the configuration must gain no new free-form key".to_owned());
    }
    None
}

// ---- 5. the controlled path is unchanged --------------------------------
fn check_the_controlled_path_is_unchanged(runtime: &str, floodfill: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(narrow) = item_body(
        &code,
        "pub fn note_explicit_bind_for_controlled_qualification",
    ) else {
        return Some("the controlled-qualification recorder is absent".to_owned());
    };
    if !narrow.contains("ReachabilitySignal::LocalConfiguredBind") {
        return Some(
            "the controlled-qualification recorder must keep recording the class".to_owned(),
        );
    }
    if !narrow.contains("is_loopback()") {
        return Some(
            "the controlled-qualification recorder must keep refusing a non-loopback bind"
                .to_owned(),
        );
    }

    let activation = code_view(floodfill);
    let Some(call) = item_body(&activation, "pub async fn activate_controlled(") else {
        return Some("the controlled activation is absent".to_owned());
    };
    if !call.contains("note_explicit_bind_for_controlled_qualification()") {
        return Some("the controlled activation must keep its narrow recorder call".to_owned());
    }
    if !call.contains("ControlledActivationError::BindFailed") {
        return Some(
            "the controlled activation must keep failing closed when the bind cannot be named"
                .to_owned(),
        );
    }
    None
}

// ---- 6. the daemon records no reachability signal of its own ------------
fn check_the_daemon_records_no_reachability_signal(
    router_i2np: &str,
    floodfill: &str,
) -> Option<String> {
    for (name, text) in [
        ("the SSU2 composition", router_i2np),
        ("the floodfill activation", floodfill),
    ] {
        let code = code_view(text);
        if code.contains("ReachabilitySignal") {
            return Some(format!(
                "{name} must not record a reachability signal of its own"
            ));
        }
    }
    None
}

// ---- 7. the recorder adds no infrastructure ------------------------------
fn check_the_recorder_adds_no_infrastructure(runtime: &str) -> Option<String> {
    let code = code_view(runtime);

    let Some(recorder) = item_body(&code, "fn note_bound_endpoints_as_explicit_bind") else {
        return Some("the runtime's bind-time explicit-bind recorder is absent".to_owned());
    };
    for needle in [
        "tokio::",
        "spawn",
        "TcpListener",
        "UdpSocket",
        "sleep(",
        "interval(",
        "std::net",
    ] {
        if recorder.contains(needle) {
            return Some(format!(
                "the recorder must add no infrastructure of its own: {needle}"
            ));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules as tests.
// ----------------------------------------------------------------------

#[test]
fn the_recorder_lives_in_the_runtime() {
    let reason = check_the_recorder_lives_in_the_runtime(&source_of(RUNTIME_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_recorder_is_gated() {
    let reason = check_the_recorder_is_gated(
        &source_of(RUNTIME_MODULE),
        &source_of(TRANSPORT_REACHABILITY_MODULE),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_reachability_model_is_untouched() {
    let reason =
        check_the_reachability_model_is_untouched(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn no_new_key_and_the_existing_mapping() {
    let reason = check_no_new_key_and_the_existing_mapping(
        &source_of(DAEMON_ROUTER_I2NP),
        &source_of(DAEMON_CONFIG),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_controlled_path_is_unchanged() {
    let reason = check_the_controlled_path_is_unchanged(
        &source_of(RUNTIME_MODULE),
        &source_of(DAEMON_FLOODFILL),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_daemon_records_no_reachability_signal() {
    let reason = check_the_daemon_records_no_reachability_signal(
        &source_of(DAEMON_ROUTER_I2NP),
        &source_of(DAEMON_FLOODFILL),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_recorder_adds_no_infrastructure() {
    let reason = check_the_recorder_adds_no_infrastructure(&source_of(RUNTIME_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

// ----------------------------------------------------------------------
// The inverted probes. Each one mutates an in-memory copy, so no file on
// disk is written.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_a_recorder_outside_the_runtime() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace(
        "fn note_bound_endpoints_as_explicit_bind",
        "fn some_other_name",
    );
    let reason = check_the_recorder_lives_in_the_runtime(&runtime);
    assert!(
        reason.is_some(),
        "a recorder that does not exist must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_start_that_never_calls_the_recorder() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace("self.note_bound_endpoints_as_explicit_bind();", "");
    let reason = check_the_recorder_lives_in_the_runtime(&runtime);
    assert!(
        reason.is_some(),
        "a start that never calls the recorder must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_recorder_that_runs_before_the_bind() {
    let mut runtime = source_of(RUNTIME_MODULE);
    // The call is moved above the block that installs the bound sockets.
    runtime = runtime.replace(
        "            if bound_v4.is_some() {\n                guard.v4 = bound_v4",
        "            self.note_bound_endpoints_as_explicit_bind();\n            if bound_v4.is_some() {\n                guard.v4 = bound_v4",
    );
    runtime = runtime.replace(
        "        self.note_bound_endpoints_as_explicit_bind();\n        Ok(Ssu2ServiceHandle {",
        "        Ok(Ssu2ServiceHandle {",
    );
    let reason = check_the_recorder_lives_in_the_runtime(&runtime);
    assert!(
        reason.is_some(),
        "a recorder that runs before the bind must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_ungated_recorder() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace(
        "        if !self.shared.config.explicit_bind_corroboration {
            return;
        }
",
        "",
    );
    let reason = check_the_recorder_is_gated(&runtime, &source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(
        reason.is_some(),
        "a recorder that ignores the opt-in must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_transport_that_counts_an_ungated_class() {
    let mut reachability = source_of(TRANSPORT_REACHABILITY_MODULE);
    reachability = reachability.replace("!self.policy.configured_direct_allowed", "false");
    let reason = check_the_recorder_is_gated(&source_of(RUNTIME_MODULE), &reachability);
    assert!(
        reason.is_some(),
        "a transport that always counts the class must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_changed_reachability_model() {
    let mut reachability = source_of(TRANSPORT_REACHABILITY_MODULE);
    reachability = reachability.replace(
        "pub const MIN_CORROBORATION_FLOOR: usize = 2;",
        "pub const MIN_CORROBORATION_FLOOR: usize = 1;",
    );
    let reason = check_the_reachability_model_is_untouched(&reachability);
    assert!(
        reason.is_some(),
        "a lowered corroboration floor must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_moved_corroboration_class() {
    let mut reachability = source_of(TRANSPORT_REACHABILITY_MODULE);
    reachability = reachability.replace(
        "Self::LocalConfiguredBind { .. } => 0,",
        "Self::LocalConfiguredBind { .. } => 4,",
    );
    let reason = check_the_reachability_model_is_untouched(&reachability);
    assert!(
        reason.is_some(),
        "a moved corroboration class must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_new_configuration_key() {
    let mut config = source_of(DAEMON_CONFIG);
    config = config.replace(
        "struct RawSsu2Config {",
        "struct RawSsu2Config {
    explicit_bind: bool,",
    );
    let reason = check_no_new_key_and_the_existing_mapping(&source_of(DAEMON_ROUTER_I2NP), &config);
    assert!(
        reason.is_some(),
        "a new explicit-bind key must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_unmapped_opt_in() {
    let mut router_i2np = source_of(DAEMON_ROUTER_I2NP);
    router_i2np = router_i2np.replace(
        "explicit_bind_corroboration: true,",
        "explicit_bind_corroboration: false,",
    );
    let reason = check_no_new_key_and_the_existing_mapping(&router_i2np, &source_of(DAEMON_CONFIG));
    assert!(reason.is_some(), "an unmapped opt-in must fail the rule");
}

#[test]
fn the_guard_reports_a_changed_controlled_path() {
    let mut floodfill = source_of(DAEMON_FLOODFILL);
    floodfill = floodfill.replace(
        "        .note_explicit_bind_for_controlled_qualification()\n",
        "",
    );
    let reason = check_the_controlled_path_is_unchanged(&source_of(RUNTIME_MODULE), &floodfill);
    assert!(
        reason.is_some(),
        "a controlled activation without its narrow recorder call must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_records_its_own_signal() {
    let mut router_i2np = source_of(DAEMON_ROUTER_I2NP);
    // The probe must inject **code**: the rule reads a comment-free view of
    // the source, so a comment would never fire it.
    router_i2np.push_str(
        "\nfn plan530_probe() { let _signal = ReachabilitySignal::LocalConfiguredBind; }\n",
    );
    let reason =
        check_the_daemon_records_no_reachability_signal(&router_i2np, &source_of(DAEMON_FLOODFILL));
    assert!(
        reason.is_some(),
        "a daemon that records its own signal must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_recorder_that_adds_infrastructure() {
    let mut runtime = source_of(RUNTIME_MODULE);
    runtime = runtime.replace(
        "        if !self.shared.config.explicit_bind_corroboration {",
        "        let _ = std::net::UdpSocket::bind(\"127.0.0.1:0\");\n        if !self.shared.config.explicit_bind_corroboration {",
    );
    let reason = check_the_recorder_adds_no_infrastructure(&runtime);
    assert!(
        reason.is_some(),
        "a recorder that opens a socket must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_missing_named_item() {
    // Every rule fails closed when the item it names is absent.
    let empty = String::new();
    assert!(check_the_recorder_lives_in_the_runtime(&empty).is_some());
    assert!(check_the_recorder_is_gated(&empty, &empty).is_some());
    assert!(check_the_reachability_model_is_untouched(&empty).is_some());
    assert!(check_no_new_key_and_the_existing_mapping(&empty, &empty).is_some());
    assert!(check_the_controlled_path_is_unchanged(&empty, &empty).is_some());
    assert!(check_the_recorder_adds_no_infrastructure(&empty).is_some());
}
