"""Plan §6: one input — the same output byte for byte. Builds the base of a small window twice
from the cache and compares the CSV files. Needs the source data: set MAP_CACHE to the cache
directory (fetch.py); skipped otherwise."""

import csv
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
def test_base_does_not_depend_on_the_bbox(tmp_path):
    """Plan §15a п. 2: widening the bbox leaves the blocks away from the old edge as they were —
    the layouts already made keep their numbers when a new scenario widens the base."""
    rows = []
    for run, window in (("narrow", WINDOW), ("wide", ["6.5", "42.5", "11.5", "46.5"])):
        out = tmp_path / run
        subprocess.run([sys.executable, "-I", "-X", "utf8", str(BASE), "--cache", CACHE, "--out", str(out),
                        "--bbox", *window], check=True, capture_output=True)
        with (out / "blocks.csv").open(encoding="utf-8") as f:
            rows.append({r["id"]: r for r in csv.DictReader(f)})
    x0, y0, x1, y1 = (float(v) for v in WINDOW)
    inner = [k for k, r in rows[0].items()
             if x0 + 0.5 <= float(r["lon"]) <= x1 - 0.5 and y0 + 0.5 <= float(r["lat"]) <= y1 - 0.5]
    assert len(inner) >= 5  # Italian blocks are whole provinces: Genoa, Alessandria, Asti, Pavia…
    assert [rows[0][k] for k in inner] == [rows[1].get(k) for k in inner]


@pytest.mark.skipif(not CACHE, reason="MAP_CACHE is not set")
def test_base_is_byte_for_byte_reproducible(tmp_path):
    digests = []
    for run in ("a", "b"):
        out = tmp_path / run
        subprocess.run([sys.executable, "-I", "-X", "utf8", str(BASE), "--cache", CACHE, "--out", str(out),
                        "--bbox", *WINDOW], check=True, capture_output=True)
        digests.append({f: hashlib.sha256((out / f).read_bytes()).hexdigest() for f in OUTPUTS})
    assert digests[0] == digests[1]
