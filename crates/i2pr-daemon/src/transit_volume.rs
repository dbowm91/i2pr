//! Plan 340: transit volume, trailing-window bandwidth, and the
//! participation-posture owner behind Proposal 170's three remaining
//! unavailable RouterInfo selectors.
//!
//! The selectors are `i2p.router.net.total.transit.bytes`,
//! `i2p.router.net.bw.transit.15s`, and
//! `i2p.router.net.tunnels.shareratio`. This module owns the state they
//! project; [`crate::i2pcontrol_inspection`] owns the projection itself.
//!
//! # Posture, not instrumentation
//!
//! Ordinary product profiles never construct a transit data-plane owner:
//! production composition consults
//! [`crate::transit_owner::controlled_transit_disabled_probe`] so the
//! live-owner module has a production caller without dispatching, and
//! `scripts/check-m11-transit-boundaries.sh` rule 10 pins that shape. A
//! production router therefore relays nothing, and the honest answer for
//! all three selectors is `0`, `0`, and `0.0`.
//!
//! Those zeros are not a missing value wearing a zero costume.
//! [`TransitParticipation::Disabled`] holds **no counters at all**, so it
//! is structurally incapable of reporting a non-zero volume; the zero is a
//! property of the enforced posture, and the only way to obtain a non-zero
//! is a real [`TransitVolumeCounters`] that a real forward dispatch has
//! advanced. Enabling participation is a separate product-posture decision
//! and is deliberately not reachable from this module.
//!
//! # Bounded and read-only on the read side
//!
//! The trailing window is a fixed-size ring of per-second buckets, one
//! slot longer than the window itself so that every second inside the
//! window has a distinct slot. That makes the read a pure scan: no lock, no
//! pruning pass, and two reads with no intervening forward agree exactly.
//! No timer, task, queue, or unbounded container is introduced.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use i2pr_proto::TUNNEL_DATA_PAYLOAD_SIZE;

/// Bytes this router accounts for one forwarded `TunnelData` cell.
///
/// The payload plus the I2NP tunnel-data size field, matching the pinned
/// i2pd `TUNNEL_DATA_MSG_SIZE = 1028` (`libi2pd/TunnelBase.h:29`) that
/// i2pd adds in `TransitTunnel::EncryptTunnelMsg`
/// (`libi2pd/TransitTunnel.cpp:43`). As in i2pd, the I2NP message header
/// sits outside the counted body.
pub const TRANSIT_CELL_ACCOUNTED_BYTES: u64 = (TUNNEL_DATA_PAYLOAD_SIZE + 4) as u64;

/// Width of the transit bandwidth window, in seconds.
pub const TRANSIT_BW_WINDOW_SECONDS: u64 = 15;

/// One bucket longer than the window, so every second inside the window
/// occupies a distinct slot and no in-window second can be aliased by a
/// stale one.
const TRANSIT_BW_RING_SLOTS: usize = (TRANSIT_BW_WINDOW_SECONDS + 1) as usize;

/// Highest value `i2p.router.net.tunnels.shareratio` may report.
///
/// Transit-forwarded bytes are a subset of bytes sent, so the ratio is
/// mathematically at most `1.0`; the clamp bounds the published value
/// rather than trusting two independently sampled counters to agree.
pub const TRANSIT_SHARE_RATIO_MAX: f64 = 1.0;

/// The one clock the volume window runs on, in seconds since the Unix
/// epoch.
///
/// Both the forward path that stamps the ring and the request path that
/// reads it must use this single base. They previously did not: the
/// forward path inherited the transit ingress clock while the read side
/// inherited the I2PControl service's *monotonic* `now_ms`, so the two
/// lived in different epochs and a live window could only ever read empty.
/// Deriving both sides here removes the class of bug rather than the
/// instance.
pub fn wall_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

/// Maps an epoch second onto its ring slot.
fn slot_of(epoch: u64) -> usize {
    (epoch % TRANSIT_BW_RING_SLOTS as u64) as usize
}

/// One second of accounted transit volume.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TransitSecondBucket {
    /// The **absolute** second this bucket describes.
    ///
    /// Storing the absolute value rather than a modulo-16 residue is
    /// load-bearing: two seconds one full ring period apart land on the
    /// same slot, and only an absolute comparison can tell "the slot has
    /// rolled to a new second" from "the same second again". A residue
    /// would make the second record accumulate into the first and the
    /// window would over-count by every recycled period.
    epoch: u64,
    /// Bytes forwarded during that second.
    bytes: u64,
}

