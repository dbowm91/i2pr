//! Plan 549 boundary — the path in use is re-proved, and nothing else moves.
//!
//! The plan gives reachability class 2 (`ValidatedPath`) a producer that keeps
//! firing while a session is **stable**. That is a change to when evidence
//! arrives, and a change like that can quietly become a change to *what*
//! evidence means: a re-validation that migrates, a re-validation that records
//! two classes, a relaxed corroboration floor, a daemon that writes a signal.
//! Each of those would still make the floodfill stay `Active` and would make
//! the measurement lie. This guard is the authority for the shape.
//!
//! Six rules, each with an inverted probe:
//!
//! 1. the reachability **model** is untouched — the floor, the classes and the
//!    observation lifetime are exactly what Plan 528 left;
//! 2. a re-validation **cannot migrate** — its outcome is distinct and the
//!    validated endpoint is never assigned on that path;
//! 3. **one observation records one class** — the re-validation recorder
//!    names `ValidatedPath` and nothing else;
//! 4. the **daemon** records no reachability signal of its own;
//! 5. the probe is **bounded** — a ceiling, one session per pass, no new
//!    infrastructure;
//! 6. the cadence is a **scheduler deadline**, so an idle session still wakes
//!    to prove itself.
//!
//! The rules read a comment-stripped view, so a needle mentioned in prose is
//! not a violation. Every rule is a pure function of the sources it reads, so
//! the inverted probes run against the real tree with one string replaced.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

const PATH_MODULE: &str = "crates/i2pr-transport-ssu2/src/path.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
const DAEMON_LIB: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_PRODUCT: &str = "crates/i2pr-daemon/src/service_product.rs";
const DAEMON_ROUTER_I2NP: &str = "crates/i2pr-daemon/src/router_i2np.rs";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repository root")
}

fn source_of(relative: &str) -> String {
    let path = repository_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

/// Removes `//` and `/* */` comments and drops `#[cfg(test)]` items.
///
/// The view is deliberately simple: it keeps string literals and everything
/// inside them, because the rules look for code, and a rule that fired on
/// documentation would be a rule about documentation.
fn code_view(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    while let Some(current) = chars.next() {
        if in_string {
            out.push(current);
            if current == '\\' {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
                continue;
            }
            if current == '"' {
                in_string = false;
            }
            continue;
        }
        match current {
            '"' => {
                in_string = true;
                out.push(current);
            }
            '/' if chars.peek() == Some(&'/') => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    if next == '\n' {
                        out.push('\n');
                    }
                    previous = next;
                }
            }
            _ => out.push(current),
        }
    }
    strip_cfg_test_items(&out)
}

/// Drops every `#[cfg(test)]` item by brace matching.
fn strip_cfg_test_items(code: &str) -> String {
    const MARKER: &str = "#[cfg(test)]";
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(position) = rest.find(MARKER) {
        out.push_str(&rest[..position]);
        let after = &rest[position + MARKER.len()..];
        let Some(open) = after.find('{') else {
            break;
        };
        // A `mod tests { ... }` body: skip to its matching close brace.
        let mut depth = 0usize;
        let mut end = after.len();
        for (index, character) in after.char_indices().skip(open) {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Returns the text of one item whose signature starts with `signature`.
fn item_body(code: &str, signature: &str) -> Option<String> {
    let start = code.find(signature)?;
    let open = start + code[start..].find('{')?;
    let mut depth = 0usize;
    for (index, character) in code.char_indices().skip(open) {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(code[start..=index].to_owned());
                }
            }
            _ => {}
        }
    }
    None
}

struct Sources {
    path: String,
    runtime: String,
    reachability: String,
    daemon_lib: String,
    daemon_product: String,
    daemon_router_i2np: String,
}

impl Sources {
    fn from_tree() -> Self {
        Self {
            path: source_of(PATH_MODULE),
            runtime: source_of(RUNTIME_MODULE),
            reachability: source_of(REACHABILITY_MODULE),
            daemon_lib: source_of(DAEMON_LIB),
            daemon_product: source_of(DAEMON_PRODUCT),
            daemon_router_i2np: source_of(DAEMON_ROUTER_I2NP),
        }
    }
}

// ---- 1. the model is untouched -----------------------------------------

fn rule_the_model_is_untouched(sources: &Sources) -> Option<String> {
    let model = code_view(&sources.reachability);
    for needle in [
        "const MIN_CORROBORATION_FLOOR",
        "const MIN_CONTRADICTION_FLOOR",
        "pub const DEFAULT_OBSERVATION_TTL",
        "if supporting > self.policy.min_corroboration {",
        "Self::LocalConfiguredBind { .. } => 0,",
        "Self::AuthenticatedPeerObservedExternalAddress { .. } => 1,",
        "Self::ValidatedPath { .. } => 2,",
        "Self::PeerTestResult { .. } => 3,",
    ] {
        if !model.contains(needle) {
            return Some(format!("the reachability model must keep: {needle}"));
        }
    }
    if !model.contains("min_corroboration: 2,") {
        return Some("the corroboration floor must stay at two".to_owned());
    }
    // A re-validation is one class, so it must never be the thing that makes
    // two classes out of one. The class map names it exactly once, and it is
    // still class 2 -- the same class the migration path records.
    if model.matches("Self::ValidatedPath { .. } => 2,").count() != 1 {
        return Some("`ValidatedPath` must map to exactly corroboration class 2".to_owned());
    }
    None
}

