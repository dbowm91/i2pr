//! Plan 352 — daemon configuration secret hygiene.
//!
//! Two live defects are closed here, both proven by rows rather than by prose.
//!
//! **D1.** `toml::de::Error`'s `Display` renders the offending *source line*,
//! and its `message()` can embed the rejected key *and value* on a
//! `deny_unknown_fields` failure. Either one reached `eprintln!("error: {e}")`
//! in `main.rs`, so a typo on a password line printed the password and the
//! likely operator response is to paste that into a bug report.
//!
//! **D2.** `Config::load` was a bare `fs::read_to_string`: the only
//! secret-bearing file in the daemon with no `& 0o077` gate, while
//! `i2pr-storage`, `i2pcontrol_tunnels.rs`, and `addressbook.rs` all enforce it.
//!
//! Every row below asserts on the **rendered** error, because the rendered
//! form is what reaches a terminal. A row that only checked an inner type would
//! pass while the leak remained.

use std::fs;
use std::path::{Path, PathBuf};

use i2pr_daemon::config::{Config, ConfigError};

/// Distinctive marker: if this string appears in any rendered error, the row
/// has caught a real leak rather than passing vacuously.
const SECRET: &str = "SUPERSECRETPASSWORD123";

/// A minimal valid config, so each row below varies exactly one thing.
fn valid_config(password: Option<&str>) -> String {
    let mut text = String::from(
        "schema_version = 1\n[router]\ndata_dir = \"/tmp/i2pr-plan352\"\n[logging]\nfilter = \"info\"\n",
    );
    if let Some(password) = password {
        text.push_str("[i2pcontrol]\nenabled = true\n");
        text.push_str(&format!("password = \"{password}\"\n"));
    }
    text
}

/// Writes `contents` to a private temp file and returns its path.
fn write_config(contents: &str) -> PathBuf {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("router.toml");
    fs::write(&path, contents).expect("write config");
    // The TempDir must outlive the file, so leak it deliberately: these are
    // short-lived unit rows and the OS reclaims /tmp.
    std::mem::forget(dir);
    path
}

/// Renders `error` the way `main.rs` does, plus the full `Debug` form, because
/// `ConfigError` derives `Debug` and a `{:?}` would be a second leak path.
fn render<E: std::fmt::Display + std::fmt::Debug>(error: &E) -> String {
    format!("{error} | debug={error:?}")
}

fn parse_error(contents: &str) -> ConfigError {
    Config::parse(contents).expect_err("config was expected to fail parsing")
}

// ---------------------------------------------------------------- D1: syntax

#[test]
fn plan352_syntax_error_on_a_password_line_leaks_no_secret_and_keeps_the_line() {
    // Unterminated string, so the error anchors on the password line.
    let contents = format!("schema_version = 1\n[i2pcontrol]\npassword = \"{SECRET}\n");
    let rendered = render(&parse_error(&contents));

    assert!(
        !rendered.contains(SECRET),
        "syntax error leaked the secret: {rendered}"
    );
    // The diagnostic must survive redaction: the operator still learns the line.
    assert!(
        rendered.contains("line 3"),
        "redaction destroyed the line number: {rendered}"
    );
    assert!(
        rendered.contains("redacted"),
        "no redaction marker present: {rendered}"
    );
}

// ------------------------------------------------------------------ D1: type

#[test]
fn plan352_type_error_on_a_password_line_leaks_no_secret() {
    // `log_level` expects a string; an unquoted word is both a type problem and
    // carries the secret value in the source line.
    let contents = format!("schema_version = 1\nlog_level = {SECRET}\n");
    let rendered = render(&parse_error(&contents));

    assert!(
        !rendered.contains(SECRET),
        "type error leaked the secret: {rendered}"
    );
    assert!(
        rendered.contains("line 2"),
        "redaction destroyed the line number: {rendered}"
    );
}

// ------------------------------------------------- D1: deny_unknown_fields

#[test]
fn plan352_unknown_field_error_on_a_secret_line_leaks_no_secret() {
    // This is the case that defeats a message-filtering redaction: toml's own
    // `message()` is `unknown field \`bogus_<secret>\`, expected one of ...`,
    // so the secret is inside the message text, not only in the rendered line.
    let contents = format!("schema_version = 1\nbogus_{SECRET} = 1\n");
    let rendered = render(&parse_error(&contents));

    assert!(
        !rendered.contains(SECRET),
        "unknown-field error leaked the secret through the message: {rendered}"
    );
    assert!(
        rendered.contains("line 2"),
        "redaction destroyed the line number: {rendered}"
    );
}

#[test]
fn plan352_the_underlying_toml_error_is_still_reachable_for_programmatic_callers() {
    use std::error::Error as _;

    let contents = format!("schema_version = 1\nbogus_{SECRET} = 1\n");
    let ConfigError::Parse(error) = parse_error(&contents) else {
        panic!("expected a parse error");
    };
    assert_eq!(error.line(), Some(2), "line was not recorded");
    assert!(error.column().is_some(), "column was not recorded");
    // Redaction must not become information loss for a caller that wants the
    // full upstream text: `source()` still carries it.
    let source = error.source().expect("source chain must be retained");
    assert!(
        format!("{source}").contains(SECRET),
        "the retained source no longer carries full fidelity"
    );
}

// ------------------------------------------- D1: no-secret diagnostics intact

#[test]
fn plan352_a_no_secret_config_error_still_reports_line_and_column() {
    let contents = "schema_version = 1\n[router]\ndata_dir = 5\n";
    let rendered = render(&parse_error(contents));

    assert!(
        rendered.contains("redacted"),
        "expected the redaction marker"
    );
    assert!(
        rendered.contains("line 3") && rendered.contains("column"),
        "a secret-free config lost its position diagnostic: {rendered}"
    );
}

// ----------------------------------------------------------------- D2: modes

#[cfg(unix)]
#[test]
fn plan352_group_readable_config_holding_a_password_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let path = write_config(&valid_config(Some(SECRET)));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod 644");

    let error = Config::load(&path).expect_err("0644 must be refused for a secret config");
    let rendered = render(&error);

    assert!(
        !rendered.contains(SECRET),
        "permission refusal leaked the password: {rendered}"
    );
    assert!(
        rendered.contains(path.to_string_lossy().as_ref()),
        "refusal must name the path: {rendered}"
    );
    assert!(
        rendered.contains("644") && rendered.contains("600"),
        "refusal must name the observed and required mode: {rendered}"
    );
}

#[cfg(unix)]
#[test]
fn plan352_private_config_holding_a_password_is_accepted() {
    use std::os::unix::fs::PermissionsExt;

    let path = write_config(&valid_config(Some(SECRET)));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod 600");

    let config = Config::load(&path).expect("0600 must be accepted");
    assert_eq!(
        config.i2pcontrol.password.as_str(),
        SECRET,
        "the password must survive a permitted load"
    );
}

#[cfg(unix)]
#[test]
fn plan352_a_config_without_a_secret_is_still_accepted_at_0644() {
    use std::os::unix::fs::PermissionsExt;

    // The gate must be conditional on a secret existing, not a blanket
    // permission tightening that would reject ordinary configs.
    let path = write_config(&valid_config(None));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod 644");

    Config::load(&path).expect("a secret-free config must load at 0644");
}

// --------------------------------------- non-POSIX: refuse, never pass silently

#[cfg(not(unix))]
#[test]
fn plan352_a_password_config_is_refused_rather_than_unchecked_on_non_posix() {
    let path = write_config(&valid_config(Some(SECRET)));
    let error = Config::load(&path).expect_err("must refuse: no mode can be checked");
    let rendered = render(&error);

    assert!(
        !rendered.contains(SECRET),
        "refusal leaked the password: {rendered}"
    );
    assert!(
        rendered.contains("no POSIX"),
        "refusal must state why, not silently fail: {rendered}"
    );
}

#[cfg(not(unix))]
#[test]
fn plan352_a_config_without_a_secret_still_loads_on_non_posix() {
    let path = write_config(&valid_config(None));
    Config::load(&path).expect("a secret-free config must still load");
}

// ------------------- platform-independent policy: both branches run anywhere

#[test]
fn plan352_permission_policy_refuses_when_no_mode_can_be_read() {
    // This is the row that stops the non-POSIX branch from being untested
    // dead code. It runs on Linux too, so "silently pass when the mode is
    // unavailable" is a mutation that fails here rather than only on Windows.
    let path = Path::new("/tmp/plan352-no-mode.toml");
    let error = i2pr_daemon::config::secret_file_permission_verdict(path, None)
        .expect_err("a platform with no mode must refuse, not pass");

    let rendered = render(&error);
    assert!(
        !rendered.contains(SECRET),
        "refusal leaked a secret: {rendered}"
    );
    assert!(
        rendered.contains("no POSIX"),
        "the refusal must say why the gate could not run: {rendered}"
    );
    assert!(
        rendered.contains("plan352-no-mode.toml"),
        "the refusal must name the path: {rendered}"
    );
}

#[test]
fn plan352_permission_policy_rejects_group_or_world_readable_and_allows_private() {
    let path = Path::new("/tmp/plan352-mode.toml");

    for mode in [0o644, 0o640, 0o604, 0o660, 0o606] {
        let error = i2pr_daemon::config::secret_file_permission_verdict(path, Some(mode))
            .expect_err("group/world readable must be refused");
        let rendered = render(&error);
        assert!(
            rendered.contains(&format!("{mode:o}")),
            "refusal must name the observed mode {mode:o}: {rendered}"
        );
    }

    for mode in [0o600, 0o400, 0o700] {
        assert!(
            i2pr_daemon::config::secret_file_permission_verdict(path, Some(mode)).is_ok(),
            "mode {mode:o} must be accepted"
        );
    }
}

// --------------------------------------------------- D3: Config stays opaque

#[test]
fn plan352_config_debug_does_not_render_the_password() {
    let path = write_config(&valid_config(Some(SECRET)));
    // The gate is real: an un-chmod'd secret config is refused outright, which
    // is why this row must make the file private before it can reach `Debug`.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod 600");
    }
    let config = Config::load(Path::new(&path)).expect("load");

    let rendered = format!("{config:?}");
    assert!(
        !rendered.contains(SECRET),
        "Config's derived Debug leaked the password: {rendered}"
    );
    assert!(
        rendered.contains("I2pControlPassword([redacted])"),
        "the redacting Debug impl is no longer used: {rendered}"
    );
}
