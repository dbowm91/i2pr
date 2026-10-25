//! Plan 503 — the peer-eligibility decision surface.
//!
//! Plan 491 attached two duties to the API version this router declares:
//! minimum peers a tunnel may be built through, and minimum floodfill peers a
//! DSM may be sent to. The decision has a receipt shape, and this module owns
//! that shape together with the live owner that produces it.
//!
//! The receipt carries **counts and stable reason labels only**. It never
//! carries a peer hash, an address, a key, a version string, or a payload.

#![forbid(unsafe_code)]
