#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_file="$root/crates/i2pr-tunnel/src/transit.rs"
daemon_transit="$root/crates/i2pr-daemon/src/transit_compose.rs"
daemon_transit_owner="$root/crates/i2pr-daemon/src/transit_owner.rs"
daemon_lib="$root/crates/i2pr-daemon/src/lib.rs"
daemon_transit_composition="$root/crates/i2pr-daemon/src/transit_composition.rs"
daemon_root="$root/crates/i2pr-daemon/src"

# Plan 502: `--self-test` exercises the rule controls alone, so they can be
# proven on a tree whose Rust side has not landed yet. The default mode
# runs the controls *and* the production checks.
case "${1:-}" in
    "") transit_mode="scan" ;;
    --self-test) transit_mode="self-test" ;;
    -h|--help)
        printf 'usage: %s [--self-test]\n' "${BASH_SOURCE[0]}"
        exit 0
        ;;
    *)
        printf 'check-m11-transit-boundaries: unknown argument: %s\n' "$1" >&2
        exit 2
        ;;
esac

fail() { echo "check-m11-transit-boundaries: $*" >&2; exit 1; }

# One temporary root for everything this script builds.
tmp_root="$(mktemp -d)"
production_src="$tmp_root/production"
comment_stripped="$tmp_root/comment-stripped"
rule_python="$tmp_root/rule.py"
mkdir -p "$production_src" "$comment_stripped"
trap 'rm -rf "$tmp_root"' EXIT

# The scoped production-code stripper, the mirror of the pre-existing
# production rules, and the Plan 502 rule-10 check live in ONE Python
# program, so they cannot drift apart. It is invoked twice: once to build
# the tree that every pre-existing rule below scans, and once -- after
# every pre-existing rule has run -- to judge the Plan 502
# transit-composition rule.
cat > "$rule_python" <<'PY'
"""Shared rule machinery for `scripts/check-m11-transit-boundaries.sh`.

Three things live here so that they cannot drift apart:

* the **scoped** production-code stripper, which builds the tree every
  pre-existing rule in the shell script scans;
* the **mirror** of those pre-existing rules, so the stripper's controls
  can prove that the tree is scanned the way the shell scans it; and
* the **Plan 502 rule-10 check**, which guards the transit data-plane
  owner in the product composition.

Modes
-----
build <repo_root> <dest_tree> <script_path>
    Build the scoped production tree and run the stripper's controls.
scan <lib.rs> <transit_composition.rs> <script_path>
    Run the rule-10 controls and judge the production tree.
self-test <script_path>
    Run both control sets. Tree-independent: no daemon source is read.
"""

import os
import re
import shutil
import sys
import tempfile

MODE = sys.argv[1]
failures = []


def fail(message):
    failures.append(message)


def read(path):
    try:
        with open(path, "r", encoding="utf-8") as handle:
            return handle.read()
    except OSError as exc:
        fail(f"cannot read {path}: {exc}")
        return ""


# ---------------------------------------------------------------------------
# the scoped stripper
# ---------------------------------------------------------------------------

CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")
RAW_START = re.compile(r"(?<![A-Za-z0-9_])(?:r|br|rb)(#*)\"")
LINE_COMMENT = re.compile(r"^[ \t]*//")


def mask(text):
    """Blank comments and literals, preserving the line structure.

    The rules judge executable code, not prose: the production sources
    name the very symbols the rules forbid, in their documentation. A
    mention in a comment must neither satisfy a rule nor trip one.
    """
    out = []
    index = 0
    length = len(text)
    while index < length:
        char = text[index]
        raw = RAW_START.match(text, index)
        if raw is not None:
            # A raw string: `r"..."`, `r#"..."#`, `br##"..."##`. It ends at
            # a quote followed by the same number of `#`s.
            closing = '"' + raw.group(1)
            end = text.find(closing, raw.end())
            end = length if end < 0 else end + len(closing)
            chunk = text[index:end]
            out.append("".join("\n" if c == "\n" else " " for c in chunk))
            index = end
            continue
        if char == "/" and text.startswith("//", index):
            end = text.find("\n", index)
            end = length if end < 0 else end
            out.append(" " * (end - index))
            index = end
            continue
        if char == "/" and text.startswith("/*", index):
            depth = 1
            end = index + 2
            while end < length and depth:
                if text.startswith("/*", end):
                    depth += 1
                    end += 2
                elif text.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            chunk = text[index:end]
            out.append("".join("\n" if c == "\n" else " " for c in chunk))
            index = end
            continue
        if char == '"':
            end = index + 1
            while end < length:
                if text[end] == "\\":
                    end += 2
                    continue
                if text[end] == '"':
                    end += 1
                    break
                end += 1
            chunk = text[index:end]
            out.append("".join("\n" if c == "\n" else " " for c in chunk))
            index = end
            continue
        if char == "'":
            literal = re.match(r"'(\\.|[^\\'])'", text[index:index + 4])
            if literal:
                out.append(" " * literal.end())
                index += literal.end()
                continue
            # A Rust lifetime (`'a`) is not a char literal.
            out.append(char)
            index += 1
            continue
        out.append(char)
        index += 1
    return "".join(out)


def test_regions(lines):
    """Per-line flag: is this line inside an `#[cfg(test)]` region?

    Scoped, never latched: a test region ends where the item it
    annotates ends -- at the matching close brace, or at the `;` of a
    brace-less annotation.

    The blunt convention this script used before -- "drop everything from
    the first `#[cfg(test)]` to end-of-file" -- is a latching defect, the
    same defect Plan 366 fixed in `check-console-boundaries.sh`. The
    daemon's `lib.rs` declares a test-only module at line 14 and
    `service_product.rs` at line 137, so the blunt form truncated the
    daemon's production composition to 13 of its 5770 lines and hid the
    production inbound path from every rule that scans the tree.
    """
    suppressed = [False] * len(lines)
    index = 0
    while index < len(lines):
        if not CFG_TEST.search(lines[index]):
            index += 1
            continue
        # Find the item the annotation applies to, skipping any further
        # attribute lines.
        item = index
        while item < len(lines):
            stripped = lines[item].strip()
            if stripped and not stripped.startswith("#["):
                break
            item += 1
        if item >= len(lines):
            for k in range(index, len(lines)):
                suppressed[k] = True
            break
        if "{" in lines[item]:
            for k in range(index, item):
                suppressed[k] = True
            depth = 0
            opened = False
            k = item
            while k < len(lines):
                depth += lines[k].count("{") - lines[k].count("}")
                if lines[k].count("{"):
                    opened = True
                suppressed[k] = True
                if opened and depth <= 0:
                    break
                k += 1
            index = k + 1
        else:
            for k in range(index, item + 1):
                suppressed[k] = True
            index = item + 1
    return suppressed


def production_source(text):
    """The file's production code, with every `#[cfg(test)]` region blanked.

    The removed lines are blanked rather than deleted, so the line numbers
    the rules report are the source file's own.
    """
    lines = mask(text).split("\n")
    suppressed = test_regions(lines)
    original = text.split("\n")
    return "\n".join(
        "" if suppressed[index] else original[index]
        for index in range(len(original))
    )


def production_code(text):
    """The file's executable production code: comments and literals blanked."""
    lines = mask(text).split("\n")
    suppressed = test_regions(lines)
    return "\n".join(
        "" if suppressed[index] else lines[index]
        for index in range(len(lines))
    )


def blunt_source(text):
    """The convention this script used before: truncate at the first test.

    Kept only so the controls can prove that the scoped strip, and not
    this one, is what makes them fire.
    """
    kept = []
    for line in text.split("\n"):
        if CFG_TEST.search(line):
            break
        kept.append(line)
    return "\n".join(kept)


def comment_strip(text):
    """Drop the line comments, as the shell's `grep -v -E` does."""
    return "\n".join(
        line for line in text.split("\n") if not LINE_COMMENT.match(line)
    )


# ---------------------------------------------------------------------------
# the pre-existing rules that scan the production tree
# ---------------------------------------------------------------------------
#
# These mirror the shell rules verbatim, so the stripper's controls can
# prove that the scoped tree is scanned the way the shell scans it. The
# drift guard below asserts that every pattern still appears in the
# script's shell body, so the mirror cannot rot unnoticed.

PRODUCTION_RULES = [
    ("process_short_build_request", r"process_short_build_request\b", None),
    ("chacha20_transform", r"chacha20_transform\(", None),
    ("short_envelope_primitives",
     r"seal_short_reply\(|open_short_reply\(|seal_short_request\(|open_short_request\(",
     None),
    ("forward_participant_layer", r"forward_participant_layer\b", "plan253"),
    ("peer_index_btreemap", r"peer_index: BTreeMap", None),
    ("bandwidth_option_redecode",
     r'mapping\.get\(|mapping\(\).get\(|get\("m"\)|get\("r"\)|get\("l"\)|get\("b"\)',
     None),
    ("participant_forward_direct", r"TunnelLayerTransform::participant_forward", "comment"),
]