/// Fixed-size ring of per-second buckets backing a trailing-window rate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransitBandwidthWindow {
    buckets: [TransitSecondBucket; TRANSIT_BW_RING_SLOTS],
}

impl TransitBandwidthWindow {
    /// Records `bytes` against `now_seconds`, starting a fresh bucket when
    /// the slot has rolled over to a new second.
    pub fn record(&mut self, bytes: u64, now_seconds: u64) {
        let slot = slot_of(now_seconds);
        if self.buckets[slot].epoch == now_seconds {
            self.buckets[slot].bytes = self.buckets[slot].bytes.saturating_add(bytes);
        } else {
            self.buckets[slot] = TransitSecondBucket {
                epoch: now_seconds,
                bytes,
            };
        }
    }

    /// Returns the bytes recorded in the trailing window ending at
    /// `now_seconds`. A slot counts only when its recorded absolute second
    /// is one of the [`TRANSIT_BW_WINDOW_SECONDS`] seconds preceding the
    /// read, so a bucket older than the window contributes nothing.
    ///
    /// Seconds are compared absolutely, and the look-back saturates through
    /// `wrapping_sub` rather than panicking on a clock reading near zero.
    pub fn bytes_in_window(&self, now_seconds: u64) -> u64 {
        let mut total = 0_u64;
        for age in 0..TRANSIT_BW_WINDOW_SECONDS {
            let second = now_seconds.wrapping_sub(age);
            let bucket = self.buckets[slot_of(second)];
            if bucket.epoch == second {
                total = total.saturating_add(bucket.bytes);
            }
        }
        total
    }
}

/// Cumulative and windowed transit volume, advanced only by a real
/// forward dispatch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransitVolumeCounters {
    total_forwarded_bytes: u64,
    window: TransitBandwidthWindow,
}

impl TransitVolumeCounters {
    /// Records one canonical `TunnelData` cell.
    pub fn record_forward(&mut self, now_seconds: u64) {
        self.record_forward_bytes(TRANSIT_CELL_ACCOUNTED_BYTES, now_seconds);
    }

    /// Records an explicit byte count, for callers that account a
    /// differently sized unit.
    pub fn record_forward_bytes(&mut self, bytes: u64, now_seconds: u64) {
        self.total_forwarded_bytes = self.total_forwarded_bytes.saturating_add(bytes);
        self.window.record(bytes, now_seconds);
    }

    /// Returns cumulative transit bytes forwarded since construction.
    pub const fn total_forwarded_bytes(&self) -> u64 {
        self.total_forwarded_bytes
    }

    /// Returns the trailing-window mean transit bandwidth in bytes per
    /// second, floored.
    ///
    /// This is computed at read time from the per-second ring. It is not
    /// i2pd's one-hertz timer sample, and this plan adds no timer: the
    /// difference is confined to the first second after a change of rate
    /// and is recorded in the reference dossier.
    pub fn transit_bandwidth_15s(&self, now_seconds: u64) -> u64 {
        self.window.bytes_in_window(now_seconds) / TRANSIT_BW_WINDOW_SECONDS
    }
}

/// Bounded, secret-free projection of transit volume. Three numbers and a
/// boolean: no payload, key, peer, endpoint, or address crosses this
/// boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitVolumeSnapshot {
    /// Whether this router participates in transit at all.
    pub participating: bool,
    /// Cumulative transit bytes forwarded since the owner was constructed.
    pub total_transit_bytes: u64,
    /// Trailing-window transit bandwidth in bytes per second.
    pub transit_bandwidth_15s: u64,
}

/// The authoritative transit-participation posture, always installed.
///
/// [`Disabled`](Self::Disabled) is the enforced product posture and owns no
/// counters; [`Enabled`](Self::Enabled) exists only for the controlled
/// qualification lane and reads counters the real forward path advances.
#[derive(Clone, Debug, Default)]
pub enum TransitParticipation {
    /// The product posture: this router relays no transit traffic.
    #[default]
    Disabled,
    /// A controlled lane installed real volume counters.
    Enabled(Arc<Mutex<TransitVolumeCounters>>),
}

impl TransitParticipation {
    /// Returns the shared counters a controlled lane should publish, or
    /// `None` while participation is disabled.
    pub fn counters(&self) -> Option<Arc<Mutex<TransitVolumeCounters>>> {
        match self {
            Self::Disabled => None,
            Self::Enabled(counters) => Some(Arc::clone(counters)),
        }
    }

