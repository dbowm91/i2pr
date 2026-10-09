//! Plan 369 §9 — the trusted source of launch authority.
//!
//! # Why this is a trait with one production implementation
//!
//! Launch authority must be *manager-created*: something inside the trusted
//! manager has to decide, on its own evidence, that one application may be
//! launched with one set of capabilities. Plans 382–383 add this authority
//! source: it loads explicit offline policy, verifies the exact installed
//! package again, and only then assembles a launch authority.
//!
//! The manager protocol still has no manager-receivable launch request, so the
//! daemon cannot ask for an executable or create authority through the wire.
//! Only the production catalog can bridge validated local policy into the
//! sealed [`LaunchAuthority`] type. See [`crate::authority`].
//!
//! # Where the other implementations come from
//!
//! `tests/` and later plans supply their own. WP5's fixture manager is a
//! **separate binary target** rather than a config field, so a production
//! `i2pr-appd` can never be made to launch an operator-supplied executable.

use std::{collections::VecDeque, path::PathBuf, sync::Mutex};

use i2pr_app_manager_proto::apphost::{
    AppDataRoot, DescriptiveResourceRequest, Entrypoint, LaunchRoot, SanitizedEnvironment,
};
use i2pr_app_manager_proto::{ManagerInstanceId, ManagerPrincipal};
use i2pr_app_proto::{AdministratorPrincipal, AppInstanceId, PublisherId};
use i2pr_app_state::{AppStateStore, LaunchDecision, RuntimeLock, target_triple};
use rand_core::{OsRng, TryRngCore};

use crate::{
    AppdError,
    authority::{AuthorityRequest, LaunchAuthority},
};

/// A bounded source of launch authorities.
///
/// `next_launch` is called from the manager body, not from a task. It returns
/// `None` when the catalog has no more launches. The production catalog yields
/// each validated operator-approved autostart once per appd lifetime; app exit
/// does not cause a relaunch.
pub trait LaunchCatalog: Send + Sync {
    fn next_launch(&self) -> Result<Option<LaunchAuthority>, AppdError>;
}

/// Empty test catalog. Production uses [`PersistentLaunchCatalog`].
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyCatalog;

impl LaunchCatalog for EmptyCatalog {
    fn next_launch(&self) -> Result<Option<LaunchAuthority>, AppdError> {
        Ok(None)
    }
}

struct LoadedCatalog {
    // Keep the runtime lock alive for the complete appd lifetime.
    _runtime_lock: RuntimeLock,
    launches: VecDeque<LaunchAuthority>,
}

/// Production catalog loaded lazily after the inherited manager handshake.
/// It holds one immutable policy snapshot and the runtime lock for the appd
/// lifetime. Individual invalid applications are skipped so siblings proceed.
pub struct PersistentLaunchCatalog {
    loaded: Mutex<Option<Result<LoadedCatalog, AppdError>>>,
}

impl PersistentLaunchCatalog {
    pub fn new() -> Self {
        Self {
            loaded: Mutex::new(None),
        }
    }

    fn initialize(&self) -> Result<LoadedCatalog, AppdError> {
        let raw_root = std::env::var_os("I2PR_APP_STATE_ROOT").ok_or(AppdError::Catalog)?;
        self.initialize_root(PathBuf::from(raw_root))
    }

    fn initialize_root(&self, root: PathBuf) -> Result<LoadedCatalog, AppdError> {
        if !root.is_absolute() || !root.is_dir() {
            return Err(AppdError::Catalog);
        }
        let store = AppStateStore::open(&root).map_err(|_| AppdError::Catalog)?;
        let runtime_lock = store.lock_runtime().map_err(|_| AppdError::Catalog)?;
        let snapshot = store.load().map_err(|_| AppdError::Catalog)?;
        let target = target_triple();
        let mut launches = VecDeque::new();
        let mut used_ids = std::collections::BTreeSet::new();
        for app in snapshot.apps.iter().filter(|app| app.autostart) {
            let Some(target) = target else { continue };
            let decision = match store.resolve(&snapshot, app, target) {
                Ok(decision) => decision,
                Err(error) => {
                    eprintln!(
                        "i2pr-appd: one autostart application was refused by local policy ({error})"
                    );
                    continue;
                }
            };
            match authority_for_decision(decision, &mut used_ids) {
                Ok(authority) => launches.push_back(authority),
                Err(LaunchBuildError::Refused(error)) => {
                    eprintln!(
                        "i2pr-appd: one autostart application could not receive launch authority ({error})"
                    );
                    continue;
                }
                Err(LaunchBuildError::Rng) => return Err(AppdError::Catalog),
            }
        }
        Ok(LoadedCatalog {
            _runtime_lock: runtime_lock,
            launches,
        })
    }
}

impl Default for PersistentLaunchCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl LaunchCatalog for PersistentLaunchCatalog {
    fn next_launch(&self) -> Result<Option<LaunchAuthority>, AppdError> {
        let mut loaded = self.loaded.lock().map_err(|_| AppdError::Catalog)?;
        if loaded.is_none() {
            *loaded = Some(self.initialize());
        }
        let catalog = loaded
            .as_mut()
            .ok_or(AppdError::Catalog)?
            .as_mut()
            .map_err(|_| AppdError::Catalog)?;
        Ok(catalog.launches.pop_front())
    }
}

fn authority_for_decision(
    decision: LaunchDecision,
    used_ids: &mut std::collections::BTreeSet<u128>,
) -> Result<LaunchAuthority, LaunchBuildError> {
    let instance_id = fresh_instance_id(&mut OsRng, used_ids)?;
    let app_instance_id = AppInstanceId::new(instance_id)
        .map_err(|_| LaunchBuildError::Refused("invalid random instance id"))?;
    let principal = ManagerPrincipal {
        app_id: decision.package.manifest.app_id.clone(),
        instance_id: ManagerInstanceId::from(&app_instance_id),
        publisher_id: Some(
            PublisherId::parse(decision.package.identity.publisher_key_id.clone())
                .map_err(|_| LaunchBuildError::Refused("invalid publisher identity"))?,
        ),
    };
    let payload_root = decision
        .package
        .path
        .join("payload")
        .canonicalize()
        .map_err(|_| LaunchBuildError::Refused("package payload root unavailable"))?;
    let root = LaunchRoot::new(
        payload_root
            .to_str()
            .ok_or(LaunchBuildError::Refused(
                "package path is not representable",
            ))?
            .to_owned(),
    )
    .map_err(|_| LaunchBuildError::Refused("package root rejected"))?;
    let data_root = AppDataRoot::new(
        decision
            .data_root
            .to_str()
            .ok_or(LaunchBuildError::Refused(
                "application data path is not representable",
            ))?
            .to_owned(),
    )
    .map_err(|_| LaunchBuildError::Refused("application data root rejected"))?;
    let entrypoint = Entrypoint::new(decision.entrypoint)
        .map_err(|_| LaunchBuildError::Refused("package entrypoint rejected"))?;
    let administrator = AdministratorPrincipal::from_trusted_local_policy(decision.generation)
        .map_err(|_| LaunchBuildError::Refused("policy generation rejected"))?;
    let environment = SanitizedEnvironment::new(std::collections::BTreeMap::new())
        .map_err(|_| LaunchBuildError::Refused("application environment rejected"))?;
    let request = AuthorityRequest {
        principal,
        capabilities: decision.capabilities,
        launch_profile: decision.launch_profile,
        root,
        data_root,
        entrypoint,
        // Managed-app v1's first application message must declare the exact
        // AppId and per-launch instance id. These two reserved arguments are
        // the launch context; package contents cannot supply or override them.
        argv: vec![
            format!(
                "--i2pr-app-id={}",
                decision.package.manifest.app_id.as_str()
            ),
            format!("--i2pr-app-instance={}", instance_id),
        ],
        environment,
        resources: DescriptiveResourceRequest {
            requested_memory_bytes: decision.memory_bytes,
            requested_open_files: decision.open_files,
        },
        max_connections: decision.max_connections,
    };
    LaunchAuthority::new(&administrator, request)
        .map_err(|_| LaunchBuildError::Refused("manager launch authority rejected"))
}

fn fresh_instance_id(
    rng: &mut impl TryRngCore,
    used_ids: &mut std::collections::BTreeSet<u128>,
) -> Result<u128, LaunchBuildError> {
    for _ in 0..16 {
        let mut bytes = [0_u8; 16];
        rng.try_fill_bytes(&mut bytes)
            .map_err(|_| LaunchBuildError::Rng)?;
        let value = u128::from_be_bytes(bytes);
        if value != 0 && used_ids.insert(value) {
            return Ok(value);
        }
    }
    Err(LaunchBuildError::Rng)
}

#[derive(Debug)]
enum LaunchBuildError {
    Rng,
    Refused(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, io::Write};

    use ed25519_dalek::{Signer, SigningKey};
    use i2pr_app_proto::{AppId, Capability, LaunchProfile};
    use i2pr_app_state::{AppPolicy, ResourceCeilings};
    use sha2::{Digest, Sha256};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn signed_package(path: &std::path::Path) -> (String, AppId) {
        let signing = SigningKey::from_bytes(&[41; 32]);
        let key = signing.verifying_key().to_bytes();
        let publisher = sha256_hex(&key);
        let app_id = AppId::parse("catalog.fixture").unwrap();
        let manifest = serde_json::json!({
            "schema_version": 1,
            "app_id": app_id,
            "publisher_id": publisher,
            "version": "1.0.0",
            "name": "catalog fixture",
            "description": "",
            "host_protocol_min": {"major": 1, "minor": 0},
            "host_protocol_max": {"major": 1, "minor": 0},
            "entrypoints": [{"target": i2pr_app_state::target_triple().unwrap(), "path": "bin/app"}],
            "requested_capabilities": ["sam", "i2cp"],
            "resources": [],
            "ui": null,
            "autostart_requested": false,
            "restart_requested": true
        });
        let manifest = serde_json::to_vec(&manifest).unwrap();
        let inventory = serde_json::json!([{
            "path": "bin/app",
            "size": 4,
            "sha256": sha256_hex(b"app!"),
            "executable": true
        }]);
        let inventory = serde_json::to_vec(&inventory).unwrap();
        let mut transcript = b"I2PR-APP-PACKAGE-V1\0".to_vec();
        transcript.extend_from_slice(&(manifest.len() as u32).to_be_bytes());
        transcript.extend_from_slice(&manifest);
        transcript.extend_from_slice(&(inventory.len() as u32).to_be_bytes());
        transcript.extend_from_slice(&inventory);
        transcript.extend_from_slice(&key);
        let signature = signing.sign(&transcript).to_bytes();
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("manifest.json", manifest.as_slice()),
            ("inventory.json", inventory.as_slice()),
            ("publisher.ed25519", key.as_slice()),
            ("signature.ed25519", signature.as_slice()),
            ("payload/bin/app", b"app!".as_slice()),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        (publisher, app_id)
    }

    fn install_and_authorize(root: &std::path::Path) -> (String, AppId) {
        let archive = root.with_extension("i2prapp");
        let (publisher, app_id) = signed_package(&archive);
        let store = AppStateStore::open(root).unwrap();
        let package = store.install_package(&archive).unwrap();
        store
            .mutate(|state| {
                state.trusted_publishers.push(publisher.clone());
                state.apps.push(AppPolicy {
                    publisher_id: publisher.clone(),
                    app_id: app_id.clone(),
                    selected: Some(package.identity),
                    granted_capabilities: vec![Capability::Sam, Capability::I2cp],
                    launch_profile: Some(LaunchProfile::UnsafeDirect),
                    autostart: true,
                    max_connections: 8,
                    resource_ceilings: ResourceCeilings::default(),
                });
                state.apps.sort_by(|a, b| {
                    (&a.publisher_id, &a.app_id).cmp(&(&b.publisher_id, &b.app_id))
                });
                state.trusted_publishers.sort();
                Ok(())
            })
            .unwrap();
        let _ = std::fs::remove_file(archive);
        (publisher, app_id)
    }

