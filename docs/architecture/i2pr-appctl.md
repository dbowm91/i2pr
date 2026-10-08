# `i2pr-appctl` — offline managed-app administration

Path: `crates/i2pr-appctl/`. Plan 374 adds a separate administrator binary
that operates on `<data-dir>/managed-apps` without a listener or live appd
protocol.

## Commands

The CLI verifies, installs, lists, inspects, and removes packages; trusts or
untrusts publisher fingerprints; and selects exact package versions, grants or
revokes Sam/I2cp, records a launch profile, and changes autostart. Mutations
create a new validated policy generation and fail while appd owns the runtime
lock. Changes apply after app-runtime/router restart.

Install does not trust or select. Signatures identify a publisher key but do
not establish that it is trusted. Untrust clears grants, profile, and
autostart. `UnsafeDirect` requires `--allow-direct-host-network` and provides
ordinary host networking without a sandbox. `Secured` is not launchable yet.
Application exit does not trigger automatic restart.

## Boundary

The binary depends on `i2pr-app-state`, `i2pr-app-package`, and contract types,
plus CLI/filesystem support. It owns no router listener, protocol, runtime, or
process-launch path. Diagnostics print identifiers and decisions, never private
key material or payload contents.