def production_rule_hits(production, comment_stripped):
    """(rule id, line number, line) for every pre-existing rule hit."""
    hits = []
    for rule_id, pattern, prefilter in PRODUCTION_RULES:
        regex = re.compile(pattern)
        source = comment_stripped if prefilter == "comment" else production
        for number, line in enumerate(source.split("\n"), 1):
            if prefilter == "plan253" and "// Plan 253 removes" in line:
                continue
            if regex.search(line):
                hits.append((rule_id, number, line.strip()))
    return hits


def shell_only(text):
    """The shell body, with any `<<'PY'` heredoc body blanked out.

    The anchors below are checked against this rather than the whole file,
    so that the anchor list cannot satisfy itself.
    """
    kept, inside = [], False
    for line in text.split("\n"):
        if line.rstrip().endswith("<<'PY'"):
            inside = True
            kept.append(line)
            continue
        if inside:
            if line.rstrip() == "PY":
                inside = False
                kept.append(line)
            else:
                kept.append("")
            continue
        kept.append(line)
    return "\n".join(kept)


def check_no_drift(script_path):
    """The shell rules must still be there; the mirror above cannot rot."""
    shell = shell_only(read(script_path))
    for _, pattern, _ in PRODUCTION_RULES:
        if pattern not in shell:
            fail(f"pre-existing production scan was removed: {pattern!r}")
    for anchor in ("// Plan 253 removes", "^[[:space:]]*//"):
        if anchor not in shell:
            fail(f"pre-existing production scan was removed: {anchor!r}")


# ---------------------------------------------------------------------------
# the stripper's controls
# ---------------------------------------------------------------------------
#
# The first fixture is the latching control: a test-only module stands
# above production code that contains every forbidden pattern. The scoped
# strip must scan that production code, and the blunt convention must
# miss it -- which is what proves the change is real and not cosmetic.
# The second fixture is the mirror image: every forbidden pattern sits
# inside a test region, and no rule may fire.

PRODUCTION_CODE_FIXTURE = """\
#[cfg(test)]
mod app_runtime_qualification;

pub fn pump(request: &ShortRequestRecord, payload: &mut [u8]) {
    let _ = process_short_build_request(request);
    let _ = chacha20_transform(payload);
    let _ = seal_short_request(payload);
    let _ = open_short_request(payload);
    let _ = seal_short_reply(payload);
    let _ = open_short_reply(payload);
    let _ = forward_participant_layer(payload);
}

pub struct PeerIndex {
    peer_index: BTreeMap<Hash, PeerId>,
}

pub fn read_options(mapping: &Mapping) {
    let _ = mapping.get("m");
    let _ = mapping().get("r");
    let _ = mapping.get("l");
    let _ = mapping.get("b");
}

pub fn transform(keys: &LayerKeys) {
    let _ = TunnelLayerTransform::participant_forward(keys);
}
"""

TEST_REGION_FIXTURE = """\
#[cfg(test)]
mod tests {
    pub fn pump(request: &ShortRequestRecord, payload: &mut [u8]) {
        let _ = process_short_build_request(request);
        let _ = chacha20_transform(payload);
        let _ = seal_short_request(payload);
        let _ = open_short_request(payload);
        let _ = seal_short_reply(payload);
        let _ = open_short_reply(payload);
        let _ = forward_participant_layer(payload);
    }

    pub struct PeerIndex {
        peer_index: BTreeMap<Hash, PeerId>,
    }

    pub fn read_options(mapping: &Mapping) {
        let _ = mapping.get("m");
        let _ = mapping().get("r");
        let _ = mapping.get("l");
        let _ = mapping.get("b");
    }

    pub fn transform(keys: &LayerKeys) {
        let _ = TunnelLayerTransform::participant_forward(keys);
    }
}

pub fn pump_production() {
    let _ = 1;
}
"""

# (label, source text, expected fired rule ids, substrings that must survive)
STRIPPER_CONTROLS = [
    (
        "positive/test-region-above-production-code",
        PRODUCTION_CODE_FIXTURE,
        {rule_id for rule_id, _, _ in PRODUCTION_RULES},
        ["pub fn pump(", "pub fn read_options(", "pub fn transform("],
    ),
    (
        "positive/forbidden-patterns-only-in-a-test-region",
        TEST_REGION_FIXTURE,
        set(),
        ["pub fn pump_production()"],
    ),
]


def run_stripper_controls():
    """Exercise the scoped strip against the pre-existing rules."""
    outcomes = []
    exercised = set()
    for label, text, expected, must_survive in STRIPPER_CONTROLS:
        production = production_source(text)
        fired = {
            rule_id
            for rule_id, _, _ in production_rule_hits(production, comment_strip(production))
        }
        outcomes.append((label, sorted(fired)))
        exercised |= fired
        if fired != expected:
            fail(
                f"control {label}: expected rules {sorted(expected)}, "
                f"reported {sorted(fired)}"
            )
        for needle in must_survive:
            if needle not in production:
                fail(
                    f"control {label}: the scoped strip removed production code "
                    f"that must survive: {needle!r}"
                )
        if not expected:
            continue
        # The blunt convention must miss this shape. If it did not, the
        # control would not distinguish the scoped strip from the
        # latching one, and the change would be cosmetic.
        blunt = blunt_source(text)
        blunt_fired = {
            rule_id
            for rule_id, _, _ in production_rule_hits(blunt, comment_strip(blunt))
        }
        if blunt_fired:
            fail(
                f"control {label}: the blunt convention also reported "
                f"{sorted(blunt_fired)}; the control does not distinguish the "
                "scoped strip from the latching one"
            )
    for rule_id, _, _ in PRODUCTION_RULES:
        if rule_id not in exercised:
            fail(f"no stripper control exercises the production rule {rule_id}")
    return outcomes


# ---------------------------------------------------------------------------
# Plan 502 rule 10: the transit owner in the product composition
# ---------------------------------------------------------------------------

PROBE = "controlled_transit_disabled_probe"
DISPATCH = "dispatch_transit_inbound"
BUILDER = "build_transit_owner"
LIVE_INBOUND = "handle_inbound"


def word(name):
    """An identifier as a whole word."""
    return re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"(?![A-Za-z0-9_])")


def call(name):
    """An identifier used as a call, turbofish included."""
    return re.compile(
        r"(?<![A-Za-z0-9_])" + re.escape(name) + r"\s*(?:::<[^;()]*>)?\s*\("
    )


class Source:
    """One production source file: its raw text and its executable code."""

    def __init__(self, path, text):
        self.path = path
        self.text = text
        self.code = production_code(text)


def rule_lib_references_owner_module(ctx):
    # The legacy assertion, unchanged: the production `lib.rs` must
    # reference the live-owner module.
    if "transit_owner" not in ctx.lib.text:
        return ["production lib.rs must reference the transit_owner live-owner module"]
    return []


def rule_lib_dispatches_through_composition(ctx):
    if not call(DISPATCH).search(ctx.lib.code):
        return [
            "production lib.rs must dispatch the inbound path through "
            "transit_composition::dispatch_transit_inbound; without that call "
            "there is no proof the production inbound path reaches an owner"
        ]
    return []


def rule_lib_has_no_direct_probe(ctx):
    if word(PROBE).search(ctx.lib.code):
        return [
            "production lib.rs must not call controlled_transit_disabled_probe "
            "directly; the no-owner fallback belongs to "
            "transit_composition::dispatch_transit_inbound, and a probe call "
            "beside a live owner is the vestigial shape this rule must reject"
        ]
    return []


def rule_lib_has_no_direct_owner_dispatch(ctx):
    if word(LIVE_INBOUND).search(ctx.lib.code):
        return [
            "production lib.rs must not call TransitLiveOwner::handle_inbound "
            "directly; the owner is reached only through "
            "transit_composition::dispatch_transit_inbound, which makes the "
            "whole owner/no-owner choice internally"
        ]
    return []


def rule_lib_constructs_the_owner(ctx):
    if not word(BUILDER).search(ctx.lib.code):
        return [
            "production lib.rs must construct the transit owner through "
            "transit_composition::build_transit_owner"
        ]
    return []


def rule_composition_keeps_the_disabled_probe(ctx):
    if not word(PROBE).search(ctx.comp.code):
        return [
            "transit_composition.rs must keep controlled_transit_disabled_probe "
            "as the no-owner fallback of dispatch_transit_inbound"
        ]
    return []


def rule_composition_reaches_the_live_owner(ctx):
    if not word(LIVE_INBOUND).search(ctx.comp.code):
        return [
            "transit_composition.rs must reach TransitLiveOwner::handle_inbound "
            "on the installed-owner branch of dispatch_transit_inbound"
        ]
    return []


def rule_composition_owns_the_builder(ctx):
    if not word(BUILDER).search(ctx.comp.code):
        return [
            "transit_composition.rs must own the build_transit_owner "
            "construction entry point"
        ]
    return []


RULES = [
    ("lib-owner-module", rule_lib_references_owner_module),
    ("lib-composition-dispatch", rule_lib_dispatches_through_composition),
    ("lib-no-direct-disabled-probe", rule_lib_has_no_direct_probe),
    ("lib-no-direct-owner-dispatch", rule_lib_has_no_direct_owner_dispatch),
    ("lib-owner-construction", rule_lib_constructs_the_owner),
    ("composition-disabled-probe", rule_composition_keeps_the_disabled_probe),
    ("composition-live-owner", rule_composition_reaches_the_live_owner),
    ("composition-builder", rule_composition_owns_the_builder),
]


