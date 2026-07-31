# Plan 064 i2pd driver fixtures
#
# This directory is reserved for local diagnostic fixtures used by the
# Plan 064 Python harness adapter and the test matrix. The fixtures
# directory is intentionally empty in the repository; the helper
# driver and the Python adapter synthesize all required test inputs at
# runtime under a temporary directory and never read from the
# repository.
#
# No persistent secret-bearing fixture is committed. No RouterInfo
# bytes, no private keys, no Noise transcripts, and no raw log lines
# may be committed under this path.
#
# See `tests/integration/ntcp2/reference-drivers/i2pd/README.md` for
# the driver description and
# `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py` for the
# synthesized fixtures.