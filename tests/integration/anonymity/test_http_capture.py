import importlib.util
import hashlib
import json
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("canonicalize_http_capture.py")
SPEC = importlib.util.spec_from_file_location("p308_capture", MODULE_PATH)
assert SPEC and SPEC.loader
CAPTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CAPTURE)
CHECKER_PATH = Path(__file__).parents[3] / "scripts/check-http-anonymity-evidence.py"
CHECKER_SPEC = importlib.util.spec_from_file_location("p308_checker", CHECKER_PATH)
assert CHECKER_SPEC and CHECKER_SPEC.loader
CHECKER = importlib.util.module_from_spec(CHECKER_SPEC)
CHECKER_SPEC.loader.exec_module(CHECKER)


class CanonicalizeHttpCaptureTests(unittest.TestCase):
    def test_preserves_duplicate_header_order_and_redacts_authority(self):
        raw = (
            b"GET http://a" + b"a" * 51 + b".b32.i2p/path HTTP/1.1\r\n"
            b"Host: a" + b"a" * 51 + b".b32.i2p\r\n"
            b"X-Order: first\r\nX-Order: second\r\n\r\n"
        )
        result = CAPTURE.canonicalize(raw, "absolute-alias-get")
        self.assertEqual(result["target_form"], "absolute")
        self.assertEqual(result["headers"], [
            ["host", "<FIXTURE_AUTHORITY>"],
            ["x-order", "first"],
            ["x-order", "second"],
        ])
        self.assertEqual(result["path"], "/path")

    def test_body_is_retained_as_length_and_digest_only(self):
        raw = b"POST /upload HTTP/1.1\r\nContent-Length: 4\r\n\r\ndata"
        result = CAPTURE.canonicalize(raw, "post")
        self.assertEqual(result["body_len"], 4)
        self.assertNotIn("body", result)
        self.assertEqual(len(result["body_sha256"]), 64)

    def test_rejects_conflicting_duplicate_content_length(self):
        raw = b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\na"
        with self.assertRaisesRegex(ValueError, "ambiguous-content-length"):
            CAPTURE.canonicalize(raw, "post")

    def test_rejects_header_controls_and_over_limit_body(self):
        with self.assertRaisesRegex(ValueError, "invalid-header-value"):
            CAPTURE.canonicalize(b"GET / HTTP/1.1\r\nX: a\x01b\r\n\r\n", "get")
        raw = b"POST / HTTP/1.1\r\nContent-Length: 1048577\r\n\r\n"
        with self.assertRaisesRegex(ValueError, "body-length-mismatch-or-exceeds-bound"):
            CAPTURE.canonicalize(raw, "post")

    def test_evidence_checker_fails_closed_on_missing_family(self):
        repo = Path(__file__).parents[3]
        with tempfile.TemporaryDirectory(prefix="p308-evidence-") as directory:
            root = Path(directory)
            (root / "manifest.json").write_text(json.dumps({"schema": 1, "plan": 308}))
            errors = CHECKER.validate(repo, root)
        self.assertIn("family-set-missing", errors)

    def test_evidence_checker_rejects_reference_pin_mismatch(self):
        repo = Path(__file__).parents[3]
        corpus_path = repo / "tests/integration/anonymity/http-corpus.toml"
        corpus_hash = hashlib.sha256(corpus_path.read_bytes()).hexdigest()
        with tempfile.TemporaryDirectory(prefix="p308-evidence-") as directory:
            root = Path(directory)
            (root / "manifest.json").write_text(json.dumps({
                "schema": 1,
                "plan": 308,
                "source_head": "head",
                "verified_source_head": "head",
                "corpus_sha256": corpus_hash,
                "references": {"i2pd": "0" * 40, "java_i2p": "0" * 40},
                "families": {},
            }))
            errors = CHECKER.validate(repo, root)
        self.assertIn("reference-pin-mismatch:i2pd", errors)
        self.assertIn("reference-pin-mismatch:java_i2p", errors)


if __name__ == "__main__":
    unittest.main()