class Context:
    def __init__(self, lib, comp):
        self.lib = lib
        self.comp = comp


def evaluate(lib, comp):
    """Every violation of the rule set, as (rule id, message) pairs."""
    context = Context(lib, comp)
    hits = []
    for rule_id, rule in RULES:
        for message in rule(context):
            hits.append((rule_id, message))
    return hits


# The controls for rule 10. The positive control is the shape Plan 502
# §6 and WP1/WP2 describe: the production `lib.rs` constructs the owner
# and dispatches through the composition module, and the composition
# module makes the whole owner/no-owner choice. The probe identifier
# appears in the `lib.rs` *comment* on purpose: a prose mention is not a
# call, and a rule that fired on prose would be a rule about
# documentation.
AGREED_LIB = """\
// Plan 502: the production inbound path reaches the transit owner through
// the composition module. `controlled_transit_disabled_probe` is the
// no-owner fallback inside `dispatch_transit_inbound` and is never called
// from here.
use crate::transit_composition::{build_transit_owner, dispatch_transit_inbound};
use crate::transit_owner::TransitLiveOwner;

pub fn register_ssu2_service(config: &Config) -> Result<(), DaemonError> {
    let owner: Option<TransitLiveOwner<ChaCha8Rng>> = build_transit_owner(config)?;
    let _ = dispatch_transit_inbound(&owner, &outcome, &inbound, now_ms);
    Ok(())
}
"""

AGREED_COMP = """\
use crate::transit_owner::{controlled_transit_disabled_probe, TransitLiveOwner};

pub(crate) fn build_transit_owner(
    config: &Config,
) -> Result<Option<TransitLiveOwner<ChaCha8Rng>>, TransitCompositionError> {
    Ok(None)
}

pub(crate) fn dispatch_transit_inbound(
    owner: &Option<TransitLiveOwner<ChaCha8Rng>>,
    outcome: &RouterI2npOutcome,
    inbound: &Ssu2InboundI2np,
    now_ms: u64,
) -> Result<(), TransitCompositionError> {
    match owner {
        Some(owner) => {
            owner.handle_inbound(inbound, now_ms)?;
        }
        None => {
            let _ = controlled_transit_disabled_probe(outcome);
        }
    }
    Ok(())
}
"""

# Plan 502 §3.4's vestigial shape: the production inbound path consults
# the disabled probe and discards the result. No owner is reached.
VESTIGIAL_LIB = """\
use crate::transit_owner::controlled_transit_disabled_probe;

pub fn pump(outcome: &RouterI2npOutcome) {
    let _ = controlled_transit_disabled_probe(outcome);
}
"""

# The same vestigial shape with a live owner installed beside it: the
# literal text of the old rule is satisfied, and the rule guarded nothing.
VESTIGIAL_WITH_OWNER_LIB = """\
use crate::transit_owner::{controlled_transit_disabled_probe, TransitLiveOwner};

pub fn pump(
    owner: &TransitLiveOwner<ChaCha8Rng>,
    outcome: &RouterI2npOutcome,
    inbound: &Ssu2InboundI2np,
) {
    let _ = owner.handle_inbound(inbound, 0);
    let _ = controlled_transit_disabled_probe(outcome);
}
"""

# The agreed shape plus a direct call to the live owner's inbound entry
# point: the owner is reached, but the composition no longer makes the
# whole owner/no-owner choice.
DIRECT_OWNER_DISPATCH_LIB = """\
use crate::transit_composition::{build_transit_owner, dispatch_transit_inbound};
use crate::transit_owner::TransitLiveOwner;

pub fn register_ssu2_service(config: &Config) -> Result<(), DaemonError> {
    let owner: Option<TransitLiveOwner<ChaCha8Rng>> = build_transit_owner(config)?;
    let _ = dispatch_transit_inbound(&owner, &outcome, &inbound, now_ms);
    if let Some(owner) = &owner {
        let _ = owner.handle_inbound(&inbound, now_ms);
    }
    Ok(())
}
"""

# The composition module with its no-owner fallback deleted.
COMPOSITION_WITHOUT_PROBE_COMP = """\
use crate::transit_owner::TransitLiveOwner;

pub(crate) fn build_transit_owner(
    config: &Config,
) -> Result<Option<TransitLiveOwner<ChaCha8Rng>>, TransitCompositionError> {
    Ok(None)
}

pub(crate) fn dispatch_transit_inbound(
    owner: &Option<TransitLiveOwner<ChaCha8Rng>>,
    outcome: &RouterI2npOutcome,
    inbound: &Ssu2InboundI2np,
    now_ms: u64,
) -> Result<(), TransitCompositionError> {
    match owner {
        Some(owner) => {
            owner.handle_inbound(inbound, now_ms)?;
        }
        None => {}
    }
    Ok(())
}
"""

# A composition module that dispatches but never reaches the live owner.
COMPOSITION_NEVER_REACHES_OWNER_COMP = """\
use crate::transit_owner::controlled_transit_disabled_probe;

pub(crate) fn dispatch_transit_inbound(
    outcome: &RouterI2npOutcome,
) -> Result<(), TransitCompositionError> {
    let _ = controlled_transit_disabled_probe(outcome);
    Ok(())
}
"""

# A production inbound path that names no owner at all.
NO_OWNER_LIB = """\
pub fn pump(outcome: &RouterI2npOutcome) {
    let _ = outcome;
}
"""

# The shape of the real `crates/i2pr-daemon/src/lib.rs`: a test-only
# module is declared near the top of the file, and the production code
# lives below it. A strip that latched on the first `#[cfg(test)]` would
# truncate this file and hide the production inbound path.
TEST_REGION_ABOVE_PRODUCTION_LIB = """\
#[cfg(test)]
mod app_runtime_qualification;

use crate::transit_composition::{build_transit_owner, dispatch_transit_inbound};
use crate::transit_owner::TransitLiveOwner;

pub fn register_ssu2_service(config: &Config) -> Result<(), DaemonError> {
    let owner: Option<TransitLiveOwner<ChaCha8Rng>> = build_transit_owner(config)?;
    let _ = dispatch_transit_inbound(&owner, &outcome, &inbound, now_ms);
    Ok(())
}
"""

# The agreed production shape, plus probe and live-owner calls inside a
# test region. A test-only call is not a production call and must not be
# reported.
TEST_ONLY_CALLS_LIB = """\
use crate::transit_composition::{build_transit_owner, dispatch_transit_inbound};
use crate::transit_owner::TransitLiveOwner;

pub fn register_ssu2_service(config: &Config) -> Result<(), DaemonError> {
    let owner: Option<TransitLiveOwner<ChaCha8Rng>> = build_transit_owner(config)?;
    let _ = dispatch_transit_inbound(&owner, &outcome, &inbound, now_ms);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::transit_owner::{controlled_transit_disabled_probe, TransitLiveOwner};

    #[test]
    fn probe_is_a_no_op() {
        let _ = controlled_transit_disabled_probe(&outcome);
    }

    #[test]
    fn owner_dispatches() {
        let _ = owner.handle_inbound(&inbound, 0);
    }
}
"""

# (label, lib.rs text, transit_composition.rs text, expected rule ids,
#  whether the legacy text search accepts the same lib.rs)
CONTROLS = [
    (
        "positive/agreed-composition-shape",
        AGREED_LIB,
        AGREED_COMP,
        set(),
        None,
    ),
    (
        "negative/vestigial-probe-only",
        VESTIGIAL_LIB,
        AGREED_COMP,
        {"lib-composition-dispatch", "lib-no-direct-disabled-probe", "lib-owner-construction"},
        True,
    ),
    (
        "negative/vestigial-probe-beside-live-owner",
        VESTIGIAL_WITH_OWNER_LIB,
        AGREED_COMP,
        {
            "lib-composition-dispatch",
            "lib-no-direct-disabled-probe",
            "lib-no-direct-owner-dispatch",
            "lib-owner-construction",
        },
        True,
    ),
    (
        "negative/direct-owner-dispatch-beside-composition",
        DIRECT_OWNER_DISPATCH_LIB,
        AGREED_COMP,
        {"lib-no-direct-owner-dispatch"},
        None,
    ),
    (
        "negative/composition-fallback-deleted",
        AGREED_LIB,
        COMPOSITION_WITHOUT_PROBE_COMP,
        {"composition-disabled-probe"},
        None,
    ),
    (
        "negative/no-owner-named-in-production",
        NO_OWNER_LIB,
        AGREED_COMP,
        {"lib-owner-module", "lib-composition-dispatch", "lib-owner-construction"},
        None,
    ),
    (
        "negative/composition-never-reaches-live-owner",
        AGREED_LIB,
        COMPOSITION_NEVER_REACHES_OWNER_COMP,
        {"composition-live-owner", "composition-builder"},
        None,
    ),
    (
        "positive/test-region-does-not-hide-production",
        TEST_REGION_ABOVE_PRODUCTION_LIB,
        AGREED_COMP,
        set(),
        None,
    ),
    (
        "positive/test-only-calls-are-not-production-calls",
        TEST_ONLY_CALLS_LIB,
        AGREED_COMP,
        set(),
        None,
    ),
]


