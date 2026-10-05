#!/usr/bin/env bash
# Plan 342 static guard: the I2P-routed outproxy request-path boundary.
#
# ## What this asserts, and why a static guard at all
#
# Plan 342 invariant 1 is "no path may contain a direct-clearnet capability,
# **including failure paths**". The runtime proof for that is Plan 342's
# self-composed loopback lane, which is not in the tree yet. Until it is, this
# guard pins the part that *is* structural rather than behavioural:
#
#   1. all four client request paths (HTTP forward, HTTP CONNECT, the
#      strict-CONNECT adapter, SOCKS5) classify their target through the one
#      `classify_client_target`, and all three outcomes are handled in the
#      function that classifies — not merely mentioned somewhere in the file;
#   2. a refusal produces a client-visible error and a non-success outcome,
#      and no path falls through to its direct branch on a refusal;
#   3. the outproxy handshake prefix reaches the pump's **inbound** direction,
#      because `run_stream_pump`'s `initial_bytes` argument feeds the
#      opposite one;
#   4. no request path contains a name-resolution or direct-connect
#      primitive, so a clearnet socket is not expressible in one;
#   5. `outproxy_password` is sealed before a definition is built, is never
#      echoed, and is opened in exactly one place;
#   6. a clearnet authority can reach the classifier at all: every
#      request-target grammar parses under the tunnel's `TargetPolicy`, every
#      pre-Plan-342 entry point is still a strict `.i2p`-only wrapper, and the
#      policy comes from the provider registry rather than the options value;
#   7. the forward path forwards the *clearnet* authority in `Host:` rather
#      than the tunnel's own b32 destination, because after the handshake the
#      session is a byte pipe to the origin, not a forward proxy.
#
# ## Why the checks are function-scoped
#
# An earlier draft of this script grepped whole files for a symbol. The
# mutation table for that draft is recorded below, and it is the reason this
# one is not written that way: grepping a file cannot tell "this function
# calls `classify_client_target`" from "this function builds a value that
# happens to mention it", so a request path whose classification was wrapped
# in dead code still passed. Every row below therefore resolves a named
# function body by brace matching and asserts inside it.
#
# A guard that cannot fail is a comment. The rows here were checked against
# thirty-nine deliberate mutations of the sources; all thirty-nine are
# detected. Re-run
# `python3 scripts/check-outproxy-request-path.py --mutation-table` to
# reproduce that.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$root/scripts/check-outproxy-request-path.py" "$@"