# Plan 526 — the router advertises its own introducer role

**Status:** registered.
**Subsystem:** network-participation. **Registered:** 2026-11-10.
**Predecessor:** Plan 525 (the router publishes the introducer set it learned),
Plan 524 (the router asks a peer for a relay tag), Plan 523 (the SSU2
introducer set is the specification's form), Plan 520 (the router's own
qualified SSU2 address), Plan 499 (the SSU2 introducer role's live path).

## 1. Objective

The network-participation roadmap's **completion definition** requires that the
router "publishes the validated introducer set in its RouterInfo address".

Plan 523 corrected the **form** of that set. Plan 524 delivered the
**acquisition** half — the router learns that a peer is an introducer and asks
it for a relay tag. Plan 525 delivered the **publication** half — the router
publishes the introducer set it learned.

The **advertisement** half is this plan's subject: the router publishes the
`C` capability in its **own** SSU2 address, so that a peer reading this
router's record learns that this router serves the introducer role and may
therefore ask it to introduce a third party.

Without it, the tree's introduction mechanism is a one-way street. Plan 524
made the router *read* a peer's advertised `C`; nothing in the tree makes the
router *write* its own. The Bob role (Plans 490, 498, 499) therefore serves
nobody, because no peer can learn that it is there.

The router must be able to:

1. **advertise** the introducer role in its published SSU2 address — the
   specification's `C` letter, alongside the family letter the tree already
   publishes;
2. do so **only while the role is actually live**, so the advertisement is
   never a claim the router cannot honour;
3. **refuse** the advertisement — and refuse to *install* a record that carries
   it — whenever the role is not live;
4. publish the address through the netdb's own qualification, so an address
   the netdb refuses is never signed.

## 2. Why ready

The tree holds every fact this plan needs, and each fact has exactly one
production caller that is missing.

- The transport already **models** the capability: `Ssu2Capabilities` carries
  an `IPV4`/`IPV6`/`PEER_TEST`/`RELAY` bit set, `RELAY` is the `C` letter,
  and `supports_relay` reads it
  (`crates/i2pr-transport-ssu2/src/address.rs:426`-`:475`).
- The transport's **parser** already accepts a family-and-role combination: the
  parser is per-character and order-independent
  (`crates/i2pr-transport-ssu2/src/address.rs:1314`-`:1345`), so `"4C"` parses
  and yields both the IPv4 bit and the relay bit.
- The runtime already **knows** whether the introducer role is live:
  `Ssu2RuntimeService::introducer_service_enabled`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2505`).
- The netdb already owns **two** qualification rules and **two** builders
  (`crates/i2pr-netdb/src/floodfill_role.rs:197`, `:277`;
  `crates/i2pr-netdb/src/local.rs:268`, `:292`). A third sits beside them.
- The daemon already owns the **self-publication** path — the opt-in, the
  hand-off of the transport's address to the netdb's builder, the install, the
  withdrawal and the counters
  (`crates/i2pr-daemon/src/self_publication.rs:397`).
- The specification defines the letter: "Relay — As advertised with a `"C"`
  in the `"caps"` parameter", and the `caps` alphabet is `[B,C,4,6]`.

So the gap is not a missing capability. It is a missing **letter**, a missing
**rule**, and — see §3.6 — a missing **caller** for the role itself.

## 3. Current implementation evidence

### 3.1 The specification's letter, and where the reference publishes it

The SSU2 specification's Peer Test / Relay table reads:

> Peer Test — As advertised with a `"B"` in the `"caps"` parameter.
> Relay — As advertised with a `"C"` in the `"caps"` parameter.

and the published address properties table gives the option as:

> `caps`: `[B,C,4,6]` capabilities

The pinned i2pd `2.61.0` (`635b013a612ff47278ef02acf8580a28e10e26c5`)
publishes the role letter on the **published** form only
(`libi2pd/RouterInfo.cpp:1350`-`:1373`):

```cpp
            else if (address.transportStyle == eTransportSSU2)
            {
                WriteString ("SSU2", s);
                // caps
                std::string caps;
                if (isPublished)
                {
                    if (address.IsPeerTesting ()) caps += CAPS_FLAG_SSU2_TESTING;
                    if (address.IsIntroducer ()) caps += CAPS_FLAG_SSU2_INTRODUCER;
                }
                else
                {
                    if (address.IsV4 ()) caps += CAPS_FLAG_V4;
                    if (address.IsV6 () || address.host.is_v6 ()) caps += CAPS_FLAG_V6;
                    if (caps.empty ()) caps += CAPS_FLAG_V4;
                }
```

`isPublished` is the router's own reachability (`libi2pd/RouterInfo.cpp:1333`):

```cpp
bool isPublished = address.published && !address.host.is_unspecified () && address.port;
```

So the reference publishes the **role** letters on the form that carries an
endpoint, and the **family** letters on the form that does not. The `C` letter
never rides the unpublished form.

The tree publishes the **family** letter on both forms. It keeps that: the
tree's netdb requires the family letter to match the address's own family, and
dropping it would lose that check. The tree therefore publishes the
**combination** `"4C"` / `"6C"` where the reference publishes the role alone.
Both are inside the specification's alphabet, and both references parse a
combination (§3.7).

### 3.2 The reference's peer-side check requires an endpoint

A peer decides whether a router is an introducer from the **advertised**
capability, and the reference's check demands a published endpoint
(`libi2pd/RouterInfo.cpp:1055`-`:1059`):

```cpp
	bool RouterInfo::IsSSU2Introducer (bool v4) const
	{
		if (!(m_SupportedTransports & (v4 ? eSSU2V4 : eSSU2V6))) return false;
		auto addr = (*GetAddresses ())[v4 ? eSSU2V4Idx : eSSU2V6Idx];
		return addr && addr->IsIntroducer () && !addr->host.is_unspecified () && addr->port;
	}
```

A router that publishes no host and no port is **not** an introducer to a
reference peer, however its `caps` read. The `C` therefore rides the **direct**
form and only the direct form, in this tree exactly as in the reference.

The reference's own ask condition uses the same advertised flag
(`libi2pd/SSU2Session.cpp:753`):

```cpp
if (GetRouterStatus () == eRouterStatusFirewalled && m_Address->IsIntroducer ())
```

and the reference resolves an introducer by hash through the network database,
requiring the introducer's own published address
(`libi2pd/SSU2.cpp:961`, `:994`).

### 3.3 The reference clears the role for a floodfill

The role letter is a property of an **ordinary** router
(`libi2pd/RouterContext.cpp:502`-`:506`):

```cpp
				if (publish)
				{
                    UpdateSSU2AddressCapsTesting (address, true);
                    UpdateSSU2AddressCapsIntroducer (address, !m_IsFloodfill);
				}
```

and a floodfill router's SSU2 addresses have it **cleared**
(`libi2pd/RouterContext.cpp:655`-`:656`), while a published non-floodfill
router's have it set (`:669`, `:671`).

The tree's floodfill builder qualifies its address with the **direct** rule
(`crates/i2pr-netdb/src/local.rs:491`-`:499`):

```rust
        if addresses
            .iter()
            .filter(|address| address.transport_style() == "SSU2")
            .count()
            != 1
            || !addresses.iter().any(is_qualified_ssu2_address)
        {
            return Err(LocalRouterInfoError::UnqualifiedFloodfillAddress);
        }
```

so a role-bearing address is **refused** by the floodfill path, and the
reference's floodfill exclusion is preserved without touching the floodfill
builder.

### 3.4 The tree's direct rule refuses a role-bearing address

`crates/i2pr-netdb/src/floodfill_role.rs:197`:

```rust
pub fn is_qualified_ssu2_address(address: &RouterAddress) -> bool {
```

requires, among much else, that the capability string be **exactly** the
address's own family (`crates/i2pr-netdb/src/floodfill_role.rs:248`-`:251`):

```rust
        || match host {
            core::net::IpAddr::V4(_) => caps != "4",
            core::net::IpAddr::V6(_) => caps != "6",
        }
```

A `"4C"` address is therefore **refused** by the netdb's own direct rule, and
would never be signed. The tree cannot publish the role letter today.

### 3.5 The tree's introducer rule already forbids the role letter

`crates/i2pr-netdb/src/floodfill_role.rs:277`:

```rust
pub fn is_qualified_ssu2_introducer_address(address: &RouterAddress) -> bool {
```

requires the capability string to carry **only** family letters
(`crates/i2pr-netdb/src/floodfill_role.rs:306`):

```rust
    if caps.is_empty() || !caps.bytes().all(|byte| byte == b'4' || byte == b'6') {
        return false;
    }
```

That rule is **unchanged** by this plan, and it is what keeps the role letter
off the introducer-only form: the specification and the reference both publish
the role only on the form that carries an endpoint, and this rule refuses the
letter on the form that does not. The exclusion is therefore *executable*, not
a convention.

### 3.6 The role owner is unreachable — a latent defect in Plan 499's live path

Plan 490 made the introducer role **available** behind the explicit
`ssu2.introducer_service` opt-in and removed the parse-time refusal
(`plans/closure/network-participation/490-status.md`, R8). Plan 498 added the
role owner to the daemon's composition, and Plan 499 added the carriage and
the live drain.

Plan 490's closure record also moved the readiness contract's control to where
the invariant is then enforced — the composition's refusal — and wrote:

> "the router never publishes an introducer address — now lives […] SSU2
> service when the introducer role is requested"

(`plans/closure/network-participation/490-status.md`, *Findings*, item 4).

Plan 520 then lifted the **advertisement** refusal from that same guard and
kept the **role** refusal
(`crates/i2pr-daemon/src/lib.rs:3973`-`:3991`):

```rust
                    // Strict profile is enforced twice: once at config
                    // parse time and again here so a future bypass
                    // cannot reach socket ownership.
                    //
                    // Plan 520: the router's **own** address publication is
                    // behind an explicit opt-in and is no longer refused
                    // here. The introducer role still is.
                    if ssu2_config.introducer_service {
                        let detail = i2pr_core::HealthDetail::new(
                            "SSU2 service requires the strict controlled profile",
                        )
                        .ok();
                        return i2pr_runtime::ServiceResult::Failed(
                            i2pr_core::ServiceFailure::new(
                                i2pr_core::ServiceFailureCategory::InvalidState,
                                detail,
                            ),
                        );
                    }
```

That guard is **unconditional** in the SSU2-enabled path. The role owner the
composition constructs further down
(`crates/i2pr-daemon/src/lib.rs:4719`):

```rust
                    let mut introducer = if ssu2_config.introducer_service {
```

is therefore **unreachable**: with `ssu2.enabled = true` the guard returns
`Failed` before the construction is reached, and with `ssu2.enabled = false`
the service takes the early-return path. `ssu2.introducer_service = true` is
refused at configuration-parse time unless the SSU2 service runs
(`crates/i2pr-daemon/src/config.rs:3516`), so **there is no configuration in
which the role owner is constructed at all**.

Plan 499's closure record states the opposite:

> "An operator enables it with `ssu2.introducer_service = true` **and**
> `ssu2.enabled = true`; the daemon's live loop then constructs the role
> owner and the carriage."

(`plans/closure/network-participation/499-status.md`, *Documentation and
operations*.)

**That claim is false of the tree.** Plan 499's evidence is a local three-party
UDP mesh that constructs the role owner directly, so the mesh never exercised
the composition's reachability. The closed record is **not** rewritten; the
correction lives in this plan's closure record and in the registry row.

The defect is the advertisement's **precondition**: a router that advertises
`C` while its role owner is unreachable makes a claim it cannot honour. This
plan therefore delivers the advertisement **and** the role's reachability, and
§5.2 records the reachability change as an explicit part of the scope.

### 3.7 Both references parse a family-and-role combination

The pinned i2pd parses the capability string **per character**
(`libi2pd/RouterInfo.cpp:562`-`:581`):

```cpp
	uint8_t RouterInfo::ExtractAddressCaps (std::string_view value) const
	{
		uint8_t caps = 0;
		for (auto cap: value)
		{
			switch (cap)
			{
				case CAPS_FLAG_V4:
					caps |= AddressCaps::eV4;
				break;
				case CAPS_FLAG_V6:
					caps |= AddressCaps::eV6;
				break;
				case CAPS_FLAG_SSU2_TESTING:
					caps |= AddressCaps::eSSUTesting;
				break;
				case CAPS_FLAG_SSU2_INTRODUCER:
					caps |= AddressCaps::eSSUIntroducer;
				break;
```

so `"4C"` yields both the IPv4 bit and the introducer bit.

The pinned Java I2P `2.13.0`
(`9134f808337b401e8e53c73734c81fab04280c9d`) publishes the same letters
(`router/java/src/net/i2p/router/transport/udp/UDPTransport.java:3035`-`:3051`)
and reads them with a containment test (`:2588`, `:4185`), so the combination
and its order are both immaterial to it.

The tree's own parser is per-character as well
(`crates/i2pr-transport-ssu2/src/address.rs:1314`-`:1345`), so `"4C"` parses
here too.

## 4. Invariants that must not regress

1. **The direct rule stays the direct rule.** `is_qualified_ssu2_address` and
   `build_with_ssu2_address` are unchanged. The role-bearing address is
   qualified by a **third** rule, beside them, never over them.
2. **The introducer rule stays the introducer rule.** The introducer-only
   form never carries the role letter, and the rule that refuses it is
   unchanged.
3. **The floodfill path never carries the role letter.** The floodfill builder
   qualifies with the direct rule and is unchanged.
4. **The role letter is advertised only while the role is live.** A router
   whose role owner is not running publishes **no** `C`.
5. **A record that carries the role letter is never installed while the role
   is not live.** The runtime's install gate refuses it.
6. **Nothing is advertised.** `router.version` stays `0.9.62`, the published
   RouterInfo's `caps` string changes only behind the existing default-off
   opt-in, and the support inventory's advertised count stays **zero**.
7. **Every refusal is counted under its own reason.** The role rule's refusal
   is not the direct rule's refusal, and neither collapses into the other.
8. **The role stays default-off.** `ssu2.introducer_service` keeps its
   default and its parse-time refusals.
9. **No new configuration key.** The role's switch is `ssu2.introducer_service`
   and the publication's switch is `ssu2.advertise`. The `C` rides both.
10. **No new task, timer, listener or socket.** The advertisement rides the
    daemon's existing publication cycle.
11. **No secret crosses a boundary.** The published material carries the
    router's **public** keys only.
12. **The wire vocabulary is untouched.** The plan renders an address; it
    defines no block, no option and no codec.

## 5. Scope

### 5.1 In scope

- The **runtime**: the direct form's capability string carries the role letter
  when the role is live, and the material says so in a typed field.
- The **runtime**: the install gate refuses a role-bearing record while the
  role is not live.
- The **netdb**: a **third** qualification rule for the role-bearing direct
  address, and a **third** builder that signs it.
- The **daemon**: the self-publication owner publishes the role-bearing address
  through the netdb's third builder, and counts it under its own reason.
- The **daemon composition**: the introducer role becomes reachable behind its
  explicit, default-off opt-in (§3.6), so the advertisement can be truthful.
- The **readiness contract**: the control that pinned the blanket refusal is
  repinned at the place where the invariant is now enforced.
- The lane, the guard, the documentation and the support inventory.

### 5.2 Out of scope

- **The `B` (peer-test) advertisement.** The specification's other role
  letter. The tree's peer test is Plan 521's opt-in and is a separate matter.
- **The `caps=f` advertisement.** Plan 494 owns it and it is blocked on Plan
  493's two-family evidence.
- **The peer-test role's own advertisement** in the reference's combined
  `"BC"` form. The tree publishes the role it actually serves and nothing else.
- **The stock-peer half.** The plan's evidence is a local, in-process lane.
  Evidence against a stock peer needs the authorized two-host topology Plan
  493 owns, and it stays blocked there.
- **Any change to the transport's option vocabulary, its parser, or the
  netdb's two existing rules.**
- **Any change to the `B` bit, the peer-test machinery, or the transport's
  introducer table.**

## 6. Required production changes

1. **`i2pr-runtime`** — `Ssu2PublicationMaterial` gains a typed field saying
   whether the address advertises the introducer role. The direct form's
   capability string carries the role letter **only** when the role is live;
   the introducer-only form never carries it.
2. **`i2pr-runtime`** — the install gate refuses a record whose SSU2 address
   advertises the role while the role is not live.
3. **`i2pr-netdb`** — a **third** qualification rule accepts the direct
   address whose capability string is exactly the address's own family letter
   plus the role letter, and a **third** builder signs it. The first rule and
   the first builder are untouched.
4. **`i2pr-daemon`** — the self-publication owner matches on the material's
   typed field, hands the address to the netdb's third builder, and counts the
   role-bearing publication under its own reason.
5. **`i2pr-daemon`** — the composition's blanket refusal is replaced by the
   role's own explicit, default-off opt-in, so the role owner is reachable.
6. **`scripts/check-router-readiness-contract.py`** — the control that pinned
   the blanket refusal is repinned at the new enforcement point, and a control
   for the opt-in is added. The script's self-test must still give every
   control teeth.

## 7. Design decisions the plan must make explicit

**D1 — The `C` rides the direct form only.**

The reference publishes the role letters on the published form only
(`libi2pd/RouterInfo.cpp:1350`-`:1373`), and its peer-side check requires an
endpoint for an introducer (`libi2pd/RouterInfo.cpp:1059`). The tree's
introducer-only form publishes no endpoint, so it can never be an introducer
to a reference peer, and advertising `C` on it would be a claim no reference
peer would honour. The tree's own introducer rule already refuses the letter
on that form (§3.5), so the exclusion is enforced twice: by the rule and by
the composition.

**D2 — The tree publishes the combination `"4C"`, not the reference's `"C"`.**

The tree keeps its stricter requirement that the capability string names the
address's own family, and it adds the role letter to it. Dropping the family
letter to match the reference's shape would weaken the tree's own netdb rule
and would make the published address say nothing about which family it is for.
The specification's alphabet is `[B,C,4,6]` and both references parse the
combination (§3.7), so the combination is inside the vocabulary.

**D3 — The role letter is advertised only while the role is live.**

The advertisement is a claim the router must be able to honour. The runtime
owns the fact (`introducer_service_enabled`,
`crates/i2pr-runtime/src/ssu2_runtime.rs:2505`), and it owns the wire, so it
composes the letter. The daemon never decides whether the role is live.

**D4 — The role letter is refused, not merely omitted, when the role is not
live.**

A silent omission would leave a router that once advertised the role and no
longer does indistinguishable from a router that never advertised it. The
runtime's install gate refuses the whole record in that case, so the router's
published record **cannot** carry a stale role letter.

**D5 — The third rule is beside the first two, never over them.**

`is_qualified_ssu2_address` still demands the capability string be exactly
the family, and `build_with_ssu2_address` still refuses `"4C"`. The new rule
is a **third** rule with a **third** builder, on the pattern Plan 523
established for the introducer form. A reader of the netdb can tell which
rule admitted an address, and the daemon's refusal reason names the rule.

**D6 — The netdb's rule is exact, not permissive.**

The role-bearing rule accepts exactly one capability string per family: the
family letter and the role letter, in that order. It does not accept `"BC"`,
`"4CB"`, `"46C"` or any other arrangement, and it does not accept the peer-test
letter at all. A permissive rule would let an unearned role reach the wire.

**D7 — The composition's blanket refusal becomes the role's own opt-in.**

The guard at `crates/i2pr-daemon/src/lib.rs:3980` refused the **role**, not
the **publication**, and Plan 490's record says the role was meant to be
reachable behind its opt-in. Plan 526 removes the blanket refusal so the
opt-in is the only gate. The opt-in keeps its default (`false`), its
parse-time refusal when the SSU2 service is off
(`crates/i2pr-daemon/src/config.rs:3516`), and the role owner's own
default-off behaviour, so an ordinary deployment is unchanged.

**D8 — The readiness contract's control is repinned, not dropped.**

The control "introducer publication must remain rejected" pinned a *refusal*.
After this plan the refusal still exists, but it is the refusal of a
**role-bearing record while the role is not live**, enforced by the runtime's
install gate. The control's invariant is preserved and its enforcement point
moves — the same treatment Plan 490 gave it. A second control pins the opt-in
that now gates the role's construction. The script's mutation self-test must
prove both controls still have teeth.

**D9 — No new configuration key.**

`ssu2.introducer_service` is the role's switch and `ssu2.advertise` is the
publication's switch. The `C` appears when **both** are on and the role is
live. Splitting one behaviour across a third key would make the operator
reason about which key produces which letter.

**D10 — The private-LAN profile's publication refusal stays.**

`ssu2.advertise` is refused outside the controlled loopback profile
(`crates/i2pr-daemon/src/config.rs:3461`,
`crates/i2pr-daemon/src/router_i2np.rs:1579`). The `C` rides the published
address, so it is confined to the same profile. §14 records this as a
limitation with its owner.

**D11 — The counters distinguish the role-bearing publication.**

The role-bearing direct form is a **third** published form. It gets its own
counter, and the aggregate `published` counter becomes the sum of the three.
The direct rule's refusal and the role rule's refusal get their own reasons.

**D12 — The advertisement is not a promotion.**

The support inventory's new surface is `experimental` with
`advertised = false`, the inventory's advertised count stays **zero**, and
`router.version` stays `0.9.62`. The published RouterInfo's `caps` string
changes only behind the existing default-off opt-in, exactly as Plan 520's
`ssu2.advertise` does.

## 8. Work packages

### WP1 — the role becomes reachable

The composition's blanket refusal is replaced by the role's own explicit
opt-in, so the role owner the composition already constructs is reachable.
A regression test drives the composition and proves the role owner is
constructed when the opt-in is on, and that the service still starts.

### WP2 — the runtime composes the role letter

The direct form's capability string carries the role letter when the role is
live, the introducer-only form never carries it, and the material says in a
typed field which of the two happened. A regression test proves the letter
appears only while the role is live.

### WP3 — the netdb gains the third rule and the third builder

The role-bearing direct address is qualified by the new rule and signed by
the new builder. The first rule and the first builder are unchanged, and a
regression test proves each refuses the other's address.

### WP4 — the install gate refuses a stale role letter

A record whose address advertises the role is refused while the role is not
live, so a stale letter cannot reach the wire.

### WP5 — the daemon publishes and counts the third form

The owner matches on the material's typed field, hands the address to the
netdb's third builder, and counts the role-bearing publication under its own
reason.

### WP6 — the lane, the guard, the documentation and the inventory

The lane drives the real runtime and the real daemon. The guard pins the shape
of the plan. The documentation and the support inventory record the new
surface.

## 9. Failure, cancellation, restart, and contention semantics

- **Failure.** Every refusal is counted under its own reason and leaves no
  state behind. A poisoned state mutex degrades to "no material", never to a
  fabricated one. A netdb refusal never signs a record, and a record carrying
  a role letter the runtime refuses is never installed.
- **Cancellation.** The plan adds no task of its own. The publication rides
  the daemon's **existing** publication cycle
  (`crates/i2pr-daemon/src/lib.rs:3666`), and the runtime's material builders
  are synchronous and non-awaiting.
- **Restart.** The role's liveness is a configuration fact, so a restart with
  the opt-in on resumes advertising the role. A router that restarts with the
  opt-in off publishes no role letter. There is no stale state to reconcile.
- **Contention.** The plan adds no queue, no task and no lock of its own. The
  role's liveness is read from the runtime's existing configuration, and the
  publication cycle is the daemon's existing one.

## 10. Compatibility and migration review

- **No wire change.** The plan renders a capability string the specification
  already defines. The proto crate, the option vocabulary and the parser are
  untouched.
- **A reading change for peers.** A peer that reads this router's record may
  now find the role letter in the `caps` string. Both references parse it
  (§3.7), and the specification's alphabet contains it.
- **A writing change for this router's own record**, behind the existing
  default-off opt-in and only while the role is live. A router with
  `ssu2.advertise = true` and `ssu2.introducer_service = true` publishes a
  different `caps` string than it did before. This is the plan's purpose.
- **A public API change in the runtime and the daemon.** The material grows a
  typed field, the netdb grows a rule and a builder, and the daemon's
  counters split. The crates are experimental and the lanes are updated with
  the plan; no external consumer exists.
- **No configuration migration.** No new key, no changed default, no changed
  meaning. `ssu2.introducer_service` keeps its default and its refusals.
- **No persistence.** The published form is recomputed from live state on
  every publication cycle.

## 11. Required tests

### The role's reachability

- the composition constructs the role owner when the opt-in is on;
- the composition does **not** construct it when the opt-in is off;
- the service still starts in both cases;
- the opt-in's default is `false` and its parse-time refusal when the SSU2
  service is off is unchanged.

### The runtime's composition

- a router whose role is live publishes the role letter alongside the family
  letter, and the material says the role is advertised;
- a router whose role is not live publishes **no** role letter, and the
  material says the role is not advertised;
- the introducer-only form carries the family letter and **never** the role
  letter, in either state;
- the capability string is the family letter followed by the role letter, and
  nothing else;
- a router with no bound socket is refused with its own reason.

### The netdb's third rule and builder

- the role-bearing direct address is accepted by the new rule and signed by
  the new builder;
- the **first** rule refuses the role-bearing address, and the **first**
  builder refuses it;
- the introducer rule refuses the role-bearing address;
- the role rule refuses `"BC"`, `"4CB"`, `"46C"`, `"4"` and `"6"`;
- the role rule refuses an address whose family letter does not match its
  host;
- the role-bearing record carries **no** `f` capability;
- the floodfill builder still refuses the role-bearing address.

### The runtime's install gate

- a role-bearing record is installed while the role is live;
- a role-bearing record is **refused** while the role is not live;
- the refusal names the install gate's own reason.

### The daemon's hand-off and counters

- the role-bearing address is signed by the netdb's **third** builder, and the
  other two forms by their own builders;
- the role-bearing publication is counted under its own reason, and the
  aggregate `published` counter is the sum of the three forms;
- the direct rule's refusal and the role rule's refusal are counted under
  their own reasons;
- the withdrawal still yields a record with **no** address.

### The negative controls

Every guard rule needs an inverted probe that fires it, and every probe
mutates an **in-memory copy** of the source, so no file on disk is written.
Every guard rule must **fail closed** when the item it names is absent: a
rule whose named item is missing is **reported**, not passed (Plan 523's
precedent, Plan 524's proof).

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_introducer_advertisement -- --test-threads=1
cargo test --locked -p i2pr-daemon --test ssu2_introducer_advertisement_boundary -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
python3 scripts/check-router-readiness-contract.py
python3 scripts/check-router-readiness-contract.py --self-test
```

## 13. Documentation updates

- `specs/support.toml` — a new surface for the advertisement, `experimental`
  and `advertised = false`.
- `specs/CONFORMANCE.md` — the section for this plan and the inventory counts.
- `specs/protocols/09-ssu2.md` — the advertisement scope.
- `specs/QUALIFICATION-GATES.md` — the gates for this plan.
- `docs/architecture/i2pr-transport-ssu2.md` — the role letter and the
  combination.
- `docs/architecture/i2pr-netdb.md` — the third rule and the third builder.
- `docs/architecture/i2pr-runtime.md` and `docs/architecture/i2pr-daemon.md` —
  the composition, the install gate and the counters.

## 14. Acceptance criteria and stop conditions

The plan passes when:

1. the introducer role is **reachable** in the composition behind its
   explicit, default-off opt-in, and the service still starts;
2. a router whose role is live publishes the role letter alongside its family
   letter, and a router whose role is not live publishes **no** role letter;
3. the role letter rides the **direct** form only, and the introducer-only
   form never carries it;
4. the netdb's **third** rule accepts the role-bearing address and the
   **third** builder signs it, while the first rule, the first builder, the
   introducer rule and the floodfill builder all still refuse it;
5. a role-bearing record is **refused** by the install gate while the role is
   not live;
6. the counters distinguish the three published forms and the withdrawal, and
   the role rule's refusal has its own reason;
7. the lane and the guard are green, every guard rule has an inverted probe
   that fires it, every rule fails closed on an absent item, and no file on
   disk is written by a probe;
8. the routine floor is green on the closing tree, and the readiness
   contract's self-test still gives every control teeth;
9. `specs/support.toml` records the surface as `experimental` and
   `advertised = false`; the advertised count stays **zero**;
10. `router.version` stays `0.9.62`; `Cargo.toml` and `Cargo.lock` are
    byte-identical;
11. no boundary script is weakened.

**Stop conditions.** The plan stops and is re-scoped, rather than widened, if:

- making the role reachable turns out to need a change to the opt-in's
  default or to its parse-time refusals;
- advertising the role turns out to need the `B` letter as well;
- the third rule turns out to need to accept anything beyond the exact
  family-and-role string;
- the advertisement turns out to need a new task, a new timer or a new
  listener;
- the counters turn out to need a second aggregate.

**Known limitations, recorded not hidden.**

- The **stock-peer half** stays blocked on Plan 493's authorized two-host
  topology. The plan's evidence is a local, in-process lane.
- The advertisement is confined to the **controlled loopback profile**, because
  `ssu2.advertise` is refused outside it (D10). Under that profile the router
  binds loopback, so the `C` is a local qualification claim and not yet a
  network-reachable one. A publication-capable profile is Plan 493's and
  Plan 494's territory.
- The **`B` advertisement** is not this plan's subject (§5.2).
- The **peer's** view of the role is Plan 524's acquisition and is closed.

## 15. Closure evidence required

- The commits that carry each work package.
- A requirement-to-evidence matrix: every acceptance criterion above, the
  file and line that satisfies it, and the test that proves it.
- The exact commands run, with their outcomes.
- The invariant, failure, migration and security reviews of §4, §9 and §10.
- The negative controls: every mutation applied, the test it broke, and the
  proof that no file on disk was written.
- The findings by severity, each either fixed or recorded with its owner.
- The correction to Plan 499's closed claim about the role's live path, in
  this plan's closure record and in the registry row — never in Plan 499's
  own record.
- The roadmap disposition and the unblock audit.

## 16. Handoff notes

- The plan's subject is the **advertisement** of the router's own introducer
  role. The **form** is Plan 523's, the **acquisition** is Plan 524's, and the
  **publication** is Plan 525's; all three are closed.
- The plan also repairs the **reachability** of the role, because a truthful
  advertisement requires a live role. Plan 499's closed record claims the
  composition constructs the role owner; that claim is false of the tree and
  the correction lives here.
- The role letter is **never** a promotion: the inventory's advertised count
  stays zero and `router.version` stays pinned.
- The **`B` advertisement**, the **`caps=f` advertisement** and the
  **stock-peer half** all stay out of scope, with their owners named.

## 17. Amendment log

### 17.1 — 2026-11-10, registration

The plan is registered as **526**, the next free number. §3 records the
evidence as it stood at registration: the reference's publication shape, the
reference's peer-side check, the reference's floodfill exclusion, the tree's
two netdb rules, the tree's own parser, and the **unreachable role owner**
that the advertisement's truthfulness depends on.

§3 is evidence and is **not** rewritten. Any later correction is added here as
an amendment, additively.
### 17.2 — 2026-11-10, the guard extensions and the readiness repin

Three guard surfaces are extended by this plan. Each extension is a
**strengthening**: the extended rule names the new location **as well as** the
old one, and it fails closed when either named item is absent. No other plan's
guard file is touched, and no existing assertion is relaxed.

| Guard | Rule | Extension |
|---|---|---|
| `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs` | Plan 520's "the family capability is carried" | The rule now follows the composition into the runtime's `capabilities_for` helper, and it fails closed if that helper is absent. |
| `crates/i2pr-daemon/tests/router_info_publication_address_boundary.rs` | Plan 520's "the record is installed and published" | The rule now accepts the install inside the daemon's new `apply` seam **and** requires `refresh` to call `self.apply(`. |
| `crates/i2pr-daemon/tests/ssu2_introducer_publication_boundary.rs` | Plan 525's "the daemon matches on the typed form" | The rule now requires the `published_form` helper to exist, to read `material.form`, to name both runtime forms, and to be called by `build_record`. |
| `crates/i2pr-daemon/tests/ssu2_introducer_publication_boundary.rs` | Plan 525's "the counters distinguish the forms" | The rule now names all three counters and requires the three-term aggregate. |

`scripts/check-router-readiness-contract.py` — the control **"introducer
publication must remain rejected"** pinned the daemon's blanket refusal. That
refusal is removed by this plan (§3.6), so the control is **repinned**, not
dropped: the invariant it guarded — *the router never publishes an introducer
role it does not serve* — still exists, and its enforcement point moves to the
runtime's install gate.

| Control | Needle | File |
|---|---|---|
| "introducer publication must remain rejected unless the role is live" | `parsed.capabilities().supports_relay() && !self.introducer_service_enabled()` | `ssu2_runtime` |
| "the introducer role must be constructed only behind the explicit opt-in" | `let mut introducer = if ssu2_config.introducer_service {` | `daemon` |

Both invocations pass — `python3 scripts/check-router-readiness-contract.py`
and `python3 scripts/check-router-readiness-contract.py --self-test` — and the
self-test still proves that every control has teeth by mutating an in-memory
copy of each needle.

### 17.3 — 2026-11-10, citation drift

§3 is evidence and is **not** rewritten. This plan's own production change
moved lines in four files, so the citations that point at those files no
longer land on the text they quoted. The table below is the additive pointer
to the current text. Every citation in the plan that is **not** listed here
still lands where it did at registration.

| Quoted citation | Current pointer | What moved |
|---|---|---|
| `crates/i2pr-daemon/src/lib.rs:3973`-`:3991` — the blanket refusal (§3.6) | `crates/i2pr-daemon/src/lib.rs:3973`-`:3992` | The refusal is **removed**; the site now carries the accurate comment that explains why, and names the opt-in that replaced it. |
| `crates/i2pr-daemon/src/lib.rs:3980` — the guard (§7 D7) | `crates/i2pr-daemon/src/lib.rs:3980`-`:3992` | As above. |
| `crates/i2pr-daemon/src/lib.rs:4719` — the role owner construction (§3.6) | `crates/i2pr-daemon/src/lib.rs:4720` | One line: the removed refusal's closing lines. |
| `crates/i2pr-netdb/src/floodfill_role.rs:277` — the introducer rule (§2, §3.5) | `crates/i2pr-netdb/src/floodfill_role.rs:365` | The third rule was inserted at `:283`, pushing the introducer rule down by 88 lines. |
| `crates/i2pr-netdb/src/floodfill_role.rs:306` — the introducer rule's caps check (§3.5) | `crates/i2pr-netdb/src/floodfill_role.rs:394` | As above. |
| `crates/i2pr-netdb/src/local.rs:491`-`:499` — the floodfill builder's qualification (§3.3) | `crates/i2pr-netdb/src/local.rs:522`-`:530` | The third builder was inserted at `:323`, pushing the floodfill builder down by 31 lines. |
| `crates/i2pr-runtime/src/ssu2_runtime.rs:2505` — `introducer_service_enabled` (§2, §7 D3) | `crates/i2pr-runtime/src/ssu2_runtime.rs:2514` | The material's new field and the composition helpers were added above it. |
| `crates/i2pr-daemon/src/self_publication.rs:397` — the self-publication path (§2) | `crates/i2pr-daemon/src/self_publication.rs:453` | The `decide`/`refresh` split added the `apply` seam at `:486`. |

Citations that did **not** move, and therefore still stand as quoted:
`crates/i2pr-transport-ssu2/src/address.rs:426`-`:475`,
`crates/i2pr-transport-ssu2/src/address.rs:1314`-`:1345`,
`crates/i2pr-netdb/src/floodfill_role.rs:197`,
`crates/i2pr-netdb/src/floodfill_role.rs:248`-`:251`,
`crates/i2pr-netdb/src/local.rs:268`,
`crates/i2pr-netdb/src/local.rs:292`,
`crates/i2pr-daemon/src/lib.rs:3666`,
`crates/i2pr-daemon/src/config.rs:3461`,
`crates/i2pr-daemon/src/config.rs:3516`,
`crates/i2pr-daemon/src/router_i2np.rs:1579`.
