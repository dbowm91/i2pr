//! Plan 295 control-plane source owners: bounded log ring, explicit ban
//! ledger, and rolling control metrics.
//!
//! Each owner is a real subsystem with bounded state, never a
//! fabrication shim:
//! - [`LogRing`] retains recent `tracing` events (INFO and above) with
//!   secret-marker redaction and hard entry/byte ceilings;
//! - [`BanLedger`] is the explicit ban owner the Plan 288 matrix
//!   requires before an authoritative empty ban set may be served;
//! - [`ControlMetrics`] owns request-independent rolling bandwidth,
//!   rate, and build-outcome windows over registered cumulative
//!   counters (O(1) per tick; no whole-router scan per request).
//!
//! The daemon publishes these owners into the inspection handles at
//! composition; the inspection plane reads them per request. Nothing
//! here touches sockets, tasks, or the filesystem.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// Maximum retained log entries (mirrors `logs.recent` `max_items`).
pub const MAX_LOG_RING_ENTRIES: usize = 256;
/// Maximum bytes of one retained log line (keeps a full snapshot
/// under the 64 KiB `logs.recent` ceiling with envelope overhead).
pub const MAX_LOG_LINE_BYTES: usize = 192;
/// Maximum control-rate entries served (matches the 8-entry wire ceiling).
pub const MAX_CONTROL_RATES: usize = 8;

/// Case-insensitive secret markers: any log line containing one has
/// its message replaced, never partially masked.
const REDACT_SUBSTRINGS: [&str; 8] = [
    "password", "passwd", "token", "secret", "seed", "auth=", "bearer", "api_key",
];
/// Case-sensitive secret markers (uppercase avoids redacting ordinary
/// words such as "private").
const REDACT_CASE_SENSITIVE: [&str; 1] = ["PRIV"];
/// Replacement for a redacted log message.
const REDACTED_MESSAGE: &str = "[redacted: secret marker]";
/// Severity gate: DEBUG and TRACE volume never enters the ring.
const RING_MAX_VERBOSITY: &str = "INFO";

/// Returns `true` when the message carries a secret marker.
fn needs_redaction(message: &str) -> bool {
    if REDACT_CASE_SENSITIVE
        .iter()
        .any(|marker| message.contains(marker))
    {
        return true;
    }
    let lowered;
    let haystack = if message.bytes().any(|byte| byte.is_ascii_uppercase()) {
        lowered = message.to_lowercase();
        lowered.as_str()
    } else {
        message
    };
    REDACT_SUBSTRINGS
        .iter()
        .any(|marker| haystack.contains(marker))
}

/// Truncates a log line to [`MAX_LOG_LINE_BYTES`] on a char boundary.
fn truncate_line(line: &str) -> String {
    if line.len() <= MAX_LOG_LINE_BYTES {
        return line.to_owned();
    }
    let mut end = MAX_LOG_LINE_BYTES;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].to_owned()
}

/// One retained log line (already redacted and truncated at record).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogLine {
    /// Severity label (`ERROR`, `WARN`, or `INFO`).
    pub level: String,
    /// `tracing` target that emitted the event.
    pub target: String,
    /// Redacted, truncated message body.
    pub message: String,
}

impl LogLine {
    /// Renders the wire form `LEVEL target: message`.
    pub fn wire(&self) -> String {
        truncate_line(&format!("{} {}: {}", self.level, self.target, self.message))
    }
}

/// Bounded redacted ring of recent log lines.
#[derive(Debug, Default)]
pub struct LogRing {
    entries: Mutex<VecDeque<LogLine>>,
    dropped: Mutex<u64>,
}

impl LogRing {
    /// Creates an empty ring.
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::new()),
            dropped: Mutex::new(0),
        }
    }

    /// Process-global ring fed by the daemon `tracing` layer.
    ///
    /// Global because the `tracing` subscriber is process-global;
    /// inspection still receives the ring through an explicit
    /// publication, never through this accessor.
    pub fn global() -> Arc<LogRing> {
        static GLOBAL: OnceLock<Arc<LogRing>> = OnceLock::new();
        GLOBAL.get_or_init(|| Arc::new(LogRing::new())).clone()
    }

    /// Records one event. Severity above INFO is dropped by policy;
    /// secret-marker messages are replaced; over-long lines are
    /// truncated; the oldest entry is evicted past capacity.
    pub fn record(&self, level: &str, target: &str, message: &str) {
        if !matches!(level, "ERROR" | "WARN" | "INFO") {
            return;
        }
        let line = LogLine {
            level: level.to_owned(),
            target: truncate_line(target),
            message: if needs_redaction(message) {
                REDACTED_MESSAGE.to_owned()
            } else {
                truncate_line(message)
            },
        };
        if let Ok(mut entries) = self.entries.lock() {
            entries.push_back(line);
            if entries.len() > MAX_LOG_RING_ENTRIES {
                entries.pop_front();
                if let Ok(mut dropped) = self.dropped.lock() {
                    *dropped = dropped.saturating_add(1);
                }
            }
        }
    }

    /// Snapshots retained lines oldest-first plus the eviction count.
    pub fn snapshot(&self) -> (Vec<LogLine>, u64) {
        let entries = self
            .entries
            .lock()
            .map(|entries| entries.iter().cloned().collect())
            .unwrap_or_default();
        let dropped = self.dropped.lock().map(|dropped| *dropped).unwrap_or(0);
        (entries, dropped)
    }

    /// Number of entries currently retained.
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }

    /// The severity gate label (documents [`RING_MAX_VERBOSITY`]).
    pub fn verbosity_gate(&self) -> &'static str {
        RING_MAX_VERBOSITY
    }
}

/// `tracing` subscriber layer feeding the process-global ring.
///
/// Installed by [`crate::initialize_logging`]; the INFO-and-above
/// gate keeps DEBUG/TRACE volume out of the bounded ring while the
/// formatted stdout path keeps its own `EnvFilter`.
#[derive(Clone, Debug)]
pub struct LogRingLayer {
    ring: Arc<LogRing>,
}

impl LogRingLayer {
    /// Wraps the ring the layer feeds.
    pub fn new(ring: Arc<LogRing>) -> Self {
        Self { ring }
    }
}

/// Bounded field dump for one event (`name=value` pairs).
#[derive(Debug, Default)]
struct FieldDump {
    output: String,
}

impl FieldDump {
    fn push(&mut self, name: &str, value: &str) {
        if !self.output.is_empty() {
            self.output.push(' ');
        }
        let _ = write!(self.output, "{name}={value}");
    }
}

impl tracing::field::Visit for FieldDump {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.push(field.name(), value);
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.push(field.name(), if value { "true" } else { "false" });
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.push(field.name(), &value.to_string());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.push(field.name(), &value.to_string());
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.push(field.name(), &value.to_string());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn core::fmt::Debug) {
        self.push(field.name(), &format!("{value:?}"));
    }
}

impl<S> tracing_subscriber::Layer<S> for LogRingLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        if *metadata.level() > tracing::Level::INFO {
            return;
        }
        let mut dump = FieldDump::default();
        event.record(&mut dump);
        let message = if dump.output.is_empty() {
            metadata.name().to_owned()
        } else {
            dump.output
        };
        self.ring
            .record(metadata.level().as_str(), metadata.target(), &message);
    }
}

/// Explicit ban owner: the authority the Plan 288 matrix requires
/// before `network.banned_peers` may answer.
///
/// No i2pr subsystem reports bans and no ban criteria exist, so the
/// ledger attests an empty set. The attestation is the owner's real
/// state, not a fabricated view: the day a subsystem reports a ban,
/// its reporting call lands here (a future plan owns that wiring).
#[derive(Debug, Default)]
pub struct BanLedger {
    banned: Mutex<Vec<String>>,
}

impl BanLedger {
    /// Creates the empty ledger.
    pub fn new() -> Self {
        Self {
            banned: Mutex::new(Vec::new()),
        }
    }

    /// Attests the current ban set (empty: no bans reported, none
    /// possible without a reporting subsystem).
    pub fn attested(&self) -> Vec<String> {
        self.banned
            .lock()
            .map(|banned| banned.clone())
            .unwrap_or_default()
    }
}

/// Rolling control-metric windows over registered cumulative counters.
///
/// Sources register cumulative totals through
/// [`ControlMetrics::observe_transport`]; [`ControlMetrics::tick`]
/// advances per-second windows in O(1) (counter reads plus one
/// timestamp, never a router scan). Inspection calls `tick` then reads
/// the snapshot, so windows roll across requests while no request
/// triggers measurement work beyond the tick itself.
#[derive(Debug)]
pub struct ControlMetrics {
    state: Mutex<MetricsState>,
}

#[derive(Debug)]
struct MetricsState {
    observed: bool,
    rx_bytes: u64,
    tx_bytes: u64,
    rx_dgrams: u64,
    tx_dgrams: u64,
    base_rx_bytes: u64,
    base_tx_bytes: u64,
    base_rx_dgrams: u64,
    base_tx_dgrams: u64,
    last_tick: Option<Instant>,
    inbound_bps: u64,
    outbound_bps: u64,
    rates: BTreeMap<String, u64>,
    succeeded: u64,
    attempted: u64,
}

impl ControlMetrics {
    /// Creates unticked metrics with no registered sources.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MetricsState {
                observed: false,
                rx_bytes: 0,
                tx_bytes: 0,
                rx_dgrams: 0,
                tx_dgrams: 0,
                base_rx_bytes: 0,
                base_tx_bytes: 0,
                base_rx_dgrams: 0,
                base_tx_dgrams: 0,
                last_tick: None,
                inbound_bps: 0,
                outbound_bps: 0,
                rates: BTreeMap::new(),
                succeeded: 0,
                attempted: 0,
            }),
        }
    }

    /// Registers cumulative transport counters (SSU2 bytes/datagrams).
    /// Coverage is exactly the registered sources: local
    /// loopback/destination traffic that bypasses the transport
    /// counters is not counted, and the dossier says so.
    pub fn observe_transport(&self, rx_bytes: u64, tx_bytes: u64, rx_dgrams: u64, tx_dgrams: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.observed = true;
            state.rx_bytes = rx_bytes;
            state.tx_bytes = tx_bytes;
            state.rx_dgrams = rx_dgrams;
            state.tx_dgrams = tx_dgrams;
        }
    }

    /// Registers cumulative tunnel-build outcomes. No build reporter
    /// exists in the default graph, so production holds (0, 0).
    pub fn observe_builds(&self, succeeded: u64, attempted: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.succeeded = succeeded;
            state.attempted = attempted;
        }
    }

    /// Advances the rolling windows to `now` (deterministic seam for
    /// tests; production passes [`Instant::now`]).
    pub fn tick_at(&self, now: Instant) {
        if let Ok(mut state) = self.state.lock() {
            let elapsed = state
                .last_tick
                .map(|last| now.saturating_duration_since(last));
            state.last_tick = Some(now);
            let (rx_delta, tx_delta, rx_dgrams, tx_dgrams) = (
                state.rx_bytes.saturating_sub(state.base_rx_bytes),
                state.tx_bytes.saturating_sub(state.base_tx_bytes),
                state.rx_dgrams.saturating_sub(state.base_rx_dgrams),
                state.tx_dgrams.saturating_sub(state.base_tx_dgrams),
            );
            state.base_rx_bytes = state.rx_bytes;
            state.base_tx_bytes = state.tx_bytes;
            state.base_rx_dgrams = state.rx_dgrams;
            state.base_tx_dgrams = state.tx_dgrams;
            let Some(delta) = elapsed else {
                return;
            };
            let secs = delta.as_secs_f64();
            if !(secs > 0.0) || !state.observed {
                return;
            }
            // Per-second rates from cumulative deltas against the
            // previous tick baseline.
            let rx_bps = (rx_delta as f64 / secs) as u64;
            let tx_bps = (tx_delta as f64 / secs) as u64;
            let rx_dps = (rx_dgrams as f64 / secs) as u64;
            let tx_dps = (tx_dgrams as f64 / secs) as u64;
            state.inbound_bps = rx_bps;
            state.outbound_bps = tx_bps;
            state.rates.clear();
            state.rates.insert("ssu2.rx_bps".to_owned(), rx_bps);
            state.rates.insert("ssu2.tx_bps".to_owned(), tx_bps);
            state.rates.insert("ssu2.rx_dgrams_ps".to_owned(), rx_dps);
            state.rates.insert("ssu2.tx_dgrams_ps".to_owned(), tx_dps);
            debug_assert!(state.rates.len() <= MAX_CONTROL_RATES);
        }
    }

    /// Advances the rolling windows to now.
    pub fn tick(&self) {
        self.tick_at(Instant::now());
    }

    /// Current bandwidth pair (ticks first so windows roll).
    pub fn bandwidth(&self) -> (u64, u64) {
        self.tick();
        self.state
            .lock()
            .map(|state| (state.inbound_bps, state.outbound_bps))
            .unwrap_or((0, 0))
    }

    /// Current rate map (ticks first; empty until a source observes).
    pub fn rates_snapshot(&self) -> BTreeMap<String, u64> {
        self.tick();
        self.state
            .lock()
            .map(|state| state.rates.clone())
            .unwrap_or_default()
    }

    /// Current build-outcome pair (ticks first for window parity).
    pub fn success(&self) -> (u64, u64) {
        self.tick();
        self.state
            .lock()
            .map(|state| (state.succeeded, state.attempted))
            .unwrap_or((0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_records_info_and_above_only() {
        let ring = LogRing::new();
        ring.record("INFO", "daemon", "control plane ready");
        ring.record("WARN", "daemon", "slow peer");
        ring.record("ERROR", "daemon", "bind failed");
        ring.record("DEBUG", "daemon", "verbose detail");
        ring.record("TRACE", "daemon", "packet bytes");
        let (lines, dropped) = ring.snapshot();
        assert_eq!(lines.len(), 3);
        assert_eq!(dropped, 0);
        assert_eq!(lines[0].wire(), "INFO daemon: control plane ready");
    }

    #[test]
    fn ring_redacts_every_secret_marker() {
        let ring = LogRing::new();
        for marker in [
            "auth password hunter2",
            "PassWd=secret",
            "session token abc",
            "shared secret value",
            "signing seed bytes",
            "auth=basic dXNlcg==",
            "bearer eyJhbGciOi",
            "api_key sk-live",
            "router PRIV key material",
        ] {
            ring.record("INFO", "daemon", marker);
        }
        let (lines, _) = ring.snapshot();
        assert_eq!(lines.len(), 9);
        for line in &lines {
            assert_eq!(
                line.message,
                REDACTED_MESSAGE,
                "unredacted: {}",
                line.wire()
            );
        }
    }

    #[test]
    fn ring_keeps_ordinary_naming_traffic() {
        let ring = LogRing::new();
        ring.record(
            "INFO",
            "addressbook",
            "private address book committed generation 7",
        );
        ring.record("INFO", "daemon", "control plane ready");
        let (lines, _) = ring.snapshot();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].message.contains("private address book"));
    }

    #[test]
    fn ring_evicts_oldest_past_capacity_and_counts_drops() {
        let ring = LogRing::new();
        for index in 0..MAX_LOG_RING_ENTRIES + 40 {
            ring.record("INFO", "daemon", &format!("event {index}"));
        }
        assert_eq!(ring.len(), MAX_LOG_RING_ENTRIES);
        let (lines, dropped) = ring.snapshot();
        assert_eq!(dropped, 40);
        assert!(lines[0].message.contains("event 40"));
        // Full worst-case snapshot stays under the 64 KiB wire ceiling
        // (entry cap times line cap plus JSON envelope slack).
        let worst_case = lines.iter().map(|line| line.wire().len()).sum::<usize>();
        assert!(worst_case <= 65536, "snapshot bytes: {worst_case}");
    }

    #[test]
    fn ledger_attests_empty_without_reporters() {
        let ledger = BanLedger::new();
        assert!(ledger.attested().is_empty());
    }

    #[test]
    fn layer_feeds_ring_through_subscriber() {
        use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt as _;
        let ring = Arc::new(LogRing::new());
        let subscriber = tracing_subscriber::registry().with(LogRingLayer::new(Arc::clone(&ring)));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "daemon", "control plane ready");
            tracing::debug!(target: "daemon", "verbose detail");
            tracing::info!(target: "daemon", password = "hunter2");
        });
        let (lines, _) = ring.snapshot();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].message.contains("control plane ready"));
        assert_eq!(lines[1].message, REDACTED_MESSAGE);
    }

    #[test]
    fn metrics_idle_without_sources() {
        let metrics = ControlMetrics::new();
        assert_eq!(metrics.bandwidth(), (0, 0));
        assert!(metrics.rates_snapshot().is_empty());
        assert_eq!(metrics.success(), (0, 0));
    }

    #[test]
    fn metrics_rolls_rates_across_ticks() {
        let metrics = ControlMetrics::new();
        let first = Instant::now();
        metrics.observe_transport(10_000, 5_000, 100, 50);
        metrics.tick_at(first);
        // Baseline tick establishes the window without computing rates.
        assert!(metrics.state.lock().expect("state").rates.is_empty());
        let second = first + std::time::Duration::from_secs(10);
        metrics.observe_transport(30_000, 15_000, 300, 150);
        metrics.tick_at(second);
        let rates = metrics.state.lock().expect("state").rates.clone();
        assert_eq!(rates.get("ssu2.rx_bps"), Some(&2000));
        assert_eq!(rates.get("ssu2.tx_bps"), Some(&1000));
        assert_eq!(rates.len(), 4);
        let (inbound, outbound) = (rates["ssu2.rx_bps"], rates["ssu2.tx_bps"]);
        assert_eq!((inbound, outbound), (2000, 1000));
    }

    #[test]
    fn metrics_build_outcomes_hold_until_reported() {
        let metrics = ControlMetrics::new();
        assert_eq!(metrics.success(), (0, 0));
        metrics.observe_builds(7, 9);
        assert_eq!(metrics.success(), (7, 9));
    }
}
