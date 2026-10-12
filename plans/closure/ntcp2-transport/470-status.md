# Plan 470 status: blocked — no Java-owned NTCP2 dial after stock admission

Closure token: `blocked-java-stock-flood-did-not-produce-owned-ntcp2-dial`

Plan: [`470-normal-java-i2p-ntcp2-two-way-product-qualification.md`](../../implementation/ntcp2-transport/470-normal-java-i2p-ntcp2-two-way-product-qualification.md)

## Outcome

The controlled runner starts unmodified Java I2P 2.13.0 `Router(Properties)`
from exact pin `9134f808337b401e8e53c73734c81fab04280c9d` in a separate
NTCP2-only network-ID-2 profile. Its signed address matches the actual loopback
listener. The i2pr participant is an ordinary `i2pr run` using its named
controlled-loopback profile; there is no public bind, reseed, SAM, or Java SSU2
listener.

Java loaded the signed i2pr RouterInfo into its persistent NetDB with `f`. Its
live floodfill selector selected that candidate for the local RouterInfo
routing key; the stock `StoreJob.shouldStoreTo` policy accepted the candidate,
it was neither banlisted nor NTCP-unreachable, and the stock NTCP transport
returned a usable NTCP2 bid. The test invoked the public
`FloodfillNetworkDatabaseFacade.flood(RouterInfo)` after NetDB and CommSystem
readiness. This is a stock Java-owned NetDB operation, not a constructed I2NP
message.

The bounded replay did not produce a Java-initiated connection, an authenticated
inbound link, an I2NP product effect, or a correlated reply. The sanitized
receipt recorded 25 i2pr outbound attempts that completed SessionRequest writes
and then read EOF at SessionCreated; these do not establish that Java initiated
a dial. Exact pinned Java source shows `flood` calls
`CommSystem.processMessage` when the selected peer has a local RouterInfo and
passes `shouldFloodTo`. The live preflight established selector membership,
local RI presence, store eligibility, and a non-self target. The first missing
transition is after stock NetDB dispatch, at Java's CommSystem/transport send
owner. Do not inspect Noise/NTCP2 crypto until a Java-owned dial is observed.

Plan 470 is **blocked**, not passed. J1 and the signed-RI/selector/NTCP2-bid
preconditions passed. J2, J3, J4, and the J5 end-to-end safety/lifecycle matrix
remain unproven. The normal daemon remains default-off outside its named
controlled-loopback profile; no Java or protocol support is advertised.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Exact pinned normal Java 2.13.0 process and isolated NTCP2-only profile | Passed; source pin verified, ordinary `Router(Properties)` lifecycle, network ID 2, loopback listener and signed endpoint match. |
| Signed i2pr RouterInfo loaded by Java and candidate role/selector admission | Passed; present in main NetDB with `f`, selected for the local RouterInfo routing key. |
| Stock StoreJob policy, banlist/reachability, and NTCP2 transport bid | Passed; `shouldStoreTo=true`, not banlisted, transport not marked unreachable, usable NTCP2 bid. |
| Java-owned normal NetDB operation | Invoked stock `FloodfillNetworkDatabaseFacade.flood` after readiness. |
| J2 i2pr-initiated request and correlated Java reply | Not reached; outbound attempts ended at EOF while reading SessionCreated; no outbound link promotion. |
| J3 fresh Java-initiated NTCP2 and i2pr product effect/reply | Blocked; no Java-owned dial or inbound handshake observed. |
| J4 actual request/reply in both directions | Not reached. |
| J5 wrong-network/key/identity, expiry, duplicate-link, saturation, timeout, cancellation and restart matrix | Not run end-to-end; component controls do not substitute for this product matrix. |
| Public capability or support promotion | None. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk python3 -m py_compile tests/integration/ntcp2/plan470_java_ntcp2.py` | Passed. |
| `rtk python3 tests/integration/ntcp2/plan470_java_ntcp2.py` | Exact-pin controlled replay; selector, RouterInfo store policy and NTCP2 bid preflights passed. First missing stage was `daemon_lookup_submitted`; sanitized receipt had no Java NTCP/DatabaseStore event, no inbound daemon handshake, and no outbound link promotion. |
| `rtk cargo build --locked -p i2pr-daemon -p i2pr-interop` | Passed inside the controlled runner. |
| `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | Passed. |
| `rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1` | Passed, 9 tests. |
| `rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1` | Passed, 135 passed, 1 ignored. |
| `rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets` | Passed, 35 tests. |
| `rtk cargo fmt --all --check` | Passed. |
| `rtk bash scripts/check-ntcp2-vectors.sh` | Passed; manifest complete and hashes match. |
| `rtk bash scripts/check-ntcp2-interoperability.sh` | Passed; Plan 099 static check. |

An additional replay after adding stock-flood log counters was interrupted
before its receipt was collected and is not used as evidence. Temporary router
state is private and runner-owned; raw Java/i2pr logs, identities, RouterInfos,
keys, and addresses are not retained in the closure.

## Invariant, security, and compatibility review

- No stock Java source or binary was modified. The Java helper emits fixed
  booleans/counts, not hashes, addresses, peer identities, or raw logs.
- Java binds only to loopback, reseed is disabled, and router state uses a
  private temporary directory. i2pr uses its existing guarded loopback profile.
- New runtime/daemon log fields are closed handshake stage labels; they omit
  peer bytes, identity, address, and raw error details.
- No dependency, wire behavior, default configuration, support inventory,
  conformance claim, or RouterInfo advertisement changed. Migration impact: none.

## Unblock audit and disposition

Plans 459/461 and 467–469 remain passed only at their recorded controlled scope;
they do not satisfy Java's second-family gate. Plan 444's Java floodfill
preference/store matrix remains independent. Plan 435, Plan 101, and the support
inventory retain their gates. Plan 471 is independent and remains eligible to
proceed; Plans 472 and 473 are independent as well. No historical closure is
rewritten and no successor is registered: source-localize the missing Java
CommSystem send/dial transition before another protocol attempt.

Disposition: **blocked at Java stock NetDB-to-CommSystem send initiation**.