def legacy_rule_accepted(lib_text):
    """Rule 10 exactly as it stood before Plan 502: two text searches."""
    return "transit_owner" in lib_text and PROBE in lib_text


def run_rule_controls():
    """Exercise the rule set against synthetic files on disk."""
    exercised = set()
    outcomes = []
    for label, lib_text, comp_text, expected, legacy_accepts in CONTROLS:
        tmp = tempfile.mkdtemp(prefix="i2pr-transit-composition.")
        try:
            lib_path = os.path.join(tmp, "lib.rs")
            comp_path = os.path.join(tmp, "transit_composition.rs")
            with open(lib_path, "w", encoding="utf-8") as handle:
                handle.write(lib_text)
            with open(comp_path, "w", encoding="utf-8") as handle:
                handle.write(comp_text)
            lib = Source(lib_path, read(lib_path))
            comp = Source(comp_path, read(comp_path))
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
        hits = evaluate(lib, comp)
        reported = {rule_id for rule_id, _ in hits}
        exercised |= reported
        outcomes.append((label, sorted(reported)))
        if reported != expected:
            fail(
                f"control {label}: expected rules {sorted(expected)}, "
                f"reported {sorted(reported)}"
            )
        if legacy_accepts is None:
            continue
        # The strictness proof: the rule this amendment replaces accepts
        # the vestigial shape, and the amended rule reports it.
        if not legacy_rule_accepted(lib_text):
            fail(
                f"control {label}: the legacy text search no longer accepts "
                "this shape; the strictness comparison is stale"
            )
        if not reported:
            fail(
                f"control {label}: the amended rule accepted the vestigial "
                "shape the legacy rule accepted"
            )
    for rule_id, _ in RULES:
        if rule_id not in exercised:
            fail(f"no control exercises the rule {rule_id}")
    return outcomes


def build_production_tree(repo_root, dest):
    """Write the scoped production tree that every pre-existing rule scans."""
    daemon_root = os.path.join(repo_root, "crates", "i2pr-daemon", "src")
    written = 0
    for dirpath, dirnames, filenames in os.walk(daemon_root):
        dirnames.sort()
        for name in sorted(filenames):
            if not name.endswith(".rs"):
                continue
            src = os.path.join(dirpath, name)
            out = os.path.join(dest, os.path.relpath(src, repo_root))
            os.makedirs(os.path.dirname(out), exist_ok=True)
            with open(out, "w", encoding="utf-8") as handle:
                handle.write(production_source(read(src)))
            written += 1
    return written


def check_production_tree(lib_path, comp_path):
    if not os.path.isfile(lib_path):
        fail(f"production daemon lib.rs is missing: {lib_path}")
        return
    if not os.path.isfile(comp_path):
        fail(
            "production transit composition module is missing: "
            f"{comp_path} -- Plan 502 installs the transit data-plane owner "
            "there, and rule 10 cannot be satisfied without it"
        )
        return
    lib = Source(lib_path, read(lib_path))
    comp = Source(comp_path, read(comp_path))
    for rule_id, message in evaluate(lib, comp):
        fail(f"rule 10 [{rule_id}]: {message}")


def report(outcomes, summary):
    for label, reported in outcomes:
        print(
            f"check-m11-transit-boundaries: control {label}: "
            + (f"reported {reported}" if reported else "clean")
        )
    print(summary)


if MODE == "build":
    repo_root, dest, script_path = sys.argv[2], sys.argv[3], sys.argv[4]
    check_no_drift(script_path)
    stripper_outcomes = run_stripper_controls()
    written = build_production_tree(repo_root, dest)
    if written == 0:
        fail(
            "the daemon source tree produced no production files; the "
            "production scans below would be vacuous"
        )
    if failures:
        for message in failures:
            print(f"check-m11-transit-boundaries: FAIL: {message}", file=sys.stderr)
        raise SystemExit(1)
    report(
        stripper_outcomes,
        "check-m11-transit-boundaries: production tree ok "
        f"({written} daemon source files, {len(PRODUCTION_RULES)} production "
        "rules mirrored, per-rule controls passed)",
    )
elif MODE == "scan":
    lib_path, comp_path, script_path = sys.argv[2], sys.argv[3], sys.argv[4]
    check_no_drift(script_path)
    rule_outcomes = run_rule_controls()
    check_production_tree(lib_path, comp_path)
    if failures:
        for message in failures:
            print(f"check-m11-transit-boundaries: FAIL: {message}", file=sys.stderr)
        raise SystemExit(1)
    report(
        rule_outcomes,
        "check-m11-transit-boundaries: transit composition rule ok "
        f"({len(RULES)} rules, {len(CONTROLS)} controls exercised)",
    )
elif MODE == "self-test":
    script_path = sys.argv[2]
    check_no_drift(script_path)
    stripper_outcomes = run_stripper_controls()
    rule_outcomes = run_rule_controls()
    if failures:
        for message in failures:
            print(f"check-m11-transit-boundaries: FAIL: {message}", file=sys.stderr)
        raise SystemExit(1)
    report(
        stripper_outcomes + rule_outcomes,
        "check-m11-transit-boundaries: self-test ok "
        f"({len(PRODUCTION_RULES)} production rules, {len(STRIPPER_CONTROLS)} "
        f"stripper controls, {len(RULES)} transit rules, "
        f"{len(CONTROLS)} transit controls exercised)",
    )
else:
    print(f"check-m11-transit-boundaries: unknown mode {MODE!r}", file=sys.stderr)
    raise SystemExit(2)
PY

# `--self-test` runs the controls alone: it reads no daemon source, so it
# is independent of whether the Plan 502 Rust has landed.
if [ "$transit_mode" = "self-test" ]; then
    python3 "$rule_python" self-test "${BASH_SOURCE[0]}" || exit 1
    echo "check-m11-transit-boundaries: passed"
    exit 0
fi

for owner in TransitHopRole TransitHopRegistration TransitRegistry TransitAdmissionState TransitAdmissionToken; do
    if rg -U -n "derive\([^)]*Clone[^)]*\)[[:space:]]*(pub[[:space:]]+)?(struct|enum)[[:space:]]+$owner" "$source_file"; then
        fail "$owner must not implement Clone"
    fi
done

# Plan 253: TransitHopMaterial in the daemon must be move-only; the
# static private key never crosses a Clone boundary.
if rg -U -n "derive\([^)]*Clone[^)]*\)[[:space:]]*(pub[[:space:]]+)?(struct|enum)[[:space:]]+TransitHopMaterial" "$daemon_transit"; then
    fail "TransitHopMaterial must not implement Clone"
fi
if rg -n "^impl[[:space:]]+Clone[[:space:]]+for[[:space:]]+TransitHopMaterial" "$daemon_transit"; then
    fail "TransitHopMaterial must not hand-implement Clone"
fi

remove_body="$(sed -n '/pub fn remove(/,/pub fn remove_into(/p' "$source_file")"
[[ -n "$remove_body" ]] || fail "TransitRegistry::remove not found"
if rg -n 'expect\(|unwrap\(|panic!|unreachable!' <<<"$remove_body"; then
    fail "TransitRegistry::remove must return typed unknown-id errors"
fi
if rg -n 'tokio::|std::net|std::fs|async[[:space:]]+fn|JoinHandle|spawn\(' "$source_file"; then
    fail "runtime ownership leaked into runtime-neutral transit module"
fi
if rg -n 'std::sync|Mutex|RwLock|AtomicU|AtomicI' "$source_file"; then
    fail "M11 foundation must leave synchronization ownership to the runtime plan"
fi

# Plan 252: production daemon code must use the message-level
# transit API (`process_short_build_message`) rather than calling
# the per-record Plan 250 transaction and then independently
# re-running `MessageHopProcessor` or `chacha20_transform` to
# apply the canonical per-slot ChaCha transforms.
# The Plan 250 per-record API is retained for focused tests and
# historical consumers; it must not appear in the production
# daemon code under `crates/i2pr-daemon/src/`.
daemon_root="$root/crates/i2pr-daemon/src"
# Production boundary checks run against non-test code only: inline
# `#[cfg(test)]` modules may seal synthetic fixtures to construct
# inputs, but production paths must treat transformed payloads as
# opaque.
#
# The strip is SCOPED: every `#[cfg(test)]` region is removed, and only
# that region -- the production code around it stays. The blunt
# convention this script used before ("drop everything from the first
# `#[cfg(test)]` to end-of-file") latched on the first annotation, and
# the daemon's `lib.rs` declares a test-only module at line 14 and
# `service_product.rs` at line 137: the blunt form truncated the
# daemon's production composition to 13 of its 5770 lines and hid the
# production inbound path from every rule below. The scoped form is
# Plan 366's fix, applied here.
python3 "$rule_python" build "$root" "$production_src" "${BASH_SOURCE[0]}" || exit 1
if rg -n 'process_short_build_request\b' "$production_src"; then
    fail "production daemon code must not call process_short_build_request; use process_short_build_message"