    struct FixedRng([u8; 16]);

    impl TryRngCore for FixedRng {
        type Error = std::io::Error;

        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            Ok(u32::from_be_bytes(
                self.0[..4].try_into().expect("four bytes"),
            ))
        }

        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            Ok(u64::from_be_bytes(
                self.0[..8].try_into().expect("eight bytes"),
            ))
        }

        fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
            for (index, byte) in destination.iter_mut().enumerate() {
                *byte = self.0[index % self.0.len()];
            }
            Ok(())
        }
    }

    struct FailedRng;

    impl TryRngCore for FailedRng {
        type Error = std::io::Error;

        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            Err(std::io::Error::other("injected RNG failure"))
        }

        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            Err(std::io::Error::other("injected RNG failure"))
        }

        fn try_fill_bytes(&mut self, _destination: &mut [u8]) -> Result<(), Self::Error> {
            Err(std::io::Error::other("injected RNG failure"))
        }
    }

    #[test]
    fn the_production_catalog_yields_nothing_and_claims_no_authority() {
        let catalog = EmptyCatalog;
        assert!(catalog.next_launch().unwrap().is_none());
        assert!(catalog.next_launch().unwrap().is_none());
    }

    #[test]
    fn a_catalog_is_usable_as_a_trait_object() {
        let catalog: Box<dyn LaunchCatalog> = Box::new(EmptyCatalog);
        assert!(catalog.next_launch().unwrap().is_none());
    }

    #[test]
    fn instance_ids_are_nonzero_unique_and_random_failure_is_fatal() {
        let mut used = std::collections::BTreeSet::new();
        let first = fresh_instance_id(&mut OsRng, &mut used).unwrap();
        let second = fresh_instance_id(&mut OsRng, &mut used).unwrap();
        assert_ne!(first, 0);
        assert_ne!(second, 0);
        assert_ne!(first, second);

        assert!(matches!(
            fresh_instance_id(&mut FailedRng, &mut used),
            Err(LaunchBuildError::Rng)
        ));
        used.insert(1);
        let mut repeated = [0_u8; 16];
        repeated[15] = 1;
        assert!(matches!(
            fresh_instance_id(&mut FixedRng(repeated), &mut used),
            Err(LaunchBuildError::Rng)
        ));
    }

    #[test]
    fn persistent_catalog_reloads_autostart_policy_with_fresh_instance_authority() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("managed-apps");
        let (publisher, app_id) = install_and_authorize(&root);

        let first_catalog = PersistentLaunchCatalog::new();
        let first_loaded = first_catalog.initialize_root(root.clone()).unwrap();
        let first = first_loaded.launches.front().unwrap();
        assert_eq!(first.principal().app_id, app_id);
        assert_eq!(
            first.principal().publisher_id.as_ref().unwrap().as_str(),
            publisher
        );
        assert!(first.permits(Capability::Sam));
        assert!(first.permits(Capability::I2cp));
        assert!(!first.permits(Capability::BrokeredTcp));
        assert_eq!(first.launch_profile(), LaunchProfile::UnsafeDirect);
        let first_instance = first.instance_id().get();
        drop(first_loaded);

        let second_catalog = PersistentLaunchCatalog::new();
        let second_loaded = second_catalog.initialize_root(root).unwrap();
        let second = second_loaded.launches.front().unwrap();
        assert_ne!(first_instance, second.instance_id().get());
        assert!(second.instance_id().get() != 0);
    }
}
