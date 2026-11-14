//! Plan 544's lane — the router's own RouterInfo export for an out-of-band
//! private-LAN handoff.
//!
//! The lane proves the property the peer actually depends on, which is a
//! **naming** property:
//!
//! - the exported filename is the lowercase hexadecimal hash of the record's
//!   own contents, derived from the bytes that were written and not from a
//!   parallel source;
//! - the record under that name round-trips through the peer's own
//!   [`CacheLoader`], which derives its expected key from the filename;
//! - the same bytes under any other valid name are refused, so the name is
//!   load-bearing rather than advisory;
//! - the export lands in `export/` and never in the two directories the router
//!   itself loads from, because a router that found itself in its own cache
//!   would hold its own record as a peer;
//! - the directory is `0700` and the file `0600`;
//! - a record whose hash cannot be derived is skipped and writes nothing;
//! - a second publication of the same identity rewrites the same name, so the
//!   file always matches the current record.
//!
//! The lane uses fixed seeds, an explicit clock, no socket, no environment
//! variable, no `#[ignore]`, and no sleeping. It calls the **production**
//! helper (`i2pr_daemon::export_local_router_info`) rather than a
//! re-implementation of the naming rule.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_daemon::export_local_router_info;
use i2pr_netdb::{RouterHash, RouterInfoStore, ValidationContext, router_hash};
use i2pr_netdb_persist::cache_loader::CacheLoader;
use i2pr_proto::{Date, Mapping, RouterInfo};
use i2pr_storage::cache_seam::ByteCache;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The clock the lane's records are published around.
const NOW_MS: u64 = 1_700_000_000_000;

/// One deterministic test identity.
fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

/// Encodes one signed RouterInfo published at `published_ms`.
fn encoded_record(signer: &RouterIdentityBundle, published_ms: u64) -> Vec<u8> {
    let mut options = Mapping::builder();
    options
        .insert("router.version".to_owned(), "0.9.62".to_owned())
        .expect("router.version");
    let info = signer
        .sign_router_info(
            Date::from_millis(published_ms),
            Vec::new(),
            Vec::new(),
            options.build().expect("options"),
        )
        .expect("sign");
    info.encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("encode")
}

/// The lowercase hexadecimal hash of an encoded record's own contents.
fn hash_of(encoded: &[u8]) -> (String, RouterHash) {
    let info = RouterInfo::decode(encoded, encoded.len()).expect("decode");
    let hash = router_hash(info.router_identity()).expect("router hash");
    let mut hex = String::with_capacity(64);
    for byte in hash.as_bytes() {
        hex.push_str(&format!("{byte:02x}"));
    }
    (hex, hash)
}

/// A temporary data directory at the router's own `0700` posture.
///
/// The data directory is a **child** of the temporary directory: `tempdir`
/// creates its root at the process umask (`0755`), and the storage seam refuses
/// to adopt a directory that is not already private, so the lane lets the seam
/// create the directory it owns.
fn private_data_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("temp dir");
    let state = directory.path().join("state");
    i2pr_storage::IdentityStore::prepare_directory(&state).expect("private data dir");
    (directory, state)
}

#[test]
fn the_export_name_is_the_hash_of_its_own_contents() {
    let (_root, data_dir) = private_data_dir();
    let encoded = encoded_record(&bundle(0x5440_0001), NOW_MS);

    let (name, path) = export_local_router_info(data_dir.as_path(), &encoded)
        .expect("export")
        .expect("a signed record has a derivable hash");

    let (expected, hash) = hash_of(&encoded);
    assert_eq!(name, expected, "the export name is not the record's hash");
    assert_eq!(
        path.file_name().and_then(|value| value.to_str()),
        Some(expected.as_str()),
        "the file on disk is not named after the hash"
    );
    let expected_directory = data_dir.join("export");
    assert_eq!(
        path.parent(),
        Some(expected_directory.as_path()),
        "the export did not land in export/"
    );
    assert_eq!(hash.as_bytes().len(), 32, "a router hash is 32 bytes");
}

#[test]
fn the_exported_bytes_are_the_bytes_the_router_signed() {
    let (_root, data_dir) = private_data_dir();
    let encoded = encoded_record(&bundle(0x5440_0002), NOW_MS);

    let (_, path) = export_local_router_info(data_dir.as_path(), &encoded)
        .expect("export")
        .expect("hash");

    let written = std::fs::read(&path).expect("read back");
    assert_eq!(
        written, encoded,
        "the export is not byte-identical to the signed record: the plan adds no transformation"
    );
    // The written bytes must still decode and still hash to the file name, so
    // the name describes the bytes on disk and not the bytes in memory.
    let (expected, _) = hash_of(&written);
    assert_eq!(
        path.file_name().and_then(|value| value.to_str()),
        Some(expected.as_str()),
        "the file name does not describe the bytes on disk"
    );
}

#[test]
fn the_exported_record_round_trips_through_the_peer_cache_loader() {
    let (_root, data_dir) = private_data_dir();
    let encoded = encoded_record(&bundle(0x5440_0003), NOW_MS);
    let (name, path) = export_local_router_info(data_dir.as_path(), &encoded)
        .expect("export")
        .expect("hash");

    let (_peer_root, peer) = private_data_dir();
    let cache = ByteCache::in_data_dir(peer.as_path());
    cache.prepare().expect("peer cache prepare");
    let bytes = std::fs::read(&path).expect("read exported record");
    cache.write(&name, &bytes).expect("peer cache write");

    let loader = CacheLoader::new(cache);
    let mut store = RouterInfoStore::default();
    let report = loader
        .load_into(
            &mut store,
            ValidationContext::new(Date::from_millis(NOW_MS)),
        )
        .expect("peer loader runs");

    assert_eq!(
        report.entries_inspected, 1,
        "the peer inspected a different number of entries"
    );
    assert_eq!(report.invalid, 0, "the peer refused the exported record");
    assert_eq!(report.unreadable, 0, "the peer could not read the export");
    assert_eq!(report.inserted, 1, "the peer did not insert the record");
    assert_eq!(store.len(), 1, "the peer's store is not holding the record");

    let (_, hash) = hash_of(&bytes);
    assert!(
        store.contains(&hash),
        "the peer's store does not hold the record under its own hash"
    );
    assert_eq!(
        store.get(&hash).expect("record").key(),
        hash,
        "the stored record's key is not the name-derived hash"
    );
}