// ---- 2. a re-validation cannot migrate ----------------------------------

fn rule_a_revalidation_cannot_migrate(sources: &Sources) -> Option<String> {
    let path = code_view(&sources.path);
    let Some(request) = item_body(&path, "pub fn request_revalidation(") else {
        return Some("the re-validation entry point is absent".to_owned());
    };
    if !request.contains("Ok(self.validated.endpoint())") {
        return Some("a re-validation must name the endpoint already in use".to_owned());
    }
    let Some(response) = item_body(&path, "pub fn on_path_response(") else {
        return Some("the response handler is absent".to_owned());
    };
    let Some(revalidated) = response.find("PathEvent::Revalidated") else {
        return Some("the response handler must answer a re-validation".to_owned());
    };
    // The re-validation branch runs before the migration machine and returns;
    // nothing between its `return` and the migration machine may assign the
    // validated endpoint. The window starts a little before the variant name
    // so the `return Ok(` that carries it is inside the view.
    let window = revalidated.saturating_sub(48);
    let branch = response.get(window..response.len()).unwrap_or_default();
    let branch = branch
        .split("let position = self")
        .next()
        .unwrap_or_default();
    if !branch.contains("return Ok(PathEvent::Revalidated") {
        return Some("a re-validation must return its own outcome".to_owned());
    }
    if branch.contains("self.validated.endpoint =") {
        return Some("a re-validation must never assign the validated endpoint".to_owned());
    }
    // The migration outcome stays the only one that moves the endpoint.
    let moves = path.matches("self.validated.endpoint =").count();
    if moves != 1 {
        return Some(format!(
            "exactly one path may assign the validated endpoint, found {moves}"
        ));
    }
    None
}

// ---- 3. one observation records one class -------------------------------

fn rule_one_observation_records_one_class(sources: &Sources) -> Option<String> {
    let runtime = code_view(&sources.runtime);
    let Some(recorder) = item_body(&runtime, "fn record_revalidated_path_locked(") else {
        return Some("the re-validation recorder is absent".to_owned());
    };
    let classes = recorder.matches("ReachabilitySignal::").count();
    if classes != 1 {
        return Some(format!(
            "a re-validation must record exactly one signal, found {classes}"
        ));
    }
    if !recorder.contains("ReachabilitySignal::ValidatedPath") {
        return Some("a re-validation must record the validated-path class".to_owned());
    }
    for forbidden in ["PeerTestResult", "AuthenticatedPeerObservedExternalAddress"] {
        if recorder.contains(forbidden) {
            return Some(format!("a re-validation must not also record {forbidden}"));
        }
    }
    None
}

// ---- 4. the daemon writes no reachability -------------------------------

fn rule_the_daemon_records_no_signal(sources: &Sources) -> Option<String> {
    for (name, source) in [
        ("lib.rs", &sources.daemon_lib),
        ("service_product.rs", &sources.daemon_product),
        ("router_i2np.rs", &sources.daemon_router_i2np),
    ] {
        let code = code_view(source);
        if code.contains("ReachabilitySignal::") {
            return Some(format!(
                "the daemon must record no reachability signal of its own: {name}"
            ));
        }
    }
    None
}

// ---- 5. the probe is bounded -------------------------------------------

fn rule_the_probe_is_bounded(sources: &Sources) -> Option<String> {
    let path = code_view(&sources.path);
    if !path.contains("pub const MAX_PATH_REVALIDATIONS_PER_SESSION") {
        return Some("the re-validation budget must be a named ceiling".to_owned());
    }
    if !path.contains(">= MAX_PATH_REVALIDATIONS_PER_SESSION") {
        return Some("the ceiling must be enforced, not merely declared".to_owned());
    }
    if !path.contains("PathError::RevalidationInFlight") {
        return Some("a second in-flight re-validation must be refused".to_owned());
    }
    let runtime = code_view(&sources.runtime);
    let Some(driver) = item_body(&runtime, "fn drive_path_revalidations_locked(") else {
        return Some("the re-validation driver is absent".to_owned());
    };
    // One session per pass: the loop breaks as soon as a target is chosen.
    if !driver.contains("break;") {
        return Some("the driver must probe at most one session per pass".to_owned());
    }
    for forbidden in [
        "tokio::spawn",
        "std::thread",
        "TcpListener",
        "UdpSocket::bind",
    ] {
        if driver.contains(forbidden) {
            return Some(format!(
                "the re-validation must add no infrastructure: {forbidden}"
            ));
        }
    }
    if !driver.contains("revalidation_in_flight(") {
        return Some("the driver must not stack probes on one session".to_owned());
    }
    None
}