fi
# The canonical per-slot ChaCha transform primitive is owned by
# i2pr-tunnel's multirecord module. Production daemon code must
# never reach in to invoke it directly alongside a Plan 250
# per-record transaction.
if rg -n 'chacha20_transform\(' "$production_src"; then
    fail "production daemon code must not call chacha20_transform directly; compose via process_short_build_message"
fi
# The Plan 250 per-record post-`process_short_build_request` reply
# envelope must not be resealed by the daemon; the daemon must
# treat the transformed build payload as opaque.
if rg -n 'seal_short_reply\(|open_short_reply\(|seal_short_request\(|open_short_request\(' "$production_src"; then
    fail "production daemon code must not invoke build-cryptography primitives; secrets stay in i2pr-tunnel"
fi

# Plan 253 corrective boundaries. Each new check must remain
# small and semantic; do not turn this file into a source-text
# grep harness.
#
# 1. The Plan 252 placeholder `forward_participant_layer` is
#    removed; production daemon code must never introduce a
#    no-op TunnelData transform helper.
if rg -n -v '// Plan 253 removes' "$production_src" | rg -n 'forward_participant_layer\b'; then
    fail "production daemon code must not call forward_participant_layer; compose via TransitHopRegistration::process_tunnel_data"
fi
# 2. Production daemon code must use the runtime-neutral
#    `TransitHopRegistration::process_tunnel_data` API rather than
#    re-implementing the canonical participant / OBEP / IBGW
#    transforms locally. Comment lines beginning with `//` are
#    excluded because the production daemon may mention the
#    canonical helper in documentation only. This pass runs on the
#    scoped production tree, so a `#[cfg(test)]` region can neither
#    hide production code nor contribute a mention.
while IFS= read -r src; do
    rel="${src#$production_src/}"
    dest="$comment_stripped/$rel"
    mkdir -p "$(dirname "$dest")"
    # Strip comment lines; do not strip string literals — the
    # production code never references the helper in string
    # literals because it is a typed constant.
    grep -v -E '^[[:space:]]*//' "$src" > "$dest"
done < <(find "$production_src" -name '*.rs')
if rg -n 'TunnelLayerTransform::participant_forward' "$comment_stripped"; then
    fail "production daemon code must not invoke TunnelLayerTransform directly; the runtime-neutral data plane owns the transform"
fi
# 3. `TransitBuildService` must have a production caller outside
#    its own test module once the daemon composition is enabled.
#    The presence of `TransitOwner::dispatch_short_build` and the
#    lib-public `transit_owner` module satisfy the rule.
if [[ ! -f "$daemon_transit_owner" ]]; then
    fail "transit_owner module must exist to provide a live caller for TransitBuildService"
fi
if ! rg -q 'gate\.dispatch_short_build\b' "$daemon_transit_owner"; then
    fail "transit_owner must wire the controlled gate via TransitIngressGate::dispatch_short_build"
fi
# 4. Rejection delivery must not discard the route metadata —
#    every code-30 path must surface the role-correct
#    `TransitDispatchRole` shape.
if ! rg -q 'enum TransitDispatchRole' "$daemon_transit"; then
    fail "daemon transit_compose must expose TransitDispatchRole for code-30 route metadata"
fi
# 5. The daemon-side build dispatcher must wrap the
#    transformed body in a complete I2NP envelope (STBM type
#    0x19 or OTBRM type 0x1A) before handing bytes to
#    `RouterDeliveryService`; a bare STBM/OTBRM body is
#    never sent.
if ! rg -q 'wrap_short_tunnel_build_envelope' "$daemon_transit"; then
    fail "daemon transit_compose must wrap ShortTunnelBuild bodies via wrap_short_tunnel_build_envelope"
fi
if ! rg -q 'wrap_outbound_tunnel_build_reply_envelope' "$daemon_transit"; then
    fail "daemon transit_compose must wrap OutboundTunnelBuildReply bodies via wrap_outbound_tunnel_build_reply_envelope"
fi
# 6. The peer/session index is bounded; the production daemon
#    must use `TransitPeerIndex` rather than `BTreeMap<Hash, PeerId>`.
if rg -n 'peer_index: BTreeMap' "$production_src"; then
    fail "production daemon code must use TransitPeerIndex, not BTreeMap<Hash, PeerId>"
fi
# 7. The peer/session index must declare the
#    `MAX_TRANSIT_PEER_INDEX` ceiling; the daemon must not
#    silently accept an unlimited container.
if ! rg -q 'MAX_TRANSIT_PEER_INDEX' "$daemon_transit"; then
    fail "daemon transit_compose must reference MAX_TRANSIT_PEER_INDEX"
fi
# 8. `TransitBuildService::cancel` must drain the registry
#    synchronously rather than wait for the 600-second
#    expiration timer.
if rg -n 'fn cancel' "$daemon_transit" | rg -q .; then
    if ! rg -n 'registry.expire\(u64::MAX\)' "$daemon_transit"; then
        fail "TransitBuildService::cancel must drain registry synchronously via registry.expire(u64::MAX)"
    fi
fi

# Plan 254 corrective boundaries.
#
# 9. The Plan 253 empty-body shim is removed completely. Neither the
#    original `plan253_short_build_payload` helper nor a renamed
#    equivalent returning a hard-coded empty transit build body may
#    remain in daemon transit code.
if rg -n 'plan253_short_build_payload' "$daemon_root"; then
    fail "plan253_short_build_payload must be removed; thread the canonical decoded body instead"
fi
if rg -n 'fn .*short_build_payload' "$daemon_root"; then
    fail "no short_build_payload helper may remain in daemon code; use the canonical transit body handoff"
fi
# 11. The production short-build dispatch path must use the outer
#     owner's real cancellation token, never a fresh token created
#     inside the dispatch.
if rg -n 'CancellationToken::new\(\)' "$daemon_transit_owner"; then
    fail "transit_owner must not create a fresh CancellationToken; use the outer owner token"
fi
# 12. No second I2NP decode may live in `transit_owner.rs`. The
#     canonical `router_i2np` dispatcher is the single decoder; the
#     live owner consumes `dispatch_router_i2np_with_transit_bodies`
#     only.
if rg -n 'decode_standard|decode_short_transport|I2npMessage::decode' "$daemon_transit_owner"; then
    fail "transit_owner must not decode I2NP envelopes; use the canonical router_i2np handoff"
fi
# 13. Daemon transit code must never hold raw `LayerKeys` or reach
#     the canonical `TunnelLayerTransform` directly; the
#     runtime-neutral data plane owns the transform.
if rg -n 'LayerKeys' "$daemon_transit_owner"; then
    fail "transit_owner must not name LayerKeys; secrets stay in i2pr-tunnel"
fi
if rg -n 'TunnelLayerTransform' "$daemon_transit_owner"; then
    fail "transit_owner must not invoke TunnelLayerTransform directly"
fi
# 14. The live owner must expose the controlled enablement, the
#     creator/service ownership probes, the outer-cancellation
#     drain, and the session-close peer reconciliation the Plan 254
#     acceptance criteria require.
for symbol in 'TransitLiveOwner' 'install_creator_build' 'install_creator_data' 'install_creator_gateway' 'note_session_closed' 'dispatch_router_i2np_with_transit_bodies' 'TransitInboundBodies'; do
    if ! rg -q "$symbol" "$daemon_transit_owner" && ! rg -q "$symbol" "$root/crates/i2pr-daemon/src/router_i2np.rs"; then
        fail "expected Plan 254 live-ingress symbol $symbol in transit_owner/router_i2np"
    fi
done

# Plan 255 qualification boundaries (historical; the Plan 256
# corrective supersedes the driver/runner/checker expectations in
# rules 20-24 below, but the production-code invariants stay).
#
# 15. The external qualification driver must exist and must be
#     `#[ignore]`-gated with its plan explanation.
external_driver="$root/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"
if [[ ! -f "$external_driver" ]]; then
    fail "Plan 255 external driver missing: $external_driver"
fi
if ! rg -qF '#[ignore = "Plan 25' "$external_driver"; then
    fail "external driver must be #[ignore]-gated with its plan explanation"
fi
# 16. The external driver must consume Ssu2InboundI2np through
#     `TransitLiveOwner::handle_inbound` rather than constructing
#     STBMs by hand after runtime startup.
for symbol in 'Ssu2InboundI2np' 'next_inbound' 'TransitLiveOwner' 'handle_inbound' 'dispatch_router_i2np_with_transit_bodies'; do
    if ! rg -q "$symbol" "$external_driver"; then
        fail "external driver missing Plan 255 symbol $symbol"
    fi
done
# 17. The external driver must not reintroduce the Plan 253 empty-body
#     shim or hand-built STBM helper in production daemon code.
#     (The driver file may name the historical symbol inside an
#     assertion that proves the production invariant; production
#     daemon source is the actual surface to guard.)
if rg -q 'plan253_short_build_payload' "$daemon_root"; then
    fail "daemon transit code must not reintroduce the empty-body shim"
fi
if rg -q 'fn short_build_payload' "$daemon_root"; then
    fail "daemon transit code must not define a hand-built STBM helper"
