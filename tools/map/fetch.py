"""Download the map pipeline's source data into the cache and check it against sources.toml.

    python -I tools/map/fetch.py [--cache D:/map-cache] [--record]

Files land in <cache>/src. A file already present is not downloaded again, only checked.
`--record` writes the hashes of the files in the cache into sources.toml (the first fetch,
or a deliberate update of a source); without it a mismatch is an error.
"""

import argparse
import hashlib
import http.client
import json
import pathlib
import re
import sys
import tomllib
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
SOURCES = HERE / "sources.toml"
CHUNK = 1 << 20


def sha256_of(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        while block := f.read(CHUNK):
            h.update(block)
    return h.hexdigest()


def md5_of(path: pathlib.Path) -> str:
    h = hashlib.md5()
    with path.open("rb") as f:
        while block := f.read(CHUNK):
            h.update(block)
    return h.hexdigest()


def download(url: str, dest: pathlib.Path, attempts: int = 8) -> None:
    """Download with resumption: a broken transfer continues from the bytes already in
    `<dest>.part` (HTTP Range)."""
    part = dest.with_suffix(dest.suffix + ".part")
    for attempt in range(1, attempts + 1):
        have = part.stat().st_size if part.exists() else 0
        headers = {"User-Agent": "engine13-map-fetch"}
        if have:
            headers["Range"] = f"bytes={have}-"
        req = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                mode = "ab" if have and resp.status == 206 else "wb"
                with part.open(mode) as out:
                    while block := resp.read(CHUNK):
                        out.write(block)
            break
        except (OSError, http.client.HTTPException) as e:
            print(f"  attempt {attempt}: {e!r}", flush=True)
            if attempt == attempts:
                raise
    part.replace(dest)


def normalise_wfs(dest: pathlib.Path) -> None:
    """Drop the response's timestamp and counters, sort features: the bytes then depend on
    the data alone."""
    data = json.loads(dest.read_text(encoding="utf-8"))
    feats = sorted(data["features"], key=lambda f: str(f["properties"].get("mrgid")))
    for f in feats:
        f.pop("id", None)
    out = {"type": "FeatureCollection", "features": feats}
    dest.write_text(json.dumps(out, ensure_ascii=False, sort_keys=True, separators=(",", ":")),
                    encoding="utf-8")


def record(name: str, digest: str) -> None:
    text = SOURCES.read_text(encoding="utf-8")
    pattern = re.compile(r"(\[" + re.escape(name) + r"\][^\[]*?sha256 = )\"[0-9a-f]*\"", re.S)
    text, n = pattern.subn(lambda m: m.group(1) + f'"{digest}"', text, count=1)
    if n != 1:
        sys.exit(f"cannot record sha256 of {name}")
    SOURCES.write_text(text, encoding="utf-8")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default="D:/map-cache")
    ap.add_argument("--record", action="store_true")
    args = ap.parse_args()
    src = pathlib.Path(args.cache) / "src"
    src.mkdir(parents=True, exist_ok=True)
    sources = tomllib.loads(SOURCES.read_text(encoding="utf-8"))
    bad = []
    for name, s in sources.items():
        dest = src / s["file"]
        if not dest.exists():
            print(f"{name}: downloading {s['url']}", flush=True)
            download(s["url"], dest)
            if s.get("wfs"):
                normalise_wfs(dest)
        if "md5" in s and md5_of(dest) != s["md5"]:
            bad.append(f"{name}: md5 differs from the publisher's")
            continue
        digest = sha256_of(dest)
        size = dest.stat().st_size
        if args.record:
            record(name, digest)
            print(f"{name}: {size} bytes, sha256 {digest} recorded")
        elif digest != s["sha256"]:
            bad.append(f"{name}: sha256 {digest} != {s['sha256'] or '(none recorded)'}")
        else:
            print(f"{name}: {size} bytes, ok")
    if bad:
        sys.exit("\n".join(bad))


if __name__ == "__main__":
    main()
