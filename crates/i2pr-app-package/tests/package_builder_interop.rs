use std::{fs, os::unix::fs::PermissionsExt};

use ed25519_dalek::SigningKey;
use i2pr_app_package::{PackageStore, verify_file};
use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn public_builder_output_passes_the_router_canonical_verifier() {
    let temp = tempfile::tempdir().unwrap();
    let payload = temp.path().join("payload");
    fs::create_dir(&payload).unwrap();
    let entrypoint = payload.join("main");
    fs::write(&entrypoint, b"managed app fixture").unwrap();
    fs::set_permissions(&entrypoint, fs::Permissions::from_mode(0o755)).unwrap();

    let signing = SigningKey::from_bytes(&[31; 32]);
    let publisher = hex(&Sha256::digest(signing.verifying_key().to_bytes()));
    let manifest = format!(
        r#"{{"schema_version":1,"app_id":"external.fixture","publisher_id":"{publisher}","version":"1.0.0","name":"External fixture","description":"","host_protocol_min":{{"major":1,"minor":0}},"host_protocol_max":{{"major":1,"minor":1}},"entrypoints":[{{"target":"x86_64-unknown-linux-gnu","path":"main"}}],"requested_capabilities":[],"resources":[],"ui":null,"autostart_requested":false,"restart_requested":false}}"#
    );
    let archive = temp.path().join("fixture.i2prapp");
    i2pr_app_package_build::build_package(manifest.as_bytes(), &payload, &archive, &signing)
        .unwrap();
    let verified = verify_file(&archive).unwrap();
    assert_eq!(verified.manifest.app_id.as_str(), "external.fixture");
    assert_eq!(verified.inventory.len(), 1);
    assert_eq!(verified.inventory[0].path, "main");
    let store = PackageStore::open(temp.path().join("store")).unwrap();
    let installed = store.install(&archive).unwrap();
    assert_eq!(installed.identity, verified.identity);
    assert_eq!(store.list().unwrap().len(), 1);
}