fi
# 18. The fail-closed runner must exist, must verify the exact i2pd
#     pin + version, must stay loopback-only, and must disable public
#     reseed.
external_runner="$root/tests/integration/m11-transit/run-i2pd.sh"
external_checker="$root/scripts/check-m11-transit-qualification-evidence.sh"
for path in "$external_runner" "$external_checker"; do
    if [[ ! -f "$path" ]]; then
        fail "Plan 255 surface missing: $path"
    fi
done
if ! rg -qF '635b013a612ff47278ef02acf8580a28e10e26c5' "$external_runner"; then
    fail "external runner lost the exact i2pd pin"
fi
if ! rg -qF '2.61.0' "$external_runner"; then
    fail "external runner lost the exact i2pd version"
fi
if ! rg -qF '127.0.0.1' "$external_runner"; then
    fail "external runner lost the loopback bind policy"
fi
# 19. The static evidence checker must exist and require every
#     mandatory Plan 255 row.
if ! rg -qF 'm11-i2pd-live-next-inbound-observed' "$external_checker"; then
    fail "evidence checker missing Plan 255 row labels"
fi

# Plan 256 qualification evidence/topology corrective boundaries.
#
# 20. The external driver is `#[ignore]`-gated with the Plan 256
#     explanation and requires the explicit single-owner
#     environment (binary, pin, both reference datadirs,
#     loopback bind, evidence dir). The Plan 255
#     `I2PD_ROUTER_INFO` / `I2PD_SSU2_ENDPOINT` split inputs are
#     gone: identity and references are owned by one process.
if ! rg -qF '#[ignore = "Plan 256' "$external_driver"; then
    fail "external driver must be #[ignore]-gated with the Plan 256 explanation"
fi
for env_name in 'I2PD_BIN' 'I2PD_A_DATADIR' 'I2PD_B_DATADIR' 'I2PD_PIN' 'I2PD_VERSION' 'I2PR_SSU2_BIND' 'EVIDENCE_DIR'; do
    if ! rg -qF "$env_name" "$external_driver"; then
        fail "external driver must require env $env_name"
    fi
done
# 21. The driver must bind role rows to typed decoded roles through
#     an epoch-qualified evidence recorder, and must own both
#     reference lifecycles.
for symbol in 'TransitHopRoleKind' 'record_row' 'enum Epoch' 'ReferenceProcess' 'I2PD_B_DATADIR' 'routerInfo-'; do
    if ! rg -qF "$symbol" "$external_driver"; then
        fail "external driver missing Plan 256 symbol $symbol"
    fi
done
# 22. The Plan 255 defects must stay fixed: no generic build fan-out
#     boolean, no independent transit responder key, no
#     evidence-directory-derived NetDB fallback. Comment and
#     self-check mentions are excluded; only code counts.
driver_code_nocomments="$(mktemp)"
grep -vE '^[[:space:]]*(//|//!|///)' "$external_driver" > "$driver_code_nocomments"
if grep -qE 'let[[:space:]]+(mut[[:space:]]+)?observed_build|observed_build[[:space:]]*=|if[[:space:]]+observed_build' "$driver_code_nocomments"; then
    fail "external driver must not reintroduce the observed_build fan-out"
fi
rm -f "$driver_code_nocomments"
if rg -v 'contains\(' "$external_driver" | rg -q 'X25519PrivateKey::generate\('; then
    fail "external driver must not generate an independent transit responder key"
fi
if rg -qF '../i2pd-a/data/netDb' "$external_driver"; then
    fail "external driver must not derive a NetDB path from the evidence directory"
fi
# 23. The runner must provision explicit fresh datadirs for both
#     references, drive the corrected lane once per invocation, and
#     keep the exact pin/version/loopback/reseed gates.
for token in 'I2PD_A_DATADIR' 'I2PD_B_DATADIR' 'I2PD_A_SAM_PORT' 'I2PD_B_SAM_PORT' 'DRIVER_TIMEOUT' 'm11_row' 'exact_row'; do
    if ! rg -qF "$token" "$external_runner"; then
        fail "external runner missing Plan 256 token $token"
    fi
done
# 24. The evidence checker must carry the Plan 256 corrective
#     invariants (anti-fan-out, typed roles, i2pd-B, exact owner,
#     key coherence, epoch keys).
for token in 'observed_build' 'TransitHopRoleKind' 'I2PD_B_DATADIR' 'routerInfo-' 'X25519PrivateKey::generate' 'EPOCH_KEYS'; do
    if ! rg -qF "$token" "$external_checker"; then
        fail "evidence checker missing Plan 256 invariant $token"
    fi
done

# Plan 257 production self-reply and evidence-completion boundaries.
#
# 25. The non-secret snapshot, the self-reply arg bundle, and the
#     typed bandwidth summary must exist with the exact Plan 257
#     shapes; the daemon must surface bandwidth from the
#     runtime-neutral transaction (never re-decode the Mapping).
for symbol in 'TransitLiveStateSnapshot' 'SelfReplyOtbrmArgs' 'TransitBandwidthSummary'; do
    if ! rg -q "$symbol" "$daemon_transit"; then
        fail "daemon transit_compose missing Plan 257 symbol $symbol"
    fi
done
if ! rg -q 'TransitBandwidthSummary' "$source_file"; then
    fail "i2pr-tunnel transit must expose TransitBandwidthSummary"
fi
if ! rg -q 'live_state_snapshot' "$daemon_transit"; then
    fail "TransitBuildService must expose live_state_snapshot"
fi
if ! rg -q 'live_state_snapshot' "$daemon_transit_owner"; then
    fail "TransitLiveOwner must expose live_state_snapshot"
fi
if ! rg -q 'has_peer' "$daemon_transit_owner"; then
    fail "TransitLiveOwner must expose has_peer for session-close B-retained proof"
fi
# 26. The daemon must not re-decode build-option bandwidth: no
#     Mapping/get("m") lookup in daemon transit code.
if rg -n 'mapping\.get\(|mapping\(\).get\(|get\("m"\)|get\("r"\)|get\("l"\)|get\("b"\)' "$production_src"; then
    fail "daemon transit code must not re-decode bandwidth options; copy TransitBandwidthSummary from the transaction"
fi
# 27. The self-reply seam must take the bundled args (Plan 257
#     work package I clippy-ceiling fix without suppression).
if ! rg -q 'SelfReplyOtbrmArgs' "$daemon_transit_owner"; then
    fail "transit_owner must route self replies through SelfReplyOtbrmArgs"
fi
# 28. The evidence checker must carry the Plan 257 invariants
#     (reply source-locks, far-side, full-drain, typed bandwidth,
#     cardinality, two-attempt gate).
for token in 'count_endpoint_messages' 'far_side_satisfied' 'cancel_fully_drained' 'record_cardinality' 'record_bandwidth_rows' 'SelfReplyOtbrmArgs' 'I2PR_M11_ATTEMPT'; do
    if ! rg -qF "$token" "$external_checker"; then
        fail "evidence checker missing Plan 257 invariant $token"
    fi
done
# 29. The runner must source-lock both i2pd reply branches and
#     emit the Plan 265 manifest-v5 shape (plan 265 + scenario +
#     attempt + attempt budget + qualification SHA + production
#     baseline + opportunity + semantic + external completion +
#     terminal class, plus the retained per-epoch ids).
for token in 'm11-i2pd-obep-remote-reply-source-lock' 'm11-i2pd-obep-local-ibgw-reply-source-lock' '"plan": 265' 'i2pr-m11-transit-qualification-v5' 'I2PR_M11_ATTEMPT' 'I2PR_M11_ONLY_EPOCH' 'I2PR_M11_EPOCH_PASS' 'I2PR_M11_SCENARIO' 'I2PR_M11_DRIVER_RC' 'EPOCH_TERMINAL_KEY' 'exact_lib_row' '"lifecycle_rows": lifecycle_rows' '"lifecycle_rows_declared": lifecycle_rows_declared' '"lane_row_scope"'; do
    if ! rg -qF "$token" "$external_runner"; then
        fail "external runner missing Plan 265 token $token"
    fi
done
for stale in '"plan": 263' '"plan": 262' '"plan": 264'; do
    if rg -qF "$stale" "$external_runner"; then
        fail "external runner must not name stale plan $stale (closure authority is Plan 265)"
    fi
done
# 31. Plan 261 work package A: the runner must source-lock the
#     B-side sender behaviors and provision the B SAM port; the
#     manifest must not name the superseded A-sender plan 260.
for token in 'm11-i2pd-b-sam-bridge-enabled-source-lock' 'm11-i2pd-explicit-peer-outbound-selection-source-lock' 'm11-i2pd-outbound-endpoint-tunnel-forward-source-lock' 'm11-i2pd-b-leaseset-resolution-source-lock' 'I2PD_B_SAM_PORT'; do
    if ! rg -qF "$token" "$external_runner"; then
        fail "external runner missing Plan 261 token $token"
    fi
done
if rg -qF '"plan": 260' "$external_runner"; then
    fail "external runner must not name stale plan 260 (B-sender authority is Plan 261)"
