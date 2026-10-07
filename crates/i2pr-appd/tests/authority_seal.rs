//! Plan 369 §9 — the launch-authority seal, asserted at compile time.
//!
//! # What is being proved
//!
//! `LaunchAuthority` and `AuthorityRequest` must never gain a decoder. If either
//! could be built from wire bytes — an application's `hello`, a manifest, an
//! apphost bootstrap payload, anything peer-supplied — then Plan 369's central
//! claim ("launch authority is manager-created, never app-created") would be
//! false, and false in the worst way: an application could *ask* for
//! capabilities.
//!
//! # Why a compile-time probe rather than a grep
//!
//! A checker that scans for `#[derive(Deserialize)]` reports on the spelling of
//! today's source. Someone adding a derive to a wrapper, or hand-writing
//! `impl<'de> Deserialize<'de> for LaunchAuthority`, defeats it while passing the
//! scan.
//!
//! This probe defeats neither, because it is *resolution* rather than *scanning*:
//! the inherent method exists only when the bound `T: DeserializeOwned` holds,
//! and method resolution prefers inherent items over trait items. So the trait
//! fallback resolving to `false` **is** the proof.
//!
//! # Why the associated-const form is not used here
//!
//! The obvious spelling of this trick uses an associated const, and it is wrong.
//! For an associated const reached by path (`Probe::<T>::CONST`), the trait impl
//! wins over the inherent one, so the probe reports `false` for *every* type —
//! including ones that genuinely deserialize. That version was written first,
//! and `the_probe_must_be_able_to_detect_a_real_decoder` is exactly the assertion
//! that exposed it. It is kept as a test here for the same reason: a guard that
//! cannot fail is worse than no guard, because it is believed in.
//!
//! The same trap has a second form: behind a *generic* helper function the
//! receiver type is unresolved during method resolution, so rustc selects the
//! candidate valid for every `T` — the blanket trait impl — and the probe answers
//! `false` for everything. Both traps produce a file that compiles, passes, and
//! proves nothing, which is why the positive control below is not optional
//! decoration.
//
//! (Verified empirically on this toolchain, Rust 1.95.0.)

use std::marker::PhantomData;

use i2pr_appd::authority::{AuthorityRequest, LaunchAuthority};

struct Probe<T>(PhantomData<T>);

/// The fallback. Present for every `Probe<T>` and answers "not deserializable".
trait Fallback {
    fn detects_decoder(&self) -> bool {
        false
    }
}

impl<T> Fallback for Probe<T> {}

/// Exists only when `T` really implements `Deserialize`, and wins method
/// resolution over the trait above because inherent items are considered first.
///
/// The `&self` receiver is load-bearing, not stylistic: an inherent associated
/// function *without* a receiver is reachable only by path, and path resolution
/// picks the trait impl instead. That version of this file compiled, passed every
/// assertion, and was vacuous — see
/// `the_probe_must_be_able_to_detect_a_real_decoder`.
impl<T: serde::de::DeserializeOwned> Probe<T> {
    fn detects_decoder(&self) -> bool {
        true
    }
}

/// Expands **monomorphically** at each call site, which is load-bearing.
///
/// Written as a generic `fn is_deserializable<T>() -> bool`, the receiver type
/// `Probe<T>` is unresolved when method resolution runs, so rustc must pick the
/// one candidate that is valid for *every* `T` — the blanket trait impl — and the
/// probe answers `false` for every type, including ones that genuinely
/// deserialize. That version compiled and passed both seal assertions while
/// proving nothing. A macro expands at the call site, where `T` is concrete and
/// the inherent impl is visible.
macro_rules! detects_decoder {
    ($type:ty) => {
        Probe::<$type>(PhantomData).detects_decoder()
    };
}

#[test]
fn launch_authority_can_never_be_decoded_from_wire_bytes() {
    assert!(
        !detects_decoder!(LaunchAuthority),
        "LaunchAuthority gained a decoder: launch authority is no longer manager-created"
    );
}

#[test]
fn the_authority_request_envelope_can_never_be_decoded_from_wire_bytes() {
    assert!(
        !detects_decoder!(AuthorityRequest),
        "AuthorityRequest gained a decoder: a wire message could now assemble a launch"
    );
}

/// The probe is only meaningful if it can actually observe a deserializable type.
/// Without this, a mistake in the trait bound would make the two assertions
/// above pass for the wrong reason.
///
/// This test exists because it has already caught a real defect: the
/// associated-const spelling of this probe reports `false` for every type and
/// would have made the whole file a no-op that always agreed with itself.
#[test]
fn the_probe_must_be_able_to_detect_a_real_decoder() {
    assert!(
        detects_decoder!(i2pr_app_manager_proto::ManagerPrincipal),
        "the probe failed to detect a real Deserialize impl, so the assertions above are vacuous"
    );
    assert!(detects_decoder!(serde_json::Value));
}

/// And the other direction, so the fallback arm is exercised rather than merely
/// present.
///
/// (`()` is deliberately absent: serde *does* implement `Deserialize` for the unit
/// type, so listing it here would be a wrong expectation rather than extra
/// coverage.)
#[test]
fn the_probe_reports_false_for_a_type_with_no_decoder() {
    assert!(!detects_decoder!(i2pr_appd::Appd));
    assert!(!detects_decoder!(i2pr_appd::AppdState));
    assert!(!detects_decoder!(i2pr_appd::manager_link::ManagerLink));
}