// ---- 6. the cadence is a scheduler deadline ----------------------------

fn rule_the_cadence_is_a_deadline(sources: &Sources) -> Option<String> {
    let runtime = code_view(&sources.runtime);
    // The `sleep_for` computation must consider the re-validation, or an idle
    // session would never wake to prove itself -- the exact dependency this
    // plan removes.
    if !runtime.contains("next_revalidation_ms") {
        return Some("the session must carry its next re-validation instant".to_owned());
    }
    let Some(scheduler) = item_body(&runtime, "fn scheduler_sleep_for(") else {
        // The helper may be inlined; fall back to a whole-view assertion.
        if !runtime.contains("earliest = earliest.min(at);") {
            return Some("the scheduler deadline computation is absent".to_owned());
        }
        return None;
    };
    if !scheduler.contains("revalidation_in_flight(") && !scheduler.contains("next_revalidation_ms")
    {
        return Some(
            "the scheduler must wake for a due re-validation, or an idle session never probes"
                .to_owned(),
        );
    }
    None
}

const RULES: [(&str, fn(&Sources) -> Option<String>); 6] = [
    ("the-model-is-untouched", rule_the_model_is_untouched),
    (
        "a-revalidation-cannot-migrate",
        rule_a_revalidation_cannot_migrate,
    ),
    (
        "one-observation-records-one-class",
        rule_one_observation_records_one_class,
    ),
    (
        "the-daemon-records-no-signal",
        rule_the_daemon_records_no_signal,
    ),
    ("the-probe-is-bounded", rule_the_probe_is_bounded),
    ("the-cadence-is-a-deadline", rule_the_cadence_is_a_deadline),
];

fn evaluate(sources: &Sources) -> Vec<(&'static str, String)> {
    let mut hits = Vec::new();
    for (rule, apply) in RULES {
        if let Some(message) = apply(sources) {
            hits.push((rule, message));
        }
    }
    hits
}

#[test]
fn the_tree_satisfies_every_rule() {
    let sources = Sources::from_tree();
    let hits = evaluate(&sources);
    assert!(hits.is_empty(), "Plan 549 boundary violations: {hits:#?}");
}

#[test]
fn every_rule_is_exercised_by_the_inverted_probes() {
    let sources = Sources::from_tree();
    let exercised: Vec<&str> = RULES.iter().map(|(id, _)| *id).collect();
    assert_eq!(exercised.len(), 6);
    for (rule, _) in RULES {
        assert!(!rule.is_empty());
    }
    // The rules must be able to fail at all: a rule that never returns a
    // message on a mutated tree would be decoration.
    let mut mutated = Sources::from_tree();
    mutated.reachability = mutated
        .reachability
        .replace("min_corroboration: 2,", "min_corroboration: 1,");
    assert!(
        rule_the_model_is_untouched(&mutated).is_some(),
        "the model rule must catch a lowered floor"
    );
    let _ = sources;
}

#[test]
fn the_probe_fires_when_a_revalidation_migrates() {
    let mut sources = Sources::from_tree();
    sources.path = sources.path.replace(
        "return Ok(PathEvent::Revalidated { endpoint });",
        "self.validated.endpoint = endpoint; return Ok(PathEvent::Revalidated { endpoint });",
    );
    assert!(
        rule_a_revalidation_cannot_migrate(&sources).is_some(),
        "a re-validation that moves the validated endpoint must be reported"
    );
}

#[test]
fn the_probe_fires_when_a_revalidation_records_two_classes() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "ReachabilitySignal::ValidatedPath { family }, now);",
        "ReachabilitySignal::ValidatedPath { family }, now);\n        state.reachability.record(ReachabilitySignal::PeerTestResult { family, outcome: i2pr_transport::PeerTestOutcomeKind::Confirmed }, now);",
    );
    // The recorder is one of two call sites with that text; the migration path
    // has it too, so the rule must report at least once.
    assert!(
        rule_one_observation_records_one_class(&sources).is_some(),
        "a re-validation that records a second class must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_daemon_writes_a_signal() {
    let mut sources = Sources::from_tree();
    sources.daemon_product = format!(
        "{}\nfn leak() {{ let _ = ReachabilitySignal::ValidatedPath {{ family }}; }}\n",
        sources.daemon_product
    );
    assert!(
        rule_the_daemon_records_no_signal(&sources).is_some(),
        "a daemon-side signal write must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_budget_is_not_enforced() {
    let mut sources = Sources::from_tree();
    sources.path = sources.path.replace(
        "if self.revalidations_issued >= MAX_PATH_REVALIDATIONS_PER_SESSION {",
        "if false {",
    );
    assert!(
        rule_the_probe_is_bounded(&sources).is_some(),
        "a declared but unenforced ceiling must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_scheduler_ignores_the_cadence() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources
        .runtime
        .replace("next_revalidation_ms", "unused_revalidation_ms");
    assert!(
        rule_the_cadence_is_a_deadline(&sources).is_some(),
        "a session that never wakes for its cadence must be reported"
    );
}
