#!/usr/bin/env python3
"""Validate Plan 411's sanitized Java SAM/I2CP diagnostic artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path

PIN = "9134f808337b401e8e53c73734c81fab04280c9d"
RESULT_KEYS = {
    "schema", "java_pin", "topology", "attempts", "stage", "outcome",
    "protocol_result", "udp_127_0_0_1_7655_available_before_start",
    "response_sha256", "elapsed_ms",
}
MANIFEST_KEYS = {
    "schema", "java_pin", "source_revision", "bind_policy", "reseed",
    "attempts", "result_sha256", "source_trace_sha256",
}
TRACE_KEYS = {"schema", "java_pin", "facts"}
FACT_KEYS = {"source", "method", "line", "fact"}
EXPECTED_FACTS = {
    "SAMBridge.DEFAULT_DATAGRAM_HOST": ("SAMBridge.java", 'DEFAULT_DATAGRAM_HOST = "127.0.0.1"'),
    "SAMBridge.DEFAULT_DATAGRAM_PORT_INT": ("SAMBridge.java", "DEFAULT_DATAGRAM_PORT_INT = 7655"),
    "SAMBridge.getV3DatagramServer": ("SAMBridge.java", "new SAMv3DatagramServer(this, host, port, props)"),
    "SAMv3Handler.execSessionMessage.DATAGRAM": ("SAMv3Handler.java", 'style.equals("DATAGRAM") ||'),
    "SAMv3Handler.execSessionMessage.getV3DatagramServer": ("SAMv3Handler.java", "SAMv3DatagramServer dgs = bridge.getV3DatagramServer(props)"),
    "SAMv3Handler.execSessionMessage.IOException": ("SAMv3Handler.java", "catch (IOException e)"),
    "SAMv3Handler.execSessionMessage.error-reply": ("SAMv3Handler.java", "return writeString(SESSION_ERROR, e.getMessage())"),
    "SAMv3DatagramServer.constructor.bind": ("SAMv3DatagramServer.java", "_server.socket().bind(new InetSocketAddress(host, port))"),
}
HEX_256 = re.compile(r"\A[0-9a-f]{64}\Z")
SECRET_KEYS = {"destination", "private_key", "router_keys", "raw_response", "raw_log"}


def load_json(path: Path) -> dict:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.name} must contain a JSON object")
    return value


def reject_secret_keys(value: object) -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if key.lower() in SECRET_KEYS:
                raise ValueError(f"forbidden evidence key: {key}")
            reject_secret_keys(child)
    elif isinstance(value, list):
        for child in value:
            reject_secret_keys(child)


def validate(evidence_dir: Path, *, check_source: bool = True) -> None:
    result = load_json(evidence_dir / "result.json")
    manifest = load_json(evidence_dir / "manifest.json")
    trace = load_json(evidence_dir / "source-trace.json")
    for value in (result, manifest, trace):
        reject_secret_keys(value)
    if set(result) != RESULT_KEYS or set(manifest) != MANIFEST_KEYS or set(trace) != TRACE_KEYS:
        raise ValueError("diagnostic schema keys changed")
    if result["schema"] != "i2pr-java-sam-diagnostic-v1":
        raise ValueError("wrong result schema")
    if manifest["schema"] != "i2pr-java-sam-diagnostic-manifest-v1":
        raise ValueError("wrong manifest schema")
    if trace["schema"] != "i2pr-java-sam-source-trace-v1":
        raise ValueError("wrong source-trace schema")
    if trace["java_pin"] != PIN:
        raise ValueError("source-trace Java pin mismatch")
    if result["java_pin"] != PIN or manifest["java_pin"] != PIN or manifest["source_revision"] != PIN:
        raise ValueError("Java reference pin mismatch")
    if manifest["bind_policy"] != "127.0.0.1 only" or manifest["reseed"] != "disabled":
        raise ValueError("network isolation evidence missing")
    if result["topology"] != "single-fresh-java-router-loopback-only":
        raise ValueError("unexpected topology")
    if result["attempts"] != 1 or manifest["attempts"] != 1:
        raise ValueError("diagnostic must contain exactly one session-create attempt")
    if result["stage"] not in {"sam-connect", "sam-hello", "session-create"}:
        raise ValueError("unknown failure stage")
    if result["outcome"] not in {
        "established", "address-in-use", "sam-error", "timeout",
        "transport-error", "invalid-response",
    }:
        raise ValueError("unknown outcome token")
    if result["protocol_result"] not in {
        "OK", "I2P_ERROR", "no-response", "read-deadline",
        "connection-refused", "socket-error", "unavailable",
        "HELLO-not-OK", "other",
    }:
        raise ValueError("unbounded protocol result")
    pairs = {
        "established": {"OK"},
        "address-in-use": {"I2P_ERROR"},
        "sam-error": {"I2P_ERROR"},
        "timeout": {"no-response", "read-deadline"},
        "transport-error": {"connection-refused", "socket-error"},
        "invalid-response": {"HELLO-not-OK", "other"},
    }
    if result["protocol_result"] not in pairs[result["outcome"]]:
        raise ValueError("outcome/result classification mismatch")
    if result["outcome"] == "address-in-use":
        if result["stage"] != "session-create" or result["udp_127_0_0_1_7655_available_before_start"]:
            raise ValueError("address-in-use classification lacks default-port collision evidence")
    if not isinstance(result["udp_127_0_0_1_7655_available_before_start"], bool):
        raise ValueError("UDP port preflight must be boolean")
    if not isinstance(result["elapsed_ms"], int) or not 0 <= result["elapsed_ms"] <= 180000:
        raise ValueError("invalid elapsed time")
    if not HEX_256.fullmatch(result["response_sha256"]):
        raise ValueError("invalid response digest")
    if not HEX_256.fullmatch(manifest["result_sha256"]) or not HEX_256.fullmatch(manifest["source_trace_sha256"]):
        raise ValueError("invalid evidence digest")
    result_bytes = (evidence_dir / "result.json").read_bytes()
    trace_bytes = (evidence_dir / "source-trace.json").read_bytes()
    if hashlib.sha256(result_bytes).hexdigest() != manifest["result_sha256"]:
        raise ValueError("result digest mismatch")
    if hashlib.sha256(trace_bytes).hexdigest() != manifest["source_trace_sha256"]:
        raise ValueError("source trace digest mismatch")
    facts = trace["facts"]
    if not isinstance(facts, list) or len(facts) != 8:
        raise ValueError("source trace must contain all eight source facts")
    methods = set()
    for fact in facts:
        if not isinstance(fact, dict) or set(fact) != FACT_KEYS:
            raise ValueError("invalid source fact shape")
        if fact["method"] not in EXPECTED_FACTS:
            raise ValueError("unexpected source method")
        expected_source, expected_token = EXPECTED_FACTS[fact["method"]]
        if fact["source"] != expected_source:
            raise ValueError("source method/file mismatch")
        if not isinstance(fact["line"], int) or fact["line"] < 1:
            raise ValueError("invalid source line")
        expected_fact = hashlib.sha256(expected_token.encode()).hexdigest()[:16]
        if fact["fact"] != expected_fact:
            raise ValueError("invalid source fact digest")
        methods.add(fact["method"])
    if methods != set(EXPECTED_FACTS):
        raise ValueError("source trace does not cover the full bind-to-SAM-error path")

    if check_source:
        source = Path(__file__).resolve().parents[1] / "target/interop/m6-java-sources" / f"i2p.i2p-{PIN}"
        actual = __import__("subprocess").check_output(
            ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
        ).strip()
        if actual != PIN:
            raise ValueError("local Java source checkout is not at the frozen pin")
        if __import__("subprocess").check_output(
            ["git", "-C", str(source), "status", "--porcelain", "--untracked-files=no"], text=True
        ):
            raise ValueError("local Java source checkout has tracked modifications")
        for fact in facts:
            source_path = source / "apps/sam/java/src/net/i2p/sam" / fact["source"]
            lines = source_path.read_text(encoding="utf-8").splitlines()
            if fact["line"] > len(lines):
                raise ValueError("source trace line is outside source file")
            token = EXPECTED_FACTS[fact["method"]][1]
            if token not in lines[fact["line"] - 1]:
                raise ValueError("source trace line does not contain its declared fact")


def make_fixture(root: Path) -> None:
    result = {
        "schema": "i2pr-java-sam-diagnostic-v1", "java_pin": PIN,
        "topology": "single-fresh-java-router-loopback-only", "attempts": 1,
        "stage": "session-create", "outcome": "address-in-use",
        "protocol_result": "I2P_ERROR",
        "udp_127_0_0_1_7655_available_before_start": False,
        "response_sha256": "a" * 64, "elapsed_ms": 12,
    }
    methods = sorted(EXPECTED_FACTS)
    sources = {
        method: EXPECTED_FACTS[method][0]
        for method in methods
    }
    trace = {
        "schema": "i2pr-java-sam-source-trace-v1",
        "java_pin": PIN,
        "facts": [{"source": sources[method], "method": method, "line": i + 1,
                   "fact": hashlib.sha256(EXPECTED_FACTS[method][1].encode()).hexdigest()[:16]}
                  for i, method in enumerate(methods)],
    }
    result_bytes = (json.dumps(result, sort_keys=True, indent=2) + "\n").encode()
    trace_bytes = (json.dumps(trace, sort_keys=True, indent=2) + "\n").encode()
    manifest = {
        "schema": "i2pr-java-sam-diagnostic-manifest-v1", "java_pin": PIN,
        "source_revision": PIN, "bind_policy": "127.0.0.1 only", "reseed": "disabled",
        "attempts": 1, "result_sha256": hashlib.sha256(result_bytes).hexdigest(),
        "source_trace_sha256": hashlib.sha256(trace_bytes).hexdigest(),
    }
    (root / "result.json").write_bytes(result_bytes)
    (root / "source-trace.json").write_bytes(trace_bytes)
    (root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="i2pr-plan411-check-") as directory:
        root = Path(directory)
        make_fixture(root)
        validate(root, check_source=False)
        mutations = [
            ("attempts", lambda x: x.__setitem__("attempts", 2)),
            ("pin", lambda x: x.__setitem__("java_pin", "0" * 40)),
            ("topology", lambda x: x.__setitem__("topology", "public-network")),
            ("stage", lambda x: x.__setitem__("stage", "matrix")),
            ("classification", lambda x: x.__setitem__("outcome", "success-ish")),
            ("unbounded result", lambda x: x.__setitem__("protocol_result", "private payload")),
            ("port preflight", lambda x: x.__setitem__("udp_127_0_0_1_7655_available_before_start", "false")),
            ("response digest", lambda x: x.__setitem__("response_sha256", "bad")),
            ("secret field", lambda x: x.__setitem__("destination", "must-not-appear")),
        ]
        for name, mutation in mutations:
            make_fixture(root)
            result = load_json(root / "result.json")
            mutation(result)
            (root / "result.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
            manifest = load_json(root / "manifest.json")
            manifest["result_sha256"] = hashlib.sha256((root / "result.json").read_bytes()).hexdigest()
            (root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
            try:
                validate(root, check_source=False)
            except (ValueError, KeyError):
                continue
            raise AssertionError(f"checker accepted mutation: {name}")
        make_fixture(root)
        result = load_json(root / "result.json")
        result["udp_127_0_0_1_7655_available_before_start"] = True
        (root / "result.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
        manifest = load_json(root / "manifest.json")
        manifest["result_sha256"] = hashlib.sha256((root / "result.json").read_bytes()).hexdigest()
        (root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
        try:
            validate(root, check_source=False)
        except ValueError:
            pass
        else:
            raise AssertionError("checker accepted address-in-use without a port collision")
        make_fixture(root)
        trace = load_json(root / "source-trace.json")
        trace["facts"].pop()
        (root / "source-trace.json").write_text(json.dumps(trace, sort_keys=True, indent=2) + "\n")
        manifest = load_json(root / "manifest.json")
        manifest["source_trace_sha256"] = hashlib.sha256((root / "source-trace.json").read_bytes()).hexdigest()
        (root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
        try:
            validate(root, check_source=False)
        except ValueError:
            pass
        else:
            raise AssertionError("checker accepted a missing source-path fact")
        make_fixture(root)
        manifest = load_json(root / "manifest.json")
        manifest["result_sha256"] = "0" * 64
        (root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
        try:
            validate(root, check_source=False)
        except ValueError:
            pass
        else:
            raise AssertionError("checker accepted changed result without manifest digest")
        print(f"check-java-sam-diagnostic.py --self-test: {len(mutations) + 3} mutations rejected")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence_dir", nargs="?", type=Path,
                        default=Path("target/interop/java-sam-diagnostic-plan411"))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    try:
        validate(args.evidence_dir)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"Java SAM diagnostic evidence rejected: {error}", file=sys.stderr)
        return 1
    print("Java SAM diagnostic evidence validated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