fi
if rg -qF '"plan": 261' "$external_runner"; then
    fail "external runner must not name stale plan 261 (B3 self-delivery boundary corrected by Plan 262)"
fi
# 32. Plan 262 work package B: the dedicated IBGW state must not
#     inherit participant previous-peer locking. The type alias is
#     gone; the struct owns only the fragment sequence.
if rg -q 'pub type TransitGatewayData' "$source_file"; then
    fail "TransitGatewayData must be a dedicated struct, not a type alias for TransitParticipantData"
fi
if ! rg -q 'pub struct TransitGatewayData' "$source_file"; then
    fail "i2pr-tunnel transit must own dedicated TransitGatewayData"
fi
gateway_struct="$(sed -n '/pub struct TransitGatewayData/,/^}/p' "$source_file")"
if printf '%s\n' "$gateway_struct" | rg -q 'locked_previous_peer'; then
    fail "TransitGatewayData must not contain locked_previous_peer"
fi
if printf '%s\n' "$gateway_struct" | rg -q 'duplicates'; then
    fail "TransitGatewayData must not inherit the participant replay window"
fi
# 33. Plan 262 work package C: the runtime-neutral IBGW gateway API
#     must not accept or check a previous-peer argument; exact
#     receive-id equality is required.
gateway_fn="$(awk '/pub fn process_tunnel_gateway/{on=1} on{print; o+=gsub(/{/,"{"); c+=gsub(/}/,"}"); if(o>0 && o==c){exit}}' "$source_file" | grep -vE '^[[:space:]]*(//|//!|///)' || true)"
if printf '%s\n' "$gateway_fn" | rg -q 'previous_peer'; then
    fail "process_tunnel_gateway must not accept or check previous_peer"
fi
if ! printf '%s\n' "$gateway_fn" | rg -q 'expected_receive'; then
    fail "process_tunnel_gateway must bind the registry key for exact receive-id equality"
fi
# 34. Plan 262 work package D: one canonical source-neutral IBGW
#     seam for network and local ingress; the OBEP TUNNEL-to-self
#     branch must not synthesize peer state.
if ! rg -q 'pub fn route_ibgw_gateway' "$daemon_transit"; then
    fail "daemon transit_compose must own source-neutral route_ibgw_gateway"
fi
if ! rg -q 'deliver_obep_tunnel_to_self' "$daemon_transit_owner"; then
    fail "transit_owner must own the OBEP TUNNEL-to-self local branch"
fi
if ! rg -q 'LocalIbgwDelivered' "$daemon_transit_owner"; then
    fail "transit_owner must expose LocalIbgwDelivered"
fi
if ! rg -q 'LocalIbgwDropped' "$daemon_transit_owner"; then
    fail "transit_owner must expose LocalIbgwDropped"
fi
self_fn="$(awk '/fn deliver_obep_tunnel_to_self/{on=1} on{print; o+=gsub(/{/,"{"); c+=gsub(/}/,"}"); if(o>0 && o==c){exit}}' "$daemon_transit_owner" | grep -vE '^[[:space:]]*(//|//!|///)' || true)"
if printf '%s\n' "$self_fn" | rg -q 'PeerId::from_hash|PeerId::from_bytes'; then
    fail "self-loop must not synthesize a PeerId"
fi
if printf '%s\n' "$self_fn" | rg -q 'install_peer'; then
    fail "self-loop must never insert the local router into the peer index"
fi
# 35. Plan 262 work package A: the runner must source-lock the
#     self-loopback and receive-id gateway dispatch (retained).
for token in 'm11-i2pd-self-loopback-source-lock' 'm11-i2pd-tunnel-gateway-by-receive-id-source-lock' 'm11-i2pd-tunnel-gateway-no-creator-peer-affinity-source-lock'; do
    if ! rg -qF "$token" "$external_runner"; then
        fail "external runner missing Plan 262 token $token"
    fi
done
# 36. Plan 263 work package A: the driver must prove mesh
#     sustainability before counted sends without tuning any
#     timeout/quota/ceiling/retry/message-size constant.
#     Session freshness (explicit SSU2 liveness after the
#     best-effort redial), relay robustness (NetDB placements +
#     B floodfill role), and SAM discipline (canonical fatal
#     read-timeout tail) are all locked here.
for symbol in 'mesh_liveness_status' 'verify_relay_netdb_prerequisites' 'verify_b_floodfill_conf' 'SAM_READ_TIMEOUT_MSG' 'plan263_sam_read_timeout_tail_is_canonical_and_fatal' 'plan263_mesh_liveness_error_names_missing_links' 'plan263_relay_netdb_prerequisites_require_all_placements' 'plan263_b_floodfill_conf_requires_floodfill_role'; do
    if ! rg -qF "$symbol" "$external_driver"; then
        fail "external driver missing Plan 263 sustainability symbol $symbol"
    fi
done
# No timeout/quota/ceiling/retry/message-size inflation: the
# Plan 262 numeric lane constants must keep their exact values.
for binding in 'DIAL_TIMEOUT: Duration = Duration::from_secs(20)' 'SAM_IO_TIMEOUT: Duration = Duration::from_secs(15)' 'SETUP_HEARTBEAT_SECS: u64 = 30' 'MAX_SAM_DATAGRAM_RX_BYTES: usize = 8192' 'MAX_LEDGER_OBSERVATIONS: usize = 4096'; do
    if ! rg -qF "$binding" "$external_driver"; then
        fail "external driver changed a frozen Plan 262 lane constant: $binding"
    fi
done
# 30. The external workflow must run the two-attempt matrix on one
#     SHA with fail-fast disabled and per-attempt artifacts, plus
#     the Plan 264 per-epoch inputs (epoch selector + pass id).
#     Full-matrix runs without an epoch are diagnostic-only.
external_workflow="$root/.github/workflows/m11-transit-external.yml"
if [[ ! -f "$external_workflow" ]]; then
    fail "M11 external workflow missing: $external_workflow"
fi
for token in 'attempt: [1, 2, 3, 4, 5, 6, 7, 8]' 'fail-fast: false' 'I2PR_M11_ATTEMPT' 'I2PR_M11_SCENARIO' 'm11-transit-evidence-' 'I2PR_M11_ONLY_EPOCH' 'I2PR_M11_EPOCH_PASS'; do
    if ! rg -qF "$token" "$external_workflow"; then
        fail "external workflow missing Plan 265 token $token"
    fi
done
# 37. Plan 264 work package A: the driver must own the per-epoch
#     fresh-mesh lane (fine-grained epoch gates, setup
#     prerequisites, lifecycle chain) plus the composition-gate
#     predicates and regressions, without tuning any lane
#     constant (frozen values locked below alongside rule 36).
for symbol in 'PLAN264_MANDATORY_EPOCHS' 'plan264_epoch_is_mandatory' 'plan264_epoch_passes_satisfy' 'plan264_composition_covers_all_epochs' 'plan264_single_mesh_is_diagnostic_only' 'needs_participant_setup' 'needs_forward_setup' 'run_lifecycle_chain' 'run_obep_data' 'run_ibgw_data_epoch' 'run_receipt_epoch' 'run_participant_data_epoch' 'plan264_single_pass_cannot_close_epoch' 'plan264_mixed_sha_epochs_rejected' 'plan264_cross_epoch_merge_rejected' 'plan264_missing_epoch_rejected' 'plan264_single_mesh_run_is_diagnostic_only'; do
    if ! rg -qF "$symbol" "$external_driver"; then
        fail "external driver missing Plan 264 per-epoch symbol $symbol"
    fi
done
# The per-epoch composition checker must exist and be wired.
if [[ ! -f "$root/scripts/check-m11-per-epoch-composition.sh" ]]; then
    fail "Plan 264 per-epoch composition checker missing: scripts/check-m11-per-epoch-composition.sh"
fi

# 38. Plan 265 work packages A-D: the fixed-budget
#     opportunity-qualified qualification surface. The driver owns the
#     closed terminal vocabulary and the three input-side opportunity
#     predicates; the runner owns the four Plan 265 source-lock rows;
#     the composer owns the fixed-budget closure gate and its negative
#     fixtures. Production crates/*/src must stay untouched (rule 36's
#     frozen numerics and this plan's diff guard both hold).
for symbol in 'PLAN265_SCENARIO_FAMILIES' 'PLAN265_ATTEMPT_BUDGET: usize = 8' 'PLAN265_PRODUCTION_BASELINE' 'PLAN265_TERMINAL_VOCABULARY' 'PLAN265_LIFECYCLE_ROWS' 'registration_live_at_input' 'plan265_ibgw_opportunity' 'plan265_ibgw_semantic_pass' 'plan265_classify_ibgw' 'plan265_receipt_opportunity' 'plan265_receipt_semantic_pass' 'plan265_classify_receipt' 'plan265_participant_opportunity' 'plan265_classify_participant' 'ReplayOutcomeKind' 'PLAN266_RECEIPT_CREATOR_LOST_ID' 'plan266_receipt_ladder_rung' 'plan266-ladder' 'DropDisposition' 'remember_drop_disposition' 'plan267_disposition_label' 'plan267_accepted_at_ms' 'plan267-drop-disposition' 'plan267-addressed-id' 'AcceptancePath' 'remember_acceptance_path' 'PLAN268_PATH_VALUES' 'plan268-acceptance-path'; do
    if ! rg -qF "$symbol" "$external_driver"; then
        fail "external driver missing Plan 265/266/267/268 symbol $symbol"
    fi
done
for row in 'm11-i2pd-plan265-production-source-lock' 'm11-i2pd-plan265-frozen-attempt-budget' 'm11-i2pd-plan265-closed-terminal-vocabulary' 'm11-i2pd-plan265-opportunity-is-input-side'; do
    if ! rg -qF "$row" "$external_runner"; then
        fail "external runner missing Plan 265 row $row"
    fi
done
for token in 'plan265_ibgw_large_input_single_cell_is_semantic_failure' 'plan265_receipt_reference_completion_miss_is_not_a_semantic_failure' 'plan265_replay_duplicate_forwarded_fails_regardless_of_b_receipt' 'plan265_manifest_v5_requires_exactly_eight_ordinals' 'plan265_lifecycle_rows_cannot_be_borrowed_across_attempts' 'plan265_production_source_diff_guard' 'plan266_receipt_ladder_names_each_unsatisfied_rung' 'plan266_receipt_creator_lost_id_is_typed_no_opportunity' 'plan266_receipt_counted_live_action_keeps_opportunity_without_advertisement' 'plan266_receipt_unobservable_advertisement_falls_back_to_plan265' 'plan267_disposition_labels_are_closed_and_never_terminals' 'plan267_disposition_store_is_epoch_scoped' 'plan267_accepted_at_ms_reads_gated_acceptance_only' 'plan268_path_values_are_closed_and_never_terminals' 'plan268_acceptance_path_store_is_epoch_scoped'; do
    if ! rg -qF "$token" "$external_driver"; then
        fail "external driver missing Plan 265/266/267/268 focused test $token"
    fi
done
# The composer must carry the Plan 265 fixed-budget contract and its
# negative fixtures, and the retained Plan 264 evidence index.
if ! rg -qF 'compose-265' "$root/scripts/check-m11-per-epoch-composition.sh"; then
    fail "Plan 265 fixed-budget composition mode missing from check-m11-per-epoch-composition.sh"
fi
for token in 'check_input_side' 'check-input-side' 'PYOPPORTUNITY'; do
    if ! rg -qF "$token" "$root/scripts/check-m11-per-epoch-composition.sh"; then
        fail "Plan 265 shared input-side opportunity guard missing $token"
    fi
done
# The external runner must consume that same shared guard, never a
# second, drifting copy of it.
if [[ "$(rg -cF 'check-m11-per-epoch-composition.sh" --check-input-side' "$external_runner" || true)" != "1" ]]; then
    fail "external runner must call the shared input-side opportunity guard exactly once"
fi
if rg -qF 'PYOPPORTUNITY' "$external_runner"; then
    fail "external runner must not carry its own copy of the input-side opportunity guard"
fi
for fixture in 'two-successes-plus-six-retained-misses-accepted' 'seven-successes-plus-one-semantic-failure-rejected' 'two-successes-then-missing-ordinals-rejected' 'duplicate-ordinal-rejected' 'mixed-qualification-sha-rejected' 'changed-attempt-budget-rejected' 'unclassified-terminal-rejected' 'opportunity-inferred-from-output-rejected' 'static-opportunity-inferred-from-output-rejected' 'static-ladder-inferred-from-output-rejected' 'partial-lifecycle-rows-borrowed-rejected' 'replay-duplicate-forwarded-rejected' 'plan264-manifest-promotion-rejected' 'missing-retained-manifest-rejected' 'receipt-only-two-successes-accepted' 'receipt-only-refuses-fresh-ibgw-set' 'receipt-only-refuses-fresh-participant-set' 'receipt-only-miss-labeled-pass-rejected' 'receipt-only-rung5-as-opportunity-rejected' 'receipt-only-nine-attempts-rejected' 'receipt-only-mixed-sha-rejected' 'receipt-only-disposition-as-terminal-rejected' 'receipt-only-path-as-terminal-rejected'; do
    if ! rg -qF "$fixture" "$root/scripts/check-m11-per-epoch-composition.sh"; then
        fail "Plan 265/266/267/268 composition fixture missing: $fixture"
    fi
done
if [[ ! -f "$root/plans/closure/transit-tunnels/265-retained-plan264-evidence.tsv" ]]; then
    fail "retained Plan 264 evidence index missing: plans/closure/transit-tunnels/265-retained-plan264-evidence.tsv"
fi

# 39. Plan 340 correction: the Plan 265 zero-production-diff guard must
#     actually be able to fail. Its pathspec was `crates/*/src`, which
#     git's default matching resolves against the whole path and so
#     selected nothing; the guard then reported "Plan 262 production
#     authority retained" regardless of how far production drifted.
#     Two independent teeth:
#       a. positive control — the corrected pathspec must select crate
#          production sources in this very tree;
#       b. the vacuous form must select nothing, so the difference is
#          proven here rather than assumed;
#       c. the runner must use the corrected form at both call sites
#          (the shell guard and the manifest/composition gate).
if ! command -v git >/dev/null 2>&1; then
    fail "git is required to prove the production-source guard pathspec is not vacuous"
fi
prod_spec_corrected=':(glob)crates/*/src/**'
prod_spec_vacuous='crates/*/src'
prod_src_selected="$(git -C "$root" ls-files -- "$prod_spec_corrected" | wc -l | tr -d ' ')"
vacuous_selected="$(git -C "$root" ls-files -- "$prod_spec_vacuous" | wc -l | tr -d ' ')"
if [[ -z "$prod_src_selected" || "$prod_src_selected" -eq 0 ]]; then
    fail "corrected production-source pathspec ${prod_spec_corrected} selected no files; the guard cannot detect drift"
fi
if [[ "$vacuous_selected" -ne 0 ]]; then
    fail "expected the bare ${prod_spec_vacuous} pathspec to match nothing, but it selected ${vacuous_selected} paths; re-derive the Plan 340 correction"
fi
# The runner quotes the spec in shell single quotes at one call site and
# in Python double quotes at the other, and the explanatory comments name
# both forms. Count only non-comment lines so a prose mention can never
# satisfy this check. The corrected spec *contains* the vacuous one as a
# substring, so the vacuous form is matched as a complete git argument
# (end of token or closing quote) rather than as free text.
runner_code="$(mktemp)"
grep -v -E '^[[:space:]]*#' "$external_runner" > "$runner_code"
corrected_in_code="$(grep -cF -- "$prod_spec_corrected" "$runner_code" || true)"
vacuous_in_code="$(grep -cE "crates/\\*/src(['\"]|\$|[[:space:]])" "$runner_code" || true)"
if [[ "$vacuous_in_code" -ne 0 ]]; then
    fail "external runner code still passes the vacuous ${prod_spec_vacuous} pathspec to git (${vacuous_in_code} site(s)); the Plan 265 guard fails open"
fi
if [[ "$corrected_in_code" -lt 2 ]]; then
    fail "external runner must pass the corrected production-source pathspec at both call sites (found ${corrected_in_code} in code, expected at least 2)"
fi

# Plan 502 (WP6): rule 10 -- the transit data-plane owner must be in the
# product composition, and this rule must guard that fact *structurally*.
#
# The rule this replaces was two text searches:
#
#   rg -q 'transit_owner'                    lib.rs
#   rg -q 'controlled_transit_disabled_probe' lib.rs
#
# Both were satisfiable without an installed owner. The second matched a
# call to `controlled_transit_disabled_probe`, which is
# `TransitOwner::is_transit_managed(outcome)` -- a `matches!` with no
# dispatch -- whose result the caller discarded. A composition that
# installed the live owner *and kept the probe call beside it* satisfied
# the literal text while the rule guarded nothing (Plan 502 §3.4).
# **A text search that can be satisfied without an installed owner is
# not an amendment.**
#
# The amended rule is a function over the two production files, so it can
# be exercised by controls:
#
#   * the production `lib.rs` must dispatch the inbound path through
#     `transit_composition::dispatch_transit_inbound`, which makes the
#     whole owner/no-owner choice internally;
#   * the production `lib.rs` must NOT reach
#     `controlled_transit_disabled_probe` directly, and must NOT call
#     `TransitLiveOwner::handle_inbound` directly. The owner is reached
#     only through the composition module, so the vestigial "probe beside
#     a live owner" shape -- and the "live owner beside the composition"
#     shape -- are failures, not passes;
#   * the composition module must keep the disabled probe (the fallback
#     must exist), must reach the live owner's `handle_inbound`, and must
#     own the construction entry point;
#   * the production `lib.rs` must still reference the live-owner
#     module, and must construct the owner through the composition
#     module's entry point.
#
# This runs LAST, so that every pre-existing rule above still runs and
# still reports before the Plan 502 gate is judged. Nothing else in this
# script is relaxed, and both the rule set and the control set are
# derived, never counted by hand.
python3 "$rule_python" scan "$daemon_lib" "$daemon_transit_composition" "${BASH_SOURCE[0]}" || exit 1

echo "check-m11-transit-boundaries: passed"
