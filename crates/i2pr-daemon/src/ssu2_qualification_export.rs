//! Plan 476 local qualification receipt export.
//!
//! The Plan 480 observer and the Plan 480 collector are in-process
//! only: the observer is an in-memory event queue and the collector is
//! a pure projection with no serialization. A guarded *per-host process
//! executor* runs as a separate process, so it needs a readout channel
//! that cannot be a second observer and cannot be a socket.
//!
//! This module is that channel. It owns exactly one thing: a
//! sanitized, count-only projection of the local observer epoch,
//! written to an operator-designated file. It holds no key, no
//! RouterInfo, no address, no endpoint, no payload, and no transient
//! message ID — the collector's projection is already incapable of
//! carrying them, and this module adds nothing to it.
//!
//! Posture:
//!
//! * The destination is operator-designated and must be an absolute
//!   path whose parent directory already exists. The file is created
//!   with mode `0600`.
//! * The export is opt-in twice over: the daemon only builds an
//!   exporter when `[ssu2] qualification_receipt_path` is configured
//!   under an explicit qualification profile, and the exporter itself
//!   refuses revoked consent.
//! * A failed collection is written as a failure, not hidden: the
//!   exported `error` field carries the collector's refusal code and
//!   the consumer must fail closed on it.
//! * `live_qualification` is always `"not-run"`. A local export is not
//!   a two-host claim; the Plan 476 executor owns that claim.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use i2pr_runtime::Ssu2QualificationObserver;

use crate::ssu2_qualification_receipts::{
    LOCAL_PHASES, LocalObservedReceipt, LocalQualificationPermit, LocalQualificationProfile,
    LocalReceiptCollector,
};

/// Schema tag for the exported receipt document.
pub const EXPORT_SCHEMA: &str = "i2pr-plan476-local-receipt-v1";

/// Sanitized projection of the local observer epoch, written for the
/// Plan 476 per-host executor.
pub struct LocalReceiptExporter {
    collector: LocalReceiptCollector,
    destination: PathBuf,
    profile: &'static str,
    counts: BTreeMap<&'static str, u32>,
    session_observed: bool,
    product_observed: bool,
    reply_correlated: bool,
    /// A collector refusal seen at any point in this process's life.
    ///
    /// Sticky by construction: an overflowed epoch is untrustworthy for
    /// the rest of the process lifetime, so a later successful drain
    /// must not be able to make the export look clean again. The
    /// consumer fails closed on a non-null `error`, so forgetting the
    /// refusal here would silently turn an untrustworthy epoch into an
    /// apparently healthy one.
    error: Option<&'static str>,
}

impl LocalReceiptExporter {
    /// Builds an exporter for one operator-designated destination.
    ///
    /// Refuses a revoked consent, a relative destination, and a
    /// destination whose parent directory does not exist. It never
    /// creates directories: a qualification readout must not be able
    /// to build a path tree on the operator's behalf.
    pub fn new(
        permit: LocalQualificationPermit,
        destination: PathBuf,
    ) -> Result<Self, &'static str> {
        let collector = LocalReceiptCollector::new(permit)?;
        if !destination.is_absolute() {
            return Err("export-destination-not-absolute");
        }
        let parent = destination.parent().ok_or("export-destination-invalid")?;
        if !parent.is_dir() {
            return Err("export-destination-parent-missing");
        }
        let profile = match permit.profile {
            LocalQualificationProfile::ControlledLoopback => "controlled_loopback",
            LocalQualificationProfile::PrivateLan => "private_lan_qualification",
        };
        Ok(Self {
            collector,
            destination,
            profile,
            counts: BTreeMap::from([("permit_checked", 1_u32)]),
            session_observed: false,
            product_observed: false,
            reply_correlated: false,
            error: None,
        })
    }

    /// The operator-designated destination path.
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Drains one observer epoch, folds it into the running totals,
    /// and rewrites the sanitized document.
    ///
    /// An empty epoch is not a failure: it simply carried no new
    /// events. Every other collector refusal is recorded as the
    /// export's `error` field and is sticky — an overflowed epoch is
    /// untrustworthy for the rest of the process lifetime, so the
    /// export keeps saying so.
    pub fn export(&mut self, observer: &Ssu2QualificationObserver) -> Result<(), String> {
        let result = self.collector.collect(observer);
        self.record(result);
        self.write()
    }

    /// Folds one collector result into the running export.
    ///
    /// Split out from `export` so the stickiness of a refusal is
    /// testable without an observer: the observer can only be
    /// constructed inside the runtime crate, but the rule that a
    /// refusal survives a later success must not depend on that.
    fn record(&mut self, result: Result<LocalObservedReceipt, &'static str>) {
        match result {
            Ok(receipt) => self.absorb(&receipt),
            Err("receipt-empty") => {}
            Err(code) => {
                // Sticky: the refusal is never cleared by a later
                // successful drain.
                self.error = Some(code);
            }
        }
    }

    fn absorb(&mut self, receipt: &LocalObservedReceipt) {
        for (phase, count) in &receipt.phases {
            // The permit phase describes the *export*, not an
            // observation: this exporter holds a permit, so it is
            // recorded once here rather than accumulated per drain.
            if *phase == "permit_checked" {
                continue;
            }
            let entry = self.counts.entry(*phase).or_insert(0);
            *entry = entry.saturating_add(*count);
        }
        self.session_observed = self.session_observed || receipt.session_observed;
        self.product_observed = self.product_observed || receipt.product_observed;
        self.reply_correlated = self.reply_correlated || receipt.reply_correlated;
    }

    /// The sanitized document, as a JSON string.
    ///
    /// The shape is fixed and count-only. There is deliberately no
    /// field that could hold a key, a RouterInfo, an address, an
    /// endpoint, a payload, or a transient message ID.
    pub fn snapshot(&self) -> String {
        let mut document = String::from("{\n");
        document.push_str("  \"error\": ");
        match self.error {
            Some(code) => document.push_str(&format!("\"{code}\"")),
            None => document.push_str("null"),
        }
        document.push_str(",\n  \"live_qualification\": \"not-run\",\n");
        document.push_str("  \"phases\": [");
        let mut first = true;
        for phase in LOCAL_PHASES {
            let Some(count) = self.counts.get(phase) else {
                continue;
            };
            if !first {
                document.push(',');
            }
            first = false;
            document.push_str(&format!(
                "\n    {{\"phase\": \"{phase}\", \"count\": {count}}}"
            ));
        }
        if !first {
            document.push_str("\n  ");
        }
        document.push_str("],\n");
        document.push_str(&format!(
            "  \"product_observed\": {},\n",
            self.product_observed
        ));
        document.push_str(&format!(
            "  \"reply_correlated\": {},\n",
            self.reply_correlated
        ));
        document.push_str(&format!("  \"schema\": \"{EXPORT_SCHEMA}\",\n"));
        document.push_str("  \"session_observed\": ");
        document.push_str(if self.session_observed {
            "true"
        } else {
            "false"
        });
        document.push_str(",\n");
        document.push_str(&format!("  \"profile\": \"{}\"\n", self.profile));
        document.push_str("}\n");
        document
    }

    fn write(&self) -> Result<(), String> {
        let body = self.snapshot();
        // A unique temporary name: two exporters writing different
        // documents that happen to share a base name in one directory
        // must not collide on a single `.tmp` path.
        let temporary = self.destination.with_file_name(format!(
            ".{}.{}.tmp",
            self.destination
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "receipt".to_owned()),
            std::process::id()
        ));
        {
            let file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(|error| format!("export-open-failed:{error}"))?;
            // `mode` only applies when the file is created. A leftover
            // temporary from an earlier run keeps its own permissions,
            // so the mode is set explicitly rather than assumed.
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("export-permission-failed:{error}"))?;
            let mut file = file;
            file.write_all(body.as_bytes())
                .map_err(|error| format!("export-write-failed:{error}"))?;
            file.sync_all()
                .map_err(|error| format!("export-sync-failed:{error}"))?;
        }
        fs::rename(&temporary, &self.destination)
            .map_err(|error| format!("export-rename-failed:{error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_destination(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "i2pr-plan476-export-{}-{label}.json",
            std::process::id()
        ));
        path
    }

    fn permit(profile: LocalQualificationProfile, consent: bool) -> LocalQualificationPermit {
        LocalQualificationPermit { profile, consent }
    }

    #[test]
    fn revoked_consent_and_relative_destinations_are_refused() {
        let relative = PathBuf::from("relative-receipt.json");
        assert_eq!(
            LocalReceiptExporter::new(
                permit(LocalQualificationProfile::PrivateLan, false),
                relative.clone()
            )
            .err(),
            Some("permit-consent-revoked")
        );
        assert_eq!(
            LocalReceiptExporter::new(
                permit(LocalQualificationProfile::PrivateLan, true),
                relative
            )
            .err(),
            Some("export-destination-not-absolute")
        );
    }

    #[test]
    fn a_missing_parent_directory_is_refused() {
        let mut path = std::env::temp_dir();
        path.push("i2pr-plan476-absent-parent");
        path.push("receipt.json");
        assert_eq!(
            LocalReceiptExporter::new(permit(LocalQualificationProfile::PrivateLan, true), path)
                .err(),
            Some("export-destination-parent-missing")
        );
    }

    #[test]
    fn the_exported_document_is_count_only_and_holds_no_secret_material() {
        let destination = temporary_destination("shape");
        let _ = fs::remove_file(&destination);
        let exporter = LocalReceiptExporter::new(
            permit(LocalQualificationProfile::PrivateLan, true),
            destination.clone(),
        )
        .expect("exporter");
        let document = exporter.snapshot();
        for forbidden in [
            "message_id",
            "messageId",
            "peer_commitment",
            "endpoint",
            "address",
            "router_info",
            "routerInfo",
            "key",
            "payload",
            "secret",
            "0x",
        ] {
            assert!(
                !document.contains(forbidden),
                "export must not contain {forbidden}: {document}"
            );
        }
        assert!(document.contains("\"schema\": \"i2pr-plan476-local-receipt-v1\""));
        assert!(document.contains("\"live_qualification\": \"not-run\""));
        assert!(document.contains("\"profile\": \"private_lan_qualification\""));
    }

    #[test]
    fn an_empty_epoch_is_not_a_failure_but_a_refusal_is_sticky() {
        let destination = temporary_destination("sticky");
        let _ = fs::remove_file(&destination);
        let mut exporter = LocalReceiptExporter::new(
            permit(LocalQualificationProfile::ControlledLoopback, true),
            destination.clone(),
        )
        .expect("exporter");
        // An empty epoch is not a refusal: it simply carried no events.
        exporter.record(Err("receipt-empty"));
        assert!(
            exporter.error.is_none(),
            "an empty epoch must not be a refusal"
        );
        // A refusal is recorded ...
        exporter.record(Err("receipt-overflow"));
        assert_eq!(exporter.error, Some("receipt-overflow"));
        // ... and a later successful epoch must NOT clear it. An
        // overflowed epoch is untrustworthy for the rest of the
        // process lifetime; a consumer that fails closed on a
        // non-null `error` must not be able to see the epoch heal.
        exporter.record(Ok(LocalObservedReceipt {
            phases: vec![("permit_checked", 1)],
            session_observed: false,
            product_observed: false,
            reply_correlated: false,
            live_qualification: "not-run",
        }));
        assert_eq!(
            exporter.error,
            Some("receipt-overflow"),
            "a refusal must survive a later successful epoch"
        );
        assert!(
            exporter
                .snapshot()
                .contains("\"error\": \"receipt-overflow\"")
        );
        let _ = fs::remove_file(&destination);
    }

    #[test]
    fn the_exported_document_is_written_private_and_replaced_atomically() {
        let destination = temporary_destination("mode");
        let _ = fs::remove_file(&destination);
        let exporter = LocalReceiptExporter::new(
            permit(LocalQualificationProfile::PrivateLan, true),
            destination.clone(),
        )
        .expect("exporter");
        exporter.write().expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&destination)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "the exported receipt must be private");
        }
        // A second write replaces the document rather than appending,
        // and leaves no temporary behind. Only this exporter's own
        // temporary name is inspected: the temporary directory is
        // shared with the rest of the host.
        exporter.write().expect("rewrite");
        let stem = destination
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let siblings: Vec<String> = fs::read_dir(destination.parent().expect("parent"))
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains(&stem) && name.ends_with(".tmp"))
            .collect();
        assert!(
            siblings.is_empty(),
            "the export must not leave temporary files behind: {siblings:?}"
        );
        let _ = fs::remove_file(&destination);
    }
}
