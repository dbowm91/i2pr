// Plan 064: supersedure marker for the Plan 059 i2pd direct connect helper.
//
// This file is a fail-closed compatibility stub. The canonical Plan 064
// i2pd direct NTCP2 driver lives under
// `tests/integration/ntcp2/reference-drivers/i2pd/` and exposes
// `inspect`, `listen`, and `dial` modes through a strict config
// contract, a compile-time-gated passive observer, and the Plan 062 v4
// trigger schema. The Plan 059 helper at this path carried the eight
// defects enumerated in Plan 064 (40-hex Router Hash, wrong static-key
// accessor, incomplete initialization, null message trigger,
// incorrect asynchronous future interpretation, reserved synthetic
// endpoint rejection, no exact receiver correlation, placeholder
// provenance). The Plan 064 driver eliminates every defect; the
// Plan 059 helper is no longer active and may not be selected by any
// primary mixed-router direction.
//
// The Python harness adapter at
// `tests/integration/ntcp2/harness/i2pd_direct_driver.py` is the
// canonical Plan 064 adapter. The Plan 059 helper Python wrapper at
// `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.py`
// remains as a bounded historical-only path used by the Plan 059 test
// matrix; it does not bind the canonical Plan 064 v4 trigger schema
// and is not used by the Plan 065 canonical primary runner.
//
// The Plan 059 source-lock record at
// `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json`
// is preserved verbatim with the explicit `helper_kind =
// i2pd-direct-helper` (Plan 055) marker; that record is the
// historical-reader path, not the active Plan 064 source lock. The
// active Plan 064 source lock lives at
// `tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json`
// with `helper_kind = i2pd-direct-driver` and the v4 trigger schema
// marker.

#include <cstdio>
#include <cstdlib>

int main(int /*argc*/, char** /*argv*/) {
    std::fprintf(stderr,
                 "i2pd-direct-connect: the Plan 059 helper is superseded by "
                 "the Plan 064 i2pd direct NTCP2 driver at "
                 "tests/integration/ntcp2/reference-drivers/i2pd/. "
                 "This path is a fail-closed compatibility stub.\n");
    return 70;
}