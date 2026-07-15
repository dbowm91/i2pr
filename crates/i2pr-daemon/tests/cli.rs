use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_i2pr"))
}

fn valid_config(path: &std::path::Path) {
    fs::write(path, "schema_version = 1\n[router]\ndata_dir = \"state\"\n")
        .expect("write valid config");
}

fn config_with_data_dir(path: &std::path::Path, data_dir: &std::path::Path) {
    fs::write(
        path,
        format!(
            "schema_version = 1\n[router]\ndata_dir = {:?}\n",
            data_dir.to_string_lossy()
        ),
    )
    .expect("write valid config");
}

#[test]
fn help_and_version_are_available() {
    let help = binary().arg("--help").output().expect("run help");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("check-config"));

    let version = binary().arg("--version").output().expect("run version");
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).starts_with("i2pr "));
}

#[test]
fn missing_config_maps_to_exit_code_ten() {
    let output = binary()
        .args(["check-config", "--config", "does-not-exist.toml"])
        .output()
        .expect("run check-config");
    assert_eq!(output.status.code(), Some(10));
    assert!(String::from_utf8_lossy(&output.stderr).contains("configuration file unavailable"));
}

#[test]
fn missing_required_argument_maps_to_usage_exit_code_two() {
    let output = binary()
        .args(["check-config"])
        .output()
        .expect("run incomplete command");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("required arguments"));
}

#[test]
fn malformed_and_unknown_config_are_rejected() {
    let directory = tempdir().expect("temp directory");
    let malformed = directory.path().join("malformed.toml");
    fs::write(&malformed, "schema_version = [").expect("write malformed config");
    let malformed_output = binary()
        .args(["check-config", "--config"])
        .arg(&malformed)
        .output()
        .expect("run malformed config");
    assert_eq!(malformed_output.status.code(), Some(11));

    let unknown = directory.path().join("unknown.toml");
    fs::write(
        &unknown,
        "schema_version = 1\nunknown = true\n[router]\ndata_dir = \"state\"\n",
    )
    .expect("write unknown config");
    let unknown_output = binary()
        .args(["check-config", "--config"])
        .arg(&unknown)
        .output()
        .expect("run unknown config");
    assert_eq!(unknown_output.status.code(), Some(11));

    let semantic = directory.path().join("semantic.toml");
    fs::write(
        &semantic,
        "schema_version = 1\n[router]\ndata_dir = \"state\"\n[limits]\nmax_tasks = 0\n",
    )
    .expect("write semantically invalid config");
    let semantic_output = binary()
        .args(["check-config", "--config"])
        .arg(&semantic)
        .output()
        .expect("run semantically invalid config");
    assert_eq!(semantic_output.status.code(), Some(12));
    assert!(String::from_utf8_lossy(&semantic_output.stderr).contains("limits.max_tasks"));
}

#[test]
fn dry_run_succeeds_and_live_run_is_not_implemented() {
    let directory = tempdir().expect("temp directory");
    let config = directory.path().join("config.toml");
    valid_config(&config);

    let dry_run = binary()
        .args(["run", "--config"])
        .arg(&config)
        .arg("--dry-run")
        .output()
        .expect("run dry-run");
    assert!(dry_run.status.success());
    assert!(String::from_utf8_lossy(&dry_run.stdout).contains("dry run complete"));

    let live = binary()
        .args(["run", "--config"])
        .arg(&config)
        .output()
        .expect("run live command");
    assert_eq!(live.status.code(), Some(20));
    assert!(String::from_utf8_lossy(&live.stderr).contains("live daemon execution is not enabled"));
}

#[test]
fn identity_lifecycle_is_explicit_and_inspection_redacts_private_material() {
    let directory = tempdir().expect("temp directory");
    let config = directory.path().join("config.toml");
    let data_dir = directory.path().join("state");
    config_with_data_dir(&config, &data_dir);

    let generated = binary()
        .args(["identity", "generate", "--config"])
        .arg(&config)
        .output()
        .expect("run identity generate");
    assert!(generated.status.success());
    assert!(data_dir.join("router.identity").is_file());

    let inspected = binary()
        .args(["identity", "inspect", "--config"])
        .arg(&config)
        .output()
        .expect("run identity inspect");
    assert!(inspected.status.success());
    let output = String::from_utf8_lossy(&inspected.stdout);
    assert!(output.contains("signing algorithm type 7"));
    assert!(output.contains("encryption algorithm type 4"));
    assert!(output.contains("private material was not displayed"));
}

#[test]
fn dry_run_does_not_create_identity_state() {
    let directory = tempdir().expect("temp directory");
    let config = directory.path().join("config.toml");
    let data_dir = directory.path().join("not-created");
    config_with_data_dir(&config, &data_dir);

    let output = binary()
        .args(["run", "--config"])
        .arg(&config)
        .arg("--dry-run")
        .output()
        .expect("run dry-run");
    assert!(output.status.success());
    assert!(!data_dir.exists());
}
