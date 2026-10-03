import csv
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[3]
CHECKER = REPO / "scripts" / "check-streaming-fingerprint-evidence.sh"
PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
HEADER = (
    "scenario\tindex\ttime_10ms\tdirection\tflags\tpayload_len\tseq_delta"
    "\tack_delta\tretransmit_ordinal\tmax_payload\tadvertised_window"
    "\tchoked\tterminal\n"
)
ROLES = {
    "i2pr-client": "to_destination",
    "i2pd-client": "to_destination",
    "i2pr-server": "from_destination",
    "i2pd-server": "from_destination",
}


class StreamingFingerprintEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.write_valid_evidence()

    def tearDown(self):
        self.temp.cleanup()

    def write_valid_evidence(self):
        (self.root / "fingerprint-manifest.tsv").write_text(
            "key\tvalue\n"
            "i2pd_version\t2.61.0\n"
            f"i2pd_revision\t{PIN}\n"
            "scenario\tclean_handshake_default_port\n"
            "dimensions\tflags,from_included,max_payload,payload_length\n"
            "raw_packet_bytes\t0\n"
            "destination_or_stream_ids\t0\n",
            encoding="utf-8",
        )
        for role, direction in ROLES.items():
            flags = 0x00B1 if "client" in role else 0x0031
            (self.root / f"fingerprint-{role}.tsv").write_text(
                HEADER
                + f"clean_handshake\t0\t0\t{direction}\t{flags}\t0\t0\t0\t0\t1730\t-\t-\t-\n",
                encoding="utf-8",
            )

    def run_checker(self):
        return subprocess.run(
            [str(CHECKER), str(self.root)],
            cwd=REPO,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_valid_two_family_directional_matrix(self):
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)
        with (self.root / "fingerprint-matrix.tsv").open(encoding="utf-8") as stream:
            matrix = list(csv.DictReader(stream, delimiter="\t"))
        self.assertEqual(len(matrix), 8)
        self.assertIn({"role": "client", "dimension": "flags", "i2pr": "177", "i2pd": "177", "result": "match"}, matrix)
        self.assertIn({"role": "server", "dimension": "flags", "i2pr": "49", "i2pd": "49", "result": "match"}, matrix)

    def test_missing_direction_fails_closed(self):
        (self.root / "fingerprint-i2pd-server.tsv").unlink()
        result = self.run_checker()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing trace: i2pd-server", result.stderr)

    def test_reference_pin_drift_fails_closed(self):
        path = self.root / "fingerprint-manifest.tsv"
        path.write_text(path.read_text(encoding="utf-8").replace(PIN, "0" * 40), encoding="utf-8")
        result = self.run_checker()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("manifest i2pd_revision mismatch", result.stderr)

    def test_trace_extra_column_fails_closed(self):
        path = self.root / "fingerprint-i2pr-client.tsv"
        path.write_text(path.read_text(encoding="utf-8").rstrip() + "\tsecret\n", encoding="utf-8")
        result = self.run_checker()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("malformed trace row: i2pr-client", result.stderr)


if __name__ == "__main__":
    unittest.main()
