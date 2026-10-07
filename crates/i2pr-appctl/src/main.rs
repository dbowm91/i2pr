//! Offline administrator tool. Filesystem access to the router data directory
//! is the administrator boundary; there is no listener or live appd protocol.

#![forbid(unsafe_code)]

use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand, ValueEnum};
use i2pr_app_package::{PackageIdentity, verify_file};
use i2pr_app_proto::{AppId, Capability, LaunchProfile};
use i2pr_app_state::{AppPolicy, AppStateStore, ResourceCeilings, StateError};

#[derive(Parser)]
#[command(
    name = "i2pr-appctl",
    about = "Offline managed-application package and policy administration"
)]
struct Cli {
    #[arg(long, value_name = "DIR")]
    data_dir: PathBuf,
    #[arg(
        long,
        global = true,
        help = "Acknowledge that UnsafeDirect provides ordinary host networking without a sandbox"
    )]
    allow_direct_host_network: bool,
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Subcommand)]
enum TopCommand {
    Package(PackageArgs),
    Publisher(PublisherArgs),
    App(AppArgs),
}

#[derive(Args)]
struct PackageArgs {
    #[command(subcommand)]
    command: PackageCommand,
}
#[derive(Subcommand)]
enum PackageCommand {
    Verify {
        file: PathBuf,
    },
    Install {
        file: PathBuf,
    },
    List,
    Inspect {
        publisher: String,
        app: String,
        version: String,
    },
    Remove {
        publisher: String,
        app: String,
        version: String,
    },
}

#[derive(Args)]
struct PublisherArgs {
    #[command(subcommand)]
    command: PublisherCommand,
}
#[derive(Subcommand)]
enum PublisherCommand {
    Trust { fingerprint: String },
    Untrust { fingerprint: String },
}

#[derive(Args)]
struct AppArgs {
    #[command(subcommand)]
    command: AppCommand,
}
#[derive(Subcommand)]
enum AppCommand {
    Select {
        publisher: String,
        app: String,
        version: String,
    },
    Grant {
        publisher: String,
        app: String,
        capability: GrantCapability,
    },
    Revoke {
        publisher: String,
        app: String,
        capability: GrantCapability,
    },
    Profile {
        publisher: String,
        app: String,
        profile: Profile,
    },
    Autostart {
        publisher: String,
        app: String,
        state: OnOff,
    },
    Inspect {
        publisher: String,
        app: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum GrantCapability {
    Sam,
    I2cp,
}
impl From<GrantCapability> for Capability {
    fn from(v: GrantCapability) -> Self {
        match v {
            GrantCapability::Sam => Self::Sam,
            GrantCapability::I2cp => Self::I2cp,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Profile {
    Secured,
    UnsafeDirect,
}
impl From<Profile> for LaunchProfile {
    fn from(v: Profile) -> Self {
        match v {
            Profile::Secured => Self::Secured,
            Profile::UnsafeDirect => Self::UnsafeDirect,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("i2pr-appctl: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<(), StateError> {
    let root = managed_apps_root(cli.data_dir)?;
    let store = AppStateStore::open(root)?;
    match cli.command {
        TopCommand::Package(args) => package_command(&store, args.command),
        TopCommand::Publisher(args) => publisher_command(&store, args.command),
        TopCommand::App(args) => app_command(&store, args.command, cli.allow_direct_host_network),
    }
}

fn managed_apps_root(data_dir: PathBuf) -> Result<PathBuf, StateError> {
    let absolute = if data_dir.is_absolute() {
        data_dir
    } else {
        std::env::current_dir()?.join(data_dir)
    };
    Ok(absolute.join("managed-apps"))
}

fn package_command(store: &AppStateStore, command: PackageCommand) -> Result<(), StateError> {
    match command {
        PackageCommand::Verify { file } => {
            let package = verify_file(&file)?;
            println!(
                "valid signed package: publisher={} app={} version={} artifact_sha256={}",
                package.identity.publisher_key_id,
                package.identity.app_id.as_str(),
                package.identity.version.as_str(),
                package.artifact_sha256
            );
            println!(
                "signature identifies the publisher key; it does not grant trust, capabilities, selection, or launch authority"
            );
        }
        PackageCommand::Install { file } => {
            let package = store.install_package(&file)?;
            println!(
                "installed package: publisher={} app={} version={} artifact_sha256={}",
                package.identity.publisher_key_id,
                package.identity.app_id.as_str(),
                package.identity.version.as_str(),
                package.identity.artifact_sha256
            );
            println!("installation does not trust, select, or launch the package");
        }
        PackageCommand::List => {
            for package in store.packages().list()? {
                print_package(&package.identity);
            }
        }
        PackageCommand::Inspect {
            publisher,
            app,
            version,
        } => {
            let package = find_package(store, &publisher, &app, &version)?;
            print_package(&package.identity);
            println!(
                "requested_capabilities={:?} autostart_requested={} restart_requested={}",
                package
                    .manifest
                    .requested_capabilities
                    .iter()
                    .map(|r| r.capability)
                    .collect::<Vec<_>>(),
                package.manifest.autostart_requested,
                package.manifest.restart_requested
            );
        }
        PackageCommand::Remove {
            publisher,
            app,
            version,
        } => {
            let package = find_package(store, &publisher, &app, &version)?;
            store.remove_package(&package.identity)?;
            println!("removed package; application data was not changed");
        }
    }
    Ok(())
}

fn publisher_command(store: &AppStateStore, command: PublisherCommand) -> Result<(), StateError> {
    match command {
        PublisherCommand::Trust { fingerprint } => {
            validate_fingerprint(&fingerprint)?;
            store.mutate(|state| {
                if state
                    .trusted_publishers
                    .binary_search(&fingerprint)
                    .is_err()
                {
                    state.trusted_publishers.push(fingerprint.clone());
                    state.trusted_publishers.sort();
                }
                Ok(())
            })?;
            println!(
                "publisher key trusted; no application is selected or granted by this operation"
            );
        }
        PublisherCommand::Untrust { fingerprint } => {
            validate_fingerprint(&fingerprint)?;
            store.mutate(|state| {
                state.trusted_publishers.retain(|v| v != &fingerprint);
                for app in state
                    .apps
                    .iter_mut()
                    .filter(|app| app.publisher_id == fingerprint)
                {
                    app.granted_capabilities.clear();
                    app.launch_profile = None;
                    app.autostart = false;
                }
                Ok(())
            })?;
            println!(
                "publisher key untrusted; launch-enabling grants, profile, and autostart were cleared"
            );
        }
    }
    Ok(())
}

fn app_command(
    store: &AppStateStore,
    command: AppCommand,
    allow_direct_host_network: bool,
) -> Result<(), StateError> {
    match command {
        AppCommand::Select {
            publisher,
            app,
            version,
        } => {
            store.mutate(|state| {
                let package = find_package(store, &publisher, &app, &version)?;
                let identity = package.identity;
                let record = app_record(state, &publisher, &app)?;
                record.selected = Some(identity);
                record.autostart = false;
                Ok(())
            })?;
            println!("exact package selected; capabilities and autostart are unchanged");
        }
        AppCommand::Grant {
            publisher,
            app,
            capability,
        } => {
            let capability = Capability::from(capability);
            store.mutate(|state| {
                let record = app_record(state, &publisher, &app)?;
                let selected = record
                    .selected
                    .as_ref()
                    .ok_or(StateError::SelectedPackageUnavailable)?;
                let package = store
                    .packages()
                    .verify_installed(&store.packages().package_path(selected)?)?;
                if !package
                    .manifest
                    .requested_capabilities
                    .iter()
                    .any(|r| r.capability == capability)
                {
                    return Err(StateError::CapabilityNotRequested);
                }
                if !record.granted_capabilities.contains(&capability) {
                    record.granted_capabilities.push(capability);
                    record.granted_capabilities.sort();
                }
                Ok(())
            })?;
            println!("capability grant recorded for this publisher and application");
        }
        AppCommand::Revoke {
            publisher,
            app,
            capability,
        } => {
            let capability = Capability::from(capability);
            store.mutate(|state| {
                let record = app_record(state, &publisher, &app)?;
                record.granted_capabilities.retain(|v| *v != capability);
                Ok(())
            })?;
            println!("capability grant revoked");
        }
        AppCommand::Profile {
            publisher,
            app,
            profile,
        } => {
            let profile = LaunchProfile::from(profile);
            let unsafe_ack = matches!(profile, LaunchProfile::UnsafeDirect);
            if unsafe_ack && !allow_direct_host_network {
                return Err(StateError::UnsafeDirectAcknowledgementRequired);
            }
            store.mutate(|state| {
                let record = app_record(state, &publisher, &app)?;
                record.launch_profile = Some(profile);
                Ok(())
            })?;
            println!("launch profile recorded; changes apply after app-runtime restart");
        }
        AppCommand::Autostart {
            publisher,
            app,
            state: on_off,
        } => {
            let enabled = matches!(on_off, OnOff::On);
            store.mutate(|state| {
                let trusted = state.trusted_publishers.binary_search(&publisher).is_ok();
                let record = app_record(state, &publisher, &app)?;
                if enabled
                    && (!trusted || record.selected.is_none() || record.launch_profile.is_none())
                {
                    return Err(StateError::InvalidState);
                }
                record.autostart = enabled;
                Ok(())
            })?;
            println!("autostart policy recorded; changes apply after app-runtime restart");
        }
        AppCommand::Inspect { publisher, app } => {
            let app_id = AppId::parse(app.clone()).map_err(|_| StateError::InvalidState)?;
            let record = store.inspect_app(&publisher, &app_id)?;
            println!(
                "publisher={} app={} selected={:?} grants={:?} profile={:?} autostart={} max_connections={} resource_ceilings={:?}",
                record.publisher_id,
                record.app_id.as_str(),
                record.selected,
                record.granted_capabilities,
                record.launch_profile,
                record.autostart,
                record.max_connections,
                record.resource_ceilings
            );
        }
    }
    Ok(())
}

fn app_record<'a>(
    state: &'a mut i2pr_app_state::PolicyState,
    publisher: &str,
    app: &str,
) -> Result<&'a mut AppPolicy, StateError> {
    validate_fingerprint(publisher)?;
    let app_id = AppId::parse(app.to_owned()).map_err(|_| StateError::InvalidState)?;
    if let Some(index) = state
        .apps
        .iter()
        .position(|record| record.publisher_id == publisher && record.app_id == app_id)
    {
        return Ok(&mut state.apps[index]);
    }
    state.apps.push(AppPolicy {
        publisher_id: publisher.to_owned(),
        app_id,
        selected: None,
        granted_capabilities: Vec::new(),
        launch_profile: None,
        autostart: false,
        max_connections: 1,
        resource_ceilings: ResourceCeilings::default(),
    });
    state
        .apps
        .sort_by(|a, b| (&a.publisher_id, &a.app_id).cmp(&(&b.publisher_id, &b.app_id)));
    let index = state
        .apps
        .iter()
        .position(|record| record.publisher_id == publisher && record.app_id.as_str() == app)
        .ok_or(StateError::InvalidState)?;
    Ok(&mut state.apps[index])
}

fn find_package(
    store: &AppStateStore,
    publisher: &str,
    app: &str,
    version: &str,
) -> Result<i2pr_app_package::InstalledPackage, StateError> {
    validate_fingerprint(publisher)?;
    let app_id = AppId::parse(app.to_owned()).map_err(|_| StateError::InvalidState)?;
    let packages = store.packages().list()?;
    packages
        .into_iter()
        .find(|package| {
            package.identity.publisher_key_id == publisher
                && package.identity.app_id == app_id
                && package.identity.version.as_str() == version
        })
        .ok_or(StateError::SelectedPackageUnavailable)
}

fn print_package(identity: &PackageIdentity) {
    println!(
        "publisher={} app={} version={} artifact_sha256={}",
        identity.publisher_key_id,
        identity.app_id.as_str(),
        identity.version.as_str(),
        identity.artifact_sha256
    );
}
fn validate_fingerprint(value: &str) -> Result<(), StateError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(StateError::InvalidState)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use i2pr_app_state::PolicyState;

    fn cli(data_dir: &std::path::Path, args: &[&str]) -> Cli {
        let mut full = vec!["i2pr-appctl", "--data-dir"];
        full.push(data_dir.to_str().unwrap());
        full.extend_from_slice(args);
        Cli::try_parse_from(full).unwrap()
    }

    #[test]
    fn trust_and_untrust_are_persisted_offline() {
        let tmp = tempfile::tempdir().unwrap();
        let fingerprint = "a".repeat(64);
        run(cli(tmp.path(), &["publisher", "trust", &fingerprint])).unwrap();
        let store = AppStateStore::open(tmp.path().join("managed-apps")).unwrap();
        let trusted = store.load().unwrap().trusted_publishers;
        assert_eq!(trusted.len(), 1);
        assert_eq!(trusted[0], fingerprint);

        run(cli(tmp.path(), &["publisher", "untrust", &fingerprint])).unwrap();
        assert!(store.load().unwrap().trusted_publishers.is_empty());
        assert_eq!(store.load().unwrap().generation, 2);
    }

    #[test]
    fn active_runtime_refuses_cli_mutation_without_a_generation_change() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path().join("managed-apps")).unwrap();
        let _runtime = store.lock_runtime().unwrap();
        let fingerprint = "b".repeat(64);
        assert!(matches!(
            run(cli(tmp.path(), &["publisher", "trust", &fingerprint],)),
            Err(StateError::RuntimeBusy)
        ));
        assert_eq!(store.load().unwrap(), PolicyState::default());
    }

    #[test]
    fn untrust_clears_latent_launch_authority_before_retrust() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AppStateStore::open(tmp.path().join("managed-apps")).unwrap();
        let fingerprint = "d".repeat(64);
        let app_id = AppId::parse("sample.app".to_owned()).unwrap();
        let version = i2pr_app_proto::AppVersion::parse("1.0".to_owned()).unwrap();
        let identity = PackageIdentity {
            publisher_key_id: fingerprint.clone(),
            app_id: app_id.clone(),
            version,
            artifact_sha256: "e".repeat(64),
        };
        store
            .mutate(|state| {
                state.trusted_publishers.push(fingerprint.clone());
                state.apps.push(AppPolicy {
                    publisher_id: fingerprint.clone(),
                    app_id: app_id.clone(),
                    selected: Some(identity.clone()),
                    granted_capabilities: vec![Capability::Sam],
                    launch_profile: Some(LaunchProfile::UnsafeDirect),
                    autostart: true,
                    max_connections: 1,
                    resource_ceilings: ResourceCeilings::default(),
                });
                Ok(())
            })
            .unwrap();
        run(cli(tmp.path(), &["publisher", "untrust", &fingerprint])).unwrap();
        let state = store.load().unwrap();
        assert!(state.trusted_publishers.is_empty());
        let app = &state.apps[0];
        assert!(app.granted_capabilities.is_empty());
        assert!(app.launch_profile.is_none());
        assert!(!app.autostart);
        assert_eq!(app.selected.as_ref(), Some(&identity));
    }

    #[test]
    fn unsafe_direct_requires_acknowledgement_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let fingerprint = "c".repeat(64);
        assert!(matches!(
            run(cli(
                tmp.path(),
                &[
                    "app",
                    "profile",
                    &fingerprint,
                    "sample.app",
                    "unsafe-direct",
                ],
            )),
            Err(StateError::UnsafeDirectAcknowledgementRequired)
        ));
        let store = AppStateStore::open(tmp.path().join("managed-apps")).unwrap();
        assert_eq!(store.load().unwrap(), PolicyState::default());
    }
}
