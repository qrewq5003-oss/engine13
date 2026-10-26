"""Plan §6: one input — the same output byte for byte. Builds the base of a small window twice
from the cache and compares the CSV files. Needs the source data: set MAP_CACHE to the cache
directory (fetch.py); skipped otherwise."""

import hashlib
import os
import pathlib
import subprocess
import sys

import pytest

CACHE = os.environ.get("MAP_CACHE")
BASE = pathlib.Path(__file__).resolve().parents[1] / "base.py"
WINDOW = ["7.5", "43.5", "10.5", "45.5"]  # Liguria and the western Po plain: coast, Apennines, plain
OUTPUTS = ["blocks.csv", "borders.csv", "coasts.csv", "zones.csv"]


@pytest.mark.skipif(not CACHE, reason="MAP_CACHE is not set")
def test_base_is_byte_for_byte_reproducible(tmp_path):
    digests = []
    for run in ("a", "b"):
        out = tmp_path / run
        subprocess.run([sys.executable, "-I", "-X", "utf8", str(BASE), "--cache", CACHE, "--out", str(out),
                        "--bbox", *WINDOW], check=True, capture_output=True)
        digests.append({f: hashlib.sha256((out / f).read_bytes()).hexdigest() for f in OUTPUTS})
    assert digests[0] == digests[1]