    /// Projects the volume snapshot, or `None` when the counters cannot be
    /// read. A poisoned lock is a gap, never a zero.
    pub fn volume_snapshot(&self, now_seconds: u64) -> Option<TransitVolumeSnapshot> {
        match self {
            Self::Disabled => Some(TransitVolumeSnapshot {
                participating: false,
                total_transit_bytes: 0,
                transit_bandwidth_15s: 0,
            }),
            Self::Enabled(counters) => {
                let counters = counters.lock().ok()?;
                Some(TransitVolumeSnapshot {
                    participating: true,
                    total_transit_bytes: counters.total_forwarded_bytes(),
                    transit_bandwidth_15s: counters.transit_bandwidth_15s(now_seconds),
                })
            }
        }
    }
}

/// Derives `i2p.router.net.tunnels.shareratio`.
///
/// Proposal 170 gives this key's name but not its arithmetic, and — unlike
/// its neighbours — does not mark it *"(adopted from i2pd)"*. i2pd's
/// handler map has no entry for it. i2pr therefore defines it locally as
/// the observed share of outbound bytes spent relaying other routers'
/// traffic, and labels it that way wherever it is published.
///
/// `sent_total` is the router's attested cumulative sent-byte total.
/// Returns `None` — a fail-closed gap — when participation is real and no
/// denominator is attested, because a ratio without a denominator would be
/// a guess.
pub fn transit_share_ratio(
    snapshot: &TransitVolumeSnapshot,
    sent_total: Option<u64>,
) -> Option<f64> {
    if !snapshot.participating {
        // A router that relays nothing shares none of its bandwidth. This
        // is true independently of the denominator, so it is not a gap.
        return Some(0.0);
    }
    let sent = sent_total?;
    if sent == 0 {
        return Some(0.0);
    }
    let ratio = (snapshot.total_transit_bytes as f64) / (sent as f64);
    if !ratio.is_finite() || ratio < 0.0 {
        return None;
    }
    Some(ratio.min(TRANSIT_SHARE_RATIO_MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 1_800_000_000;

    #[test]
    fn accounted_cell_matches_the_pinned_reference_constant() {
        // i2pd TUNNEL_DATA_MSG_SIZE: 1024-byte payload + 4-byte size field.
        assert_eq!(TRANSIT_CELL_ACCOUNTED_BYTES, 1_028);
    }

    #[test]
    fn fresh_window_reports_only_recorded_seconds() {
        let mut window = TransitBandwidthWindow::default();
        assert_eq!(window.bytes_in_window(BASE), 0);
        window.record(1_000, BASE);
        assert_eq!(window.bytes_in_window(BASE), 1_000);
        // A read one second later is still inside the window.
        assert_eq!(window.bytes_in_window(BASE + 1), 1_000);
    }

    #[test]
    fn window_drops_bytes_older_than_fifteen_seconds() {
        let mut window = TransitBandwidthWindow::default();
        window.record(1_000, BASE);
        // At +14 the sample is still the last of fifteen in-window seconds.
        assert_eq!(window.bytes_in_window(BASE + 14), 1_000);
        // At +15 it has left the window entirely.
        assert_eq!(window.bytes_in_window(BASE + 15), 0);
    }

    #[test]
    fn window_slot_reuse_starts_a_fresh_bucket() {
        let mut window = TransitBandwidthWindow::default();
        // Two full ring periods apart, the slot is reused; the stale bucket
        // must not be added to the new one.
        window.record(500, BASE);
        window.record(700, BASE + (TRANSIT_BW_RING_SLOTS as u64));
        assert_eq!(window.bytes_in_window(BASE + 16), 700);
    }

    #[test]
    fn window_sums_a_full_trailing_period() {
        let mut window = TransitBandwidthWindow::default();
        for second in 0..TRANSIT_BW_WINDOW_SECONDS {
            window.record(100, BASE + second);
        }
        assert_eq!(window.bytes_in_window(BASE + 14), 1_500);
        // The oldest sample falls out exactly at the window edge.
        assert_eq!(window.bytes_in_window(BASE + 15), 1_400);
    }

    #[test]
    fn window_reads_are_stable_without_an_intervening_record() {
        let mut window = TransitBandwidthWindow::default();
        window.record(2_048, BASE);
        let first = window.bytes_in_window(BASE + 3);
        let second = window.bytes_in_window(BASE + 3);
        assert_eq!(first, second);
        assert_eq!(first, 2_048);
    }

    #[test]
    fn window_handles_a_low_epoch_without_underflow() {
        let mut window = TransitBandwidthWindow::default();
        window.record(64, 0);
        // A read at second 0 must not underflow while looking back.
        assert_eq!(window.bytes_in_window(0), 64);
        assert_eq!(window.bytes_in_window(3), 64);
    }

    #[test]
    fn bandwidth_is_the_floored_window_mean() {
        let mut counters = TransitVolumeCounters::default();
        counters.record_forward_bytes(1_500, BASE);
        assert_eq!(counters.transit_bandwidth_15s(BASE), 100);
        // Sub-window totals floor to zero rather than rounding up.
        counters.record_forward_bytes(7, BASE);
        assert_eq!(counters.transit_bandwidth_15s(BASE), 100);
    }

    #[test]
    fn cumulative_total_is_independent_of_the_window() {
        let mut counters = TransitVolumeCounters::default();
        for second in 0..40_u64 {
            counters.record_forward(BASE + second);
        }
        assert_eq!(
            counters.total_forwarded_bytes(),
            40 * TRANSIT_CELL_ACCOUNTED_BYTES
        );
        // Only the last fifteen seconds remain inside the window.
        assert_eq!(
            counters.transit_bandwidth_15s(BASE + 39),
            (15 * TRANSIT_CELL_ACCOUNTED_BYTES) / TRANSIT_BW_WINDOW_SECONDS
        );
    }

    #[test]
    fn cumulative_total_saturates_instead_of_wrapping() {
        let mut counters = TransitVolumeCounters {
            total_forwarded_bytes: u64::MAX,
            window: TransitBandwidthWindow::default(),
        };
        counters.record_forward_bytes(1_024, BASE);
        assert_eq!(counters.total_forwarded_bytes(), u64::MAX);
    }

    #[test]
    fn disabled_participation_reports_no_volume_and_no_counters() {
        let participation = TransitParticipation::Disabled;
        assert!(participation.counters().is_none());
        let snapshot = participation
            .volume_snapshot(BASE)
            .expect("the disabled posture always projects");
        assert!(!snapshot.participating);
        assert_eq!(snapshot.total_transit_bytes, 0);
        assert_eq!(snapshot.transit_bandwidth_15s, 0);
    }

    #[test]
    fn enabled_participation_projects_the_recorded_volume() {
        let counters = Arc::new(Mutex::new(TransitVolumeCounters::default()));
        let participation = TransitParticipation::Enabled(Arc::clone(&counters));
        {
            let mut guard = counters.lock().expect("unpoisoned");
            guard.record_forward(BASE);
        }
        let snapshot = participation
            .volume_snapshot(BASE)
            .expect("readable counters");
        assert!(snapshot.participating);
        assert_eq!(snapshot.total_transit_bytes, TRANSIT_CELL_ACCOUNTED_BYTES);
        assert_eq!(snapshot.transit_bandwidth_15s, 68);
    }

    #[test]
    fn share_ratio_is_zero_for_a_disabled_posture() {
        let snapshot = TransitVolumeSnapshot {
            participating: false,
            total_transit_bytes: 0,
            transit_bandwidth_15s: 0,
        };
        // The denominator is irrelevant when nothing is relayed.
        assert_eq!(transit_share_ratio(&snapshot, None), Some(0.0));
        assert_eq!(transit_share_ratio(&snapshot, Some(0)), Some(0.0));
        assert_eq!(transit_share_ratio(&snapshot, Some(1_000)), Some(0.0));
    }

    #[test]
    fn share_ratio_needs_an_attested_denominator_when_participating() {
        let snapshot = TransitVolumeSnapshot {
            participating: true,
            total_transit_bytes: 1_028,
            transit_bandwidth_15s: 68,
        };
        // No attested sent total: a gap, never a guess.
        assert_eq!(transit_share_ratio(&snapshot, None), None);
        assert_eq!(transit_share_ratio(&snapshot, Some(0)), Some(0.0));
    }

    #[test]
    fn share_ratio_is_transit_over_sent_and_bounded() {
        let snapshot = TransitVolumeSnapshot {
            participating: true,
            total_transit_bytes: 250,
            transit_bandwidth_15s: 0,
        };
        assert_eq!(transit_share_ratio(&snapshot, Some(1_000)), Some(0.25));
        // Counters sampled independently may momentarily disagree; the
        // published value stays inside its declared bound.
        assert_eq!(
            transit_share_ratio(&snapshot, Some(100)),
            Some(TRANSIT_SHARE_RATIO_MAX)
        );
    }
}