#[test]
fn the_peer_refuses_the_same_record_under_any_other_name() {
    let (_root, data_dir) = private_data_dir();
    let encoded = encoded_record(&bundle(0x5440_0004), NOW_MS);
    let (name, path) = export_local_router_info(data_dir.as_path(), &encoded)
        .expect("export")
        .expect("hash");

    // A different, still syntactically valid, 64-lowercase-hex name. The peer's
    // loader derives its expected key from this name, so the record's identity
    // no longer matches and the entry is refused. This is why the export may
    // not pick a readable name and leave the rename to the operator.
    let mut wrong: Vec<u8> = name.clone().into_bytes();
    wrong[0] = if wrong[0] == b'0' { b'1' } else { b'0' };
    let wrong = String::from_utf8(wrong).expect("hex name");

    let (_peer_root, peer) = private_data_dir();
    let cache = ByteCache::in_data_dir(peer.as_path());
    cache.prepare().expect("peer cache prepare");
    let bytes = std::fs::read(&path).expect("read exported record");
    cache.write(&wrong, &bytes).expect("peer cache write");

    let loader = CacheLoader::new(cache);
    let mut store = RouterInfoStore::default();
    let report = loader
        .load_into(
            &mut store,
            ValidationContext::new(Date::from_millis(NOW_MS)),
        )
        .expect("peer loader runs");

    assert_eq!(
        report.invalid, 1,
        "the peer accepted a record whose name is not its hash"
    );
    assert_eq!(report.inserted, 0, "the peer inserted a misnamed record");
    assert!(store.is_empty(), "the peer's store is not empty");
}

#[test]
fn the_export_never_lands_in_the_directories_the_router_itself_loads() {
    let (_root, data_dir) = private_data_dir();
    let encoded = encoded_record(&bundle(0x5440_0005), NOW_MS);
    let (name, _) = export_local_router_info(data_dir.as_path(), &encoded)
        .expect("export")
        .expect("hash");

    // `netdb/routers/` is the cache the router loads into its **own** store at
    // startup (`crates/i2pr-storage/src/lib.rs`, `ByteCache::in_data_dir`). A
    // record written there would be re-ingested by the router that produced
    // it.
    assert!(
        !data_dir
            .as_path()
            .join("netdb")
            .join("routers")
            .join(&name)
            .exists(),
        "the export landed in the router's own load path"
    );
    assert!(
        !data_dir.as_path().join("netdb").exists(),
        "the export created the router's own NetDB directory"
    );
    assert!(
        data_dir.as_path().join("export").join(&name).exists(),
        "the export is not in export/"
    );
}

#[test]
fn the_export_is_written_private() {
    use std::os::unix::fs::PermissionsExt;

    let (_root, data_dir) = private_data_dir();
    let encoded = encoded_record(&bundle(0x5440_0006), NOW_MS);
    let (_, path) = export_local_router_info(data_dir.as_path(), &encoded)
        .expect("export")
        .expect("hash");

    let directory = std::fs::metadata(data_dir.as_path().join("export")).expect("directory");
    assert_eq!(
        directory.permissions().mode() & 0o777,
        0o700,
        "the export directory is not 0700"
    );
    let file = std::fs::metadata(&path).expect("file");
    assert_eq!(
        file.permissions().mode() & 0o777,
        0o600,
        "the export file is not 0600"
    );
}

#[test]
fn an_undecodable_record_is_skipped_without_writing_anything() {
    let (_root, data_dir) = private_data_dir();
    let outcome = export_local_router_info(data_dir.as_path(), b"not a router info")
        .expect("a local condition is not an error");
    assert!(outcome.is_none(), "an undecodable record produced a name");

    let export = data_dir.as_path().join("export");
    match std::fs::read_dir(&export) {
        Ok(mut entries) => assert!(
            entries.next().is_none(),
            "an undecodable record wrote a file anyway"
        ),
        Err(error) => assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "the export directory is unusable: {error}"
        ),
    }
}

#[test]
fn a_second_publication_of_the_same_identity_rewrites_the_same_name() {
    let (_root, data_dir) = private_data_dir();
    let signer = bundle(0x5440_0007);
    let first = encoded_record(&signer, NOW_MS);
    let (first_name, first_path) = export_local_router_info(data_dir.as_path(), &first)
        .expect("export")
        .expect("hash");

    // Same identity, a later publication: same name, new bytes. The export is
    // rewritten on each publication, so the file always matches the record in
    // force.
    let second = encoded_record(&signer, NOW_MS + 60_000);
    let (second_name, second_path) = export_local_router_info(data_dir.as_path(), &second)
        .expect("export")
        .expect("hash");

    assert_eq!(
        first_name, second_name,
        "the same identity produced two export names"
    );
    assert_eq!(first_path, second_path, "the export path moved");
    let written = std::fs::read(&second_path).expect("read back");
    assert_eq!(
        written, second,
        "the export still holds the superseded record"
    );
}
