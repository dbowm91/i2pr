//! Plan 369 §G — the fixture **manager**.
//!
//! This is the evidence-tooling counterpart to the shipped `i2pr-appd` binary.
//! It runs the *real* manager (`i2pr_appd::Appd`) over the *real* inherited pipe
//! transport, differing from production in exactly one respect: it owns a
//! [`i2pr_appd::LaunchCatalog`] that yields a prevalidated authority, where the shipped
//! binary owns [`EmptyCatalog`](i2pr_appd::EmptyCatalog) and therefore launches
//! nothing.
//!
//! # Why this is a separate binary and not a flag
//!
//! `i2pr-appd` refuses every argument, and Plan 369 §4 forbids a
//! user-configurable command. If this were a flag on the shipped manager, then
//! `--scenario` plus a catalog file would be a way for an operator — or anything
//! that can reach the daemon's process table — to make the router exec an
//! arbitrary file. Keeping the authority source in a binary that no production
//! code path can name is what preserves "a Plan 369 router cannot be talked into
//! starting an application". `scripts/check-managed-app-process-boundary.py`
//! asserts exactly that, and negative-tests the assertion.
//!
//! # Where the paths come from
//!
//! The apphost is resolved by `i2pr-appd`'s own sibling rule, unchanged, because
//! that rule is part of what is under test. The fixture application is likewise a
//! sibling of this binary. Neither path is an argument, so no argument can
//! redirect either exec.

use std::collections::BTreeMap;
use std::process::ExitCode;

use i2pr_app_fixture::FixtureArgs;
use i2pr_app_manager_proto::apphost::{
    DescriptiveResourceRequest, Entrypoint, LaunchRoot, SanitizedEnvironment,
};
use i2pr_app_manager_proto::{ManagerInstanceId, ManagerPrincipal};
use i2pr_app_proto::{AdministratorPrincipal, AppId, Capability, LaunchProfile, MAX_STREAMS};

/// The fixture application's file name, resolved as a sibling of this binary.
///
/// A constant rather than an argument for the reason in the module docs: an
/// argument here would be a second way to choose what gets exec'd.
const FIXTURE_APP_NAME: &str = "i2pr-app-fixture";

/// Gateway connections each fixture launch may hold.
///
/// One is enough for every scenario except the isolation pair, and a low bound
/// keeps a fixture that misbehaves from consuming a real backend connection
/// pool.
const FIXTURE_MAX_CONNECTIONS: u32 = 2;

/// Ceiling on instances one fixture manager may launch.
///
/// Two is the multi-instance isolation case; more would only exercise the
/// manager's own instance ceiling, which is a different test.
const FIXTURE_MAX_INSTANCES: usize = 2;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let request = match FixtureRequest::parse(argv) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("i2pr-app-fixture-manager: {error}");
            return ExitCode::from(2);
        }
    };

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("i2pr-app-fixture-manager: could not start a runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(drive(request)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("i2pr-app-fixture-manager: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn drive(request: FixtureRequest) -> Result<(), String> {
    let app_path = sibling_fixture_app()?;
    let root = app_path
        .parent()
        .ok_or("the fixture application has no parent directory")?;
    let entrypoint = Entrypoint::new(FIXTURE_APP_NAME)
        .map_err(|error| format!("fixture entrypoint rejected: {error}"))?;

    let catalog = FixtureCatalog {
        app_root: LaunchRoot::new(root.to_str().ok_or("fixture root is not valid UTF-8")?)
            .map_err(|error| format!("fixture root rejected: {error}"))?,
        entrypoint,
        capabilities: request.capabilities,
        instances: request.instances,
        first_instance: request.first_instance,
        app_args: request.app_args,
        yielded: std::sync::atomic::AtomicUsize::new(0),
    };

    // The real manager, over the real inherited transport. Only the catalog
    // differs from the shipped binary.
    let mut appd = i2pr_appd::Appd::with_catalog(catalog);
    appd.run(i2pr_appd::inherited())
        .await
        .map_err(|error| format!("manager stopped: {error}"))
}

/// Resolves the fixture application next to this binary.
fn sibling_fixture_app() -> Result<std::path::PathBuf, String> {
    let executable =
        std::env::current_exe().map_err(|_| "could not determine the manager path".to_owned())?;
    let directory = executable
        .parent()
        .ok_or("the manager has no parent directory".to_owned())?;
    let candidate = directory.join(FIXTURE_APP_NAME);
    if !candidate.is_file() {
        return Err(format!(
            "the fixture application {} is missing; build the workspace so both \
             {FIXTURE_APP_NAME} and i2pr-apphost sit beside the manager",
            candidate.display()
        ));
    }
    Ok(candidate)
}

/// A bounded, non-blocking authority source for the fixture.
///
/// `next_launch` takes `&self` because a catalog must not block the manager body,
/// so the launch counter is an atomic rather than a `&mut` field. It is bounded
/// twice over: by the requested instance count and by [`FIXTURE_MAX_INSTANCES`],
/// so a misconfigured request cannot become a launch storm.
struct FixtureCatalog {
    app_root: LaunchRoot,
    entrypoint: Entrypoint,
    capabilities: Vec<Capability>,
    instances: usize,
    first_instance: u128,
    app_args: FixtureArgs,
    yielded: std::sync::atomic::AtomicUsize,
}

impl i2pr_appd::LaunchCatalog for FixtureCatalog {
    fn next_launch(&self) -> Option<i2pr_appd::authority::LaunchAuthority> {
        use std::sync::atomic::Ordering;

        let ceiling = self.instances.min(FIXTURE_MAX_INSTANCES);
        // `fetch_add` claims a slot; a claim past the ceiling is released
        // immediately so a rejected call cannot drift the counter upward and
        // starve the launches that follow it.
        if self.yielded.fetch_add(1, Ordering::SeqCst) >= ceiling {
            self.yielded.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        let index = self.yielded.load(Ordering::SeqCst) - 1;
        let instance = self.first_instance + index as u128;
        let mut app_args = self.app_args.clone();
        app_args.instance = instance;
        build_authority(
            &self.app_root,
            &self.entrypoint,
            &self.capabilities,
            app_args,
            instance,
        )
        .ok()
    }
}

/// The application id every fixture launch declares.
const FIXTURE_APP_ID: &str = "i2pr.fixture.app";

/// Builds one trusted, prevalidated launch authority.
///
/// This is the *only* place a fixture authority is constructed, and it goes
/// through the same `LaunchAuthority::new` gate production would: the
/// administrator grant path, the canonical instance id, the gateway limit, and
/// the whole bootstrap contract (`Secured` refusal, entrypoint containment,
/// argv bounds) all run exactly as they do for a real launch. A fixture that
/// could bypass that gate would qualify nothing.
fn build_authority(
    root: &LaunchRoot,
    entrypoint: &Entrypoint,
    capabilities: &[Capability],
    app_args: FixtureArgs,
    instance: u128,
) -> Result<i2pr_appd::authority::LaunchAuthority, String> {
    let administrator = AdministratorPrincipal::from_authenticated_session(1)
        .map_err(|error| format!("administrator principal: {error:?}"))?;
    let request = i2pr_appd::authority::AuthorityRequest {
        principal: ManagerPrincipal {
            app_id: AppId::parse(FIXTURE_APP_ID).map_err(|e| format!("app id: {e:?}"))?,
            instance_id: ManagerInstanceId::new(instance),
            publisher_id: None,
        },
        capabilities: capabilities.to_vec(),
        // `Secured` is refused at authority construction because no backend is
        // qualified; a fixture that asked for it would prove nothing, and would
        // fail here rather than at exec.
        launch_profile: LaunchProfile::UnsafeDirect,
        root: root.clone(),
        entrypoint: entrypoint.clone(),
        argv: app_args.to_argv(),
        environment: SanitizedEnvironment::new(BTreeMap::new())
            .map_err(|e| format!("sanitized environment: {e:?}"))?,
        resources: DescriptiveResourceRequest {
            // Descriptive only in Plan 369: nothing enforces these yet, and a
            // fixture must not imply that it does.
            requested_memory_bytes: 0,
            requested_open_files: 0,
        },
        max_connections: FIXTURE_MAX_CONNECTIONS,
    };
    i2pr_appd::authority::LaunchAuthority::new(&administrator, request)
        .map_err(|error| format!("fixture authority refused: {error}"))
}

/// The fixture manager's own argument grammar.
///
/// Closed, like the application's: anything not named here is refused rather
/// than ignored. It is a superset of the application's grammar because the
/// manager additionally chooses *what* to launch and *with which grants*.
struct FixtureRequest {
    capabilities: Vec<Capability>,
    instances: usize,
    first_instance: u128,
    /// The application's full argument set, including its transcript path.
    app_args: FixtureArgs,
}

impl FixtureRequest {
    fn parse(argv: Vec<String>) -> Result<Self, String> {
        let mut capabilities: Option<Vec<Capability>> = None;
        let mut instances: Option<usize> = None;
        let mut instance: Option<u128> = None;
        let mut transcript: Option<String> = None;
        let mut app_argv: Vec<String> = Vec::new();

        for argument in argv {
            let Some((key, value)) = argument.split_once('=') else {
                return Err(format!("unsupported argument: {argument}"));
            };
            match key {
                "--capabilities" => {
                    if capabilities.is_some() {
                        return Err("--capabilities was supplied twice".to_owned());
                    }
                    capabilities = Some(parse_capabilities(value)?);
                }
                "--instances" => {
                    if instances.is_some() {
                        return Err("--instances was supplied twice".to_owned());
                    }
                    instances = Some(parse_count("--instances", value)?);
                }
                "--instance" => {
                    if instance.is_some() {
                        return Err("--instance was supplied twice".to_owned());
                    }
                    instance = Some(
                        value
                            .parse::<u128>()
                            .map_err(|_| format!("--instance is not a number: {value}"))?,
                    );
                }
                "--transcript" => {
                    if transcript.is_some() {
                        return Err("--transcript was supplied twice".to_owned());
                    }
                    transcript = Some(value.to_owned());
                }
                // Passed through to the application unchanged; the application
                // parses them with its own closed grammar.
                "--scenario" | "--app-id" => app_argv.push(argument),
                other => return Err(format!("unsupported argument: {other}={value}")),
            }
        }

        let transcript = transcript.ok_or("the fixture manager requires --transcript=<path>")?;
        let first_instance = instance.ok_or("the fixture manager requires --instance=<value>")?;
        if first_instance == 0 {
            return Err("--instance must be nonzero".to_owned());
        }
        // The application's own argv is completed here rather than being passed
        // in: the instance id is a manager-created value, and the catalog
        // re-derives it per launch, so accepting an application-supplied one
        // would let a launch name an identity the manager never created.
        app_argv.push(format!("--instance={first_instance}"));
        app_argv.push(format!("--transcript={transcript}"));
        let app_args = FixtureArgs::parse(app_argv).map_err(|error| error.to_string())?;

        Ok(FixtureRequest {
            capabilities: capabilities.unwrap_or_else(|| vec![Capability::Sam, Capability::I2cp]),
            instances: instances.unwrap_or(1),
            first_instance,
            app_args,
        })
    }
}

/// Parses `sam,i2cp` into capabilities.
///
/// An unknown name is an error rather than a skipped entry: a capability the
/// fixture silently dropped would turn a "request something you were not
/// granted" case into a "request something that was granted" one.
fn parse_capabilities(value: &str) -> Result<Vec<Capability>, String> {
    let mut parsed = Vec::new();
    for name in value.split(',').filter(|name| !name.is_empty()) {
        let capability = match name {
            "sam" => Capability::Sam,
            "i2cp" => Capability::I2cp,
            other => return Err(format!("unknown fixture capability: {other}")),
        };
        if parsed.contains(&capability) {
            return Err(format!("duplicate fixture capability: {name}"));
        }
        parsed.push(capability);
    }
    if parsed.is_empty() {
        return Err("--capabilities named no capability".to_owned());
    }
    Ok(parsed)
}

/// Parses a small positive count, bounded by the fixture's own ceiling.
fn parse_count(name: &str, value: &str) -> Result<usize, String> {
    let count: usize = value
        .parse()
        .map_err(|_| format!("{name} is not a number: {value}"))?;
    if count == 0 || count > FIXTURE_MAX_INSTANCES {
        return Err(format!(
            "{name} must be 1..={FIXTURE_MAX_INSTANCES}, got {count}"
        ));
    }
    Ok(count)
}

/// Keeps the stream ceiling reachable from this file's evidence vocabulary.
///
/// A fixture that silently opened more streams than the contract allows would
/// qualify the wrong thing, so the bound is named here rather than only in the
/// product crate.
#[allow(dead_code)]
const fn fixture_stays_under_the_stream_ceiling() -> usize {
    MAX_STREAMS
}
