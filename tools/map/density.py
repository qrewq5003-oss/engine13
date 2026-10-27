"""Derive the land-type densities together with the land classification (plan
docs/engine2_map.md §11): non-negative least squares over regions whose population at the
Malthusian ceiling has a source (density.toml), with the regions' areas by land type taken
from the map. The classification parameters — the relief threshold and window, the dry-land
rule — are chosen by the residual of the fit.

    python -I -X utf8 tools/map/density.py [--cache D:/map-cache]

Reads the base (<cache>/out/base) and the rasters; writes <cache>/out/density/report.txt.

A cell of 1′ (ETOPO 2022) takes one type, in this order:
- Irrigated — the cell lies in a block of an irrigated region (density.toml `irrigated`):
  an irrigated valley is a fact of the source (Egypt's cultivated land), not of the climate;
- Hills — local relief at least `relief_m` in the window;
- Desert — Köppen BW; Steppe — BS; otherwise Arable.
A unit (a block, or a pseudo-province of blocks on a lon/lat grid) takes the type of its
majority the same way, with `steppe_share` for desert and steppe together.
"""

import argparse
import collections
import csv
import itertools
import pathlib
import sys
import tomllib

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import geopandas as gpd  # noqa: E402
import numpy as np  # noqa: E402
import pyogrio  # noqa: E402
import rasterio  # noqa: E402
import rasterio.features  # noqa: E402
import rasterio.windows  # noqa: E402
import shapely  # noqa: E402

import base  # noqa: E402
import geo  # noqa: E402
import rules  # noqa: E402

TYPES = ["Irrigated", "Hills", "Desert", "Steppe", "Arable"]
I, H, D, S, A = range(5)
FITTED = [A, H, S]
E0 = {"Arable": (40.0, 26.0, 42.0), "Hills": (12.0, 8.0, 19.0), "Steppe": (1.0, 0.5, 2.0),
      "Desert": (0.5, 0.1, 1.0)}
LAND_REGION_DIFF = 0.10


def read_blocks(base_dir: pathlib.Path):
    with (base_dir / "blocks.csv").open(encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    gdf = gpd.read_file(base_dir / "blocks.gpkg", layer="blocks").set_index("id").loc[[r["id"] for r in rows]]
    climate = np.array([float(r["climate_km2"]) for r in rows])
    with np.errstate(invalid="ignore", divide="ignore"):
        bw = np.nan_to_num(np.array([float(r["bw_km2"]) for r in rows]) / climate)
        bs = np.nan_to_num(np.array([float(r["bs_km2"]) for r in rows]) / climate)
        cold = np.nan_to_num(np.array([float(r["cold_km2"]) for r in rows]) / climate)
    return {
        "id": np.array([r["id"] for r in rows]), "country": np.array([r["country"] for r in rows]),
        "name": np.array([r["name"] for r in rows]),
        "lon": np.array([float(r["lon"]) for r in rows]), "lat": np.array([float(r["lat"]) for r in rows]),
        "area": np.array([float(r["area_km2"]) for r in rows]), "bw": bw, "dry": bw + bs, "cold": cold,
        "geometry": gdf.geometry.values,
    }


def region_mask(blocks, parts) -> np.ndarray:
    m = np.zeros(len(blocks["id"]), dtype=bool)
    for p in parts:
        k = np.isin(blocks["country"], p["countries"])
        if "names" in p:
            k &= np.isin(blocks["name"], p["names"])
        if "exclude_names" in p:
            k &= ~np.isin(blocks["name"], p["exclude_names"])
        for key, arr, op in (("lon_min", "lon", np.greater_equal), ("lon_max", "lon", np.less),
                             ("lat_min", "lat", np.greater_equal), ("lat_max", "lat", np.less),
                             ("max_area_km2", "area", np.less), ("max_bw", "bw", np.less),
                             ("min_dry", "dry", np.greater_equal)):
            if key in p:
                k &= op(blocks[arr], p[key])
        if "coastal" in p:  # blocks with a shore on some sea area (layouts only)
            k &= blocks["coastal"] == p["coastal"]
        m |= k
    return m


def region_masks(cfg, blocks) -> dict:
    masks = {r: region_mask(blocks, v["parts"]) for r, v in cfg["regions"].items()}
    for r, v in cfg["regions"].items():
        for other in v.get("minus", []):
            masks[r] &= ~masks[other]
    return masks


def envelope(cfg, blocks, masks):
    """The observations with their densities on the map, and the envelope of each land.
    A land's capacity is the highest density it is documented to have held before a plague
    (E0 П1: «ёмкость — свойство земли, не года»); of the observations of one land, only the
    highest is kept. Observations are one land only if their regions are one land: they may
    differ by at most LAND_REGION_DIFF of the area of their union. An observation marked `later`
    (a later pre-industrial point) is no envelope: it only tells a ceiling from a lower bound
    (`sparse_lands`)."""
    obs = [dict(o) for o in cfg["obs"]]
    for o in obs:
        a = blocks["area"][masks[o["region"]]].sum()
        lo, hi = o["density"] if "density" in o else (o["population"][0] * 1e6 / a, o["population"][1] * 1e6 / a)
        o["lo"], o["hi"], o["y"], o["area"] = lo, hi, (lo + hi) / 2, a
    top, land_regions = {}, collections.defaultdict(set)
    for o in obs:
        if o.get("later"):
            continue
        g = o.get("land", o["id"])
        land_regions[g].add(o["region"])
        if g not in top or o["y"] > top[g]["y"]:
            top[g] = o
    for g, regions in land_regions.items():
        union = np.any(np.stack([masks[r] for r in regions]), axis=0)
        common = np.all(np.stack([masks[r] for r in regions]), axis=0)
        diff = blocks["area"][union & ~common].sum() / blocks["area"][union].sum()
        if diff > LAND_REGION_DIFF:
            raise SystemExit(f"land {g}: its observations' regions {sorted(regions)} differ by {100 * diff:.0f} % of the area")
    return obs, top


def sparse_lands(obs, top, dens, below: float) -> set:
    """Lands below Hills whose source is a ceiling, not a lower bound (owner's decision on §15a
    п. 4): a later pre-industrial observation of the land stays under `below` × Hills, so the
    land never came near Hills before industry (Russia, Scandinavia). Such a land may turn its
    provinces sparse — Steppe, «редкая земля» — the coldest first. A land below Hills without a
    later observation keeps Hills: its source is a lower bound (Hungary, §13a п. 4)."""
    later = collections.defaultdict(list)
    for o in obs:
        if o.get("later"):
            later[o["land"]].append(o["y"])
    return {land for land, ys in later.items() if land in top and top[land]["y"] < dens[H] and max(ys) < below * dens[H]}


def cell_type_areas(blocks, members: np.ndarray, irrigated: np.ndarray, src, sources, scan, rep):
    """km² of each cell type in each block (blocks of some region only), for every relief
    threshold and window."""
    idx = np.flatnonzero(members)
    geoms = blocks["geometry"][idx]
    x0, y0, x1, y1 = shapely.total_bounds(geoms)
    with rasterio.open(str(src / sources["etopo_2022_60s"]["file"])) as r:
        w = rasterio.windows.from_bounds(x0 - 0.2, y0 - 0.2, x1 + 0.2, y1 + 0.2, transform=r.transform)
        w = w.round_offsets().round_lengths()
        elev = r.read(1, window=w).astype(np.float32)
        et = r.window_transform(w)
    # The 30″ climate grid nests in the 1′ relief grid (both start at 180° W, 90° N): a cell is
    # BW or BS when at least two of its four climate cells are.
    kop = base.vsizip(src, sources["koppen_beck_2018"]["file"], "Beck_KG_V1_present_0p0083.tif")
    with rasterio.open(kop) as r:
        cls = r.read(1, window=rasterio.windows.Window(w.col_off * 2, w.row_off * 2, w.width * 2, w.height * 2))
    h, wd = elev.shape
    quad = cls.reshape(h, 2, wd, 2)
    bw = np.isin(quad, base.KOPPEN_BW).sum(axis=(1, 3)) >= 2
    bs = (np.isin(quad, base.KOPPEN_BS).sum(axis=(1, 3)) >= 2) & ~bw
    del cls, quad
    land = pyogrio.read_dataframe(base.vsizip(src, sources["ne_land"]["file"], "ne_10m_land.shp"), bbox=(x0, y0, x1, y1))
    landmask = rasterio.features.rasterize(((g, 1) for g in shapely.get_parts(land.geometry.values)),
                                           out_shape=elev.shape, transform=et, fill=0, dtype="uint8")
    elev[landmask == 0] = 0.0
    labels = rasterio.features.rasterize(((g, k + 1) for k, g in enumerate(geoms)), out_shape=elev.shape,
                                         transform=et, fill=0, dtype="int32")
    wet = np.concatenate([[False], irrigated[idx]])[labels]
    area = np.broadcast_to(geo.row_cell_area_km2(et, h)[:, None], elev.shape)
    n = len(idx)
    out = {}
    for win in scan["relief_window_cells"]:
        relief = geo.local_relief(elev, win)
        for rm in scan["relief_m"]:
            code = np.full(elev.shape, A, dtype=np.int64)
            code[bs] = S
            code[bw] = D
            code[relief >= rm] = H
            code[wet] = I
            sums = np.bincount((labels * 5 + code).ravel(), weights=area.ravel(), minlength=(n + 1) * 5)
            full = np.zeros((len(blocks["id"]), 5))
            full[idx] = sums.reshape(n + 1, 5)[1:]
            out[(rm, win)] = full
        rep.step(f"window {win}: classes measured")
    return out


def unit_type(shares: np.ndarray, steppe_share: float) -> np.ndarray:
    """Majority type of units from their shares (rows: units, columns: TYPES)."""
    t = np.full(len(shares), A)
    t[shares[:, D] + shares[:, S] >= steppe_share] = S
    t[shares[:, D] >= 0.5] = D
    t[shares[:, H] >= 0.5] = H
    t[shares[:, I] >= 0.5] = I
    return t


def region_shares(level, areas, mask, blocks, steppe_share):
    """Share of each type in a region: of its cells (level "cell"), of its blocks typed by
    majority ("block"), or of grid pseudo-provinces typed by majority ("grid:<deg>")."""
    a = areas[mask]
    if level == "cell":
        tot = a.sum(axis=0)
    else:
        if level == "block":
            groups = np.arange(len(a))
        else:
            g = float(level.split(":")[1])
            key = np.floor(blocks["lon"][mask] / g) * 1000 + np.floor(blocks["lat"][mask] / g)
            groups = np.unique(key, return_inverse=True)[1].ravel()
        units = np.zeros((groups.max() + 1, 5))
        np.add.at(units, groups, a)
        size = units.sum(axis=1)
        keep = size > 0
        units, size = units[keep], size[keep]
        tot = np.bincount(unit_type(units / size[:, None], steppe_share), weights=size, minlength=5)
    return tot / tot.sum()


def nnls_relative(X: np.ndarray, y: np.ndarray) -> np.ndarray:
    """min Σ ((X d − y) / y)² with d ≥ 0, by trying every set of free variables (few)."""
    Xw, yw = X / y[:, None], np.ones_like(y)
    best = None
    k = X.shape[1]
    for r in range(k, 0, -1):
        for free in itertools.combinations(range(k), r):
            free = list(free)
            sol, *_ = np.linalg.lstsq(Xw[:, free], yw, rcond=None)
            if (sol < 0).any():
                continue
            d = np.zeros(k)
            d[free] = sol
            rss = float(((Xw @ d - yw) ** 2).sum())
            if best is None or rss < best[1] - 1e-12:
                best = (d, rss)
    return best[0]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default="D:/map-cache")
    args = ap.parse_args()
    cache = pathlib.Path(args.cache)
    src = cache / "src"
    out = cache / "out" / "density"
    out.mkdir(parents=True, exist_ok=True)
    cfg = tomllib.loads((HERE / "density.toml").read_text(encoding="utf-8"))
    sources = tomllib.loads((HERE / "sources.toml").read_text(encoding="utf-8"))
    scan = cfg["scan"]
    rep = base.Report()

    blocks = read_blocks(cache / "out" / "base")
    masks = region_masks(cfg, blocks)
    irrigated = np.any(np.stack([masks[r] for r in scan["irrigated_regions"]]), axis=0)
    members = np.any(np.stack(list(masks.values())), axis=0)
    obs, top = envelope(cfg, blocks, masks)
    # Sub-lands (`fit = false`) split a land the fit already has; they serve the calibration only.
    fit = [o for o in obs if o["region"] not in scan["irrigated_regions"] and o.get("fit", True)]
    sets = {"все наблюдения": fit,
            "огибающая (наибольшая плотность земли)": [o for o in top.values() if o in fit]}
    wet_obs = [o for o in obs if o["region"] in scan["irrigated_regions"]]
    areas = cell_type_areas(blocks, members, irrigated, src, sources, scan, rep)

    rep("# Плотности по типам земли вместе с классификацией — подгонка")
    rep()
    rep("## Наблюдения")
    rep("| наблюдение | земля | регион карты | площадь, км² | плотность, чел./км² | источник |")
    rep("|---|---|---|---|---|---|")
    for o in obs:
        rep(f"| {o['id']} | {o.get('land', '—')} | {o['region']} | {o['area']:,.0f} | {o['lo']:.1f}–{o['hi']:.1f} | {o['source']} |")
    rep()
    for o in wet_obs:
        rep(f"**Irrigated** — не подгоняется: регион `{o['region']}` весь орошаемый по определению, плотность = "
            f"наблюдению {o['lo']:.0f}–{o['hi']:.0f} (середина {o['y']:.0f}) чел./км².")
    rep()

    levels = ["cell", "block"] + [f"grid:{g:g}" for g in scan["grid_deg"]]
    dd = scan["desert_density"]
    for set_name, ob in sets.items():
        y = np.array([o["y"] for o in ob])
        rep(f"# Набор: {set_name} ({len(ob)})")
        rep()
        for level in levels:
            results = []
            for (rm, win), ar in sorted(areas.items()):
                for s in (scan["steppe_share"] if level != "cell" else scan["steppe_share"][:1]):
                    shares = np.array([region_shares(level, ar, masks[o["region"]], blocks, s) for o in ob])
                    d = nnls_relative(shares[:, FITTED], y - dd * shares[:, D])
                    pred = shares[:, FITTED] @ d + dd * shares[:, D]
                    e0 = shares[:, [A, H, S, D]] @ np.array([E0[t][0] for t in ("Arable", "Hills", "Steppe", "Desert")])
                    rel = (pred - y) / y
                    results.append({"rm": rm, "win": win, "s": s, "d": d, "pred": pred, "rel": rel, "e0": e0,
                                    "rms": float(np.sqrt((rel ** 2).mean())),
                                    "rms_e0": float(np.sqrt((((e0 - y) / y) ** 2).mean())), "shares": shares})
            rs = sorted(results, key=lambda r: r["rms"])
            rep(f"## {set_name} — уровень {level}")
            rep()
            rep("| порог, м | окно | степь ≥ | Arable | Hills | Steppe | RMS подгонки | RMS при плотностях E0 |")
            rep("|---|---|---|---|---|---|---|---|")
            step1 = [r for r in results if r["rm"] == 300.0 and r["win"] == 5 and r["s"] == 0.5]
            for r in rs[:6] + step1:
                mark = " (шаг 1)" if r in step1 else ""
                rep(f"| {r['rm']:g}{mark} | {r['win']} | {r['s']:g} | {r['d'][0]:.1f} | {r['d'][1]:.1f} | {r['d'][2]:.2f} | "
                    f"{100 * r['rms']:.0f} % | {100 * r['rms_e0']:.0f} % |")
            rep()
            b = rs[0]
            rep(f"Лучшая: порог {b['rm']:g} м, окно {b['win']}, степь ≥ {b['s']:g}.")
            rep()
            rep("| наблюдение | источник | подгонка | невязка | при E0 | невязка E0 | Hills | Desert | Steppe | Arable |")
            rep("|---|---|---|---|---|---|---|---|---|---|")
            for k, o in enumerate(ob):
                sh = b["shares"][k]
                rep(f"| {o['id']} | {o['lo']:.1f}–{o['hi']:.1f} | {b['pred'][k]:.1f} | {100 * b['rel'][k]:+.0f} % | "
                    f"{b['e0'][k]:.1f} | {100 * (b['e0'][k] - y[k]) / y[k]:+.0f} % | "
                    f"{100 * sh[H]:.0f} % | {100 * sh[D]:.0f} % | {100 * sh[S]:.0f} % | {100 * sh[A]:.0f} % |")
            rep()
    calibration(cfg, obs, top, masks, blocks, cache / "out" / "base", rep)
    (out / "report.txt").write_text("\n".join(rep.lines) + "\n", encoding="utf-8")
    rep.step("done")


def calibrate(area, rugged, dry, desert, target, dens, key=None, dry_steppe=None, cold=None) -> np.ndarray:
    """Classes (TYPES indices) of the units of one land — blocks, or a scenario's provinces —
    whose capacity comes closest to `target` people at the densities `dens` (indexed by type).
    Desert units stay Desert. Of the rest, the most rugged become Hills first; a land sparser
    than Hills everywhere is all Hills, and its driest (then flattest) units become Steppe
    first. Units are whole: a unit changes class only while that brings the capacity closer
    to the target, so a land of few units keeps a residual.
    Variants measured for the owner (plan §13b): `key` — the order to Hills (default: rugged);
    `dry_steppe` — only units at least this dry may become Steppe (default: any).
    `cold` — a land whose source is a ceiling below Hills (`sparse_lands`): any unit may become
    Steppe, the coldest first (subarctic and polar share), then the driest; `dry_steppe` is not
    applied."""
    key = rugged if key is None else key
    cls = np.where(desert, D, A)
    rest = np.flatnonzero(~desert)
    if target < area[rest].sum() * dens[H] + area[desert].sum() * dens[D]:
        cls[rest] = H
        if cold is not None:
            to, order = S, rest[np.lexsort((rugged[rest], -dry[rest], -cold[rest]))]
        else:
            to, order = S, rest[np.lexsort((rugged[rest], -dry[rest]))]
            if dry_steppe is not None:
                order = order[dry[order] >= dry_steppe]
    else:
        to, order = H, rest[np.argsort(-key[rest], kind="stable")]
    cap = float(area @ dens[cls])
    for k in order:
        after = cap - area[k] * (dens[cls[k]] - dens[to])
        if abs(after - target) >= abs(cap - target):
            break
        cls[k], cap = to, after
    return cls


def calibrate_nearest(area, rugged, dry, desert, target, dens, dry_steppe=None, cold=None):
    """The alternative to `calibrate` (reviewer on 500f44c): no whole unit flips 50 ↔ 12 for a
    fraction of a per cent of its land. Each unit gets a continuous density
    d = A − (A − H)·min(1, k·rugged), with k found so that the land holds its ceiling (no new
    constant), and takes the class nearest to its d; the land keeps whatever residual the classes
    leave. Desert units stay Desert; a land sparser than Hills everywhere is left to `calibrate`.
    Returns the classes, the densities d and k."""
    cls = np.where(desert, D, A)
    rest = ~desert
    need = target - float(area[desert].sum()) * dens[D]
    if need <= area[rest].sum() * dens[H]:
        cls = calibrate(area, rugged, dry, desert, target, dens, dry_steppe=dry_steppe, cold=cold)
        return cls, dens[cls].astype(float), float("nan")
    a, r = area[rest], rugged[rest]

    def cap(k):
        return float(a @ (dens[A] - (dens[A] - dens[H]) * np.minimum(1.0, k * r)))

    k = 0.0
    if need < cap(0.0):
        hi = 1.0 / r[r > 0].min() if (r > 0).any() else 0.0
        if cap(hi) >= need:
            k = hi
        else:
            lo = 0.0
            for _ in range(100):  # bisection: cap() falls as k grows
                mid = (lo + hi) / 2
                lo, hi = (mid, hi) if cap(mid) > need else (lo, mid)
            k = (lo + hi) / 2
    d = dens[cls].astype(float)
    d[rest] = dens[A] - (dens[A] - dens[H]) * np.minimum(1.0, k * r)
    cls[rest] = np.where(d[rest] >= (dens[A] + dens[H]) / 2, A, H)
    return cls, d, k


def hills_share(area, poor, desert, target, dens):
    """The hills share h of the units of one land (plan §12b, a question for the owner at E4): each
    unit holds A·(1 − h) + H·h people per km² with h = min(1, k·poor), k chosen so that the land
    holds `target` people. Desert units stay Desert. A land at or below Hills everywhere is all
    h = 1, one at or above Arable all h = 0; units with no poor cells cannot leave Arable, so a
    land may keep a residual. Returns h per unit and k."""
    h = np.zeros(len(area))
    rest = ~desert
    need = target - float(area[desert] @ np.full(int(desert.sum()), dens[D]))
    a, r = area[rest], poor[rest]

    def cap(k):
        return float(a @ (dens[A] - (dens[A] - dens[H]) * np.minimum(1.0, k * r)))

    if need >= cap(0.0):
        return h, 0.0
    hi = 1.0 / r[r > 0].min() if (r > 0).any() else 0.0
    if cap(hi) >= need:
        k = hi
    else:
        lo = 0.0
        for _ in range(100):  # bisection: cap() falls as k grows
            mid = (lo + hi) / 2
            lo, hi = (mid, hi) if cap(mid) > need else (lo, mid)
        k = (lo + hi) / 2
    h[rest] = np.minimum(1.0, k * r)
    return h, k


def land_depth(land: str, lands: dict) -> int:
    d = 0
    while land in lands and "within" in lands[land]:
        d, land = d + 1, lands[land]["within"]
    return d


def calibrate_lands(order, member, ceiling, area, rugged, dry, bw, dens, p: rules.Params, method="flip",
                    sparse=frozenset(), cold=None, river=frozenset(), **variant):
    """Classes of units (blocks, or a scenario's provinces) for the lands of a scenario, with
    each unit's density (its class's, or the continuous one of `calibrate_nearest`).
    `order` — the lands, inner ones first; `member[land]` — its units (an inner land's units
    are members of the outer one too); `ceiling[land]` — people per km², or None for a land
    without an opened ceiling, which takes the §4 class (owner's rule 1). An outer land meets
    its own ceiling with the units its inner lands left. `method` — "flip" (`calibrate`) or
    "nearest" (`calibrate_nearest`). Steppe only where the climate is dry, BW + BS ≥ the §4 steppe
    share (owner's decision §13a п. 4): a land below Hills that is not dry stays Hills — unless
    the land is in `sparse` (`sparse_lands`): then any unit may turn Steppe, by `cold`, the
    coldest first (owner's decision on §15a п. 4). A land in `river` lives off its river — the
    Sawad: its desert climate does not hold its units at Desert; its source sets the class."""
    variant.setdefault("dry_steppe", p.steppe_share)
    cls = np.full(len(area), -1)
    d = np.full(len(area), np.nan)
    for land in order:
        m = member[land]
        free = m & (cls < 0)
        if ceiling[land] is not None:
            fixed = m & (cls >= 0)
            target = ceiling[land] * area[m].sum() - float(area[fixed] @ dens[cls[fixed]])
            desert = (bw[free] >= 0.5) & (land not in river)
            args = (area[free], rugged[free], dry[free], desert, target, dens)
            c = cold[free] if land in sparse else None
            if method == "nearest":
                cls[free], d[free], _ = calibrate_nearest(*args, dry_steppe=variant["dry_steppe"], cold=c)
            else:
                cls[free] = calibrate(*args, **{k: (v[free] if isinstance(v, np.ndarray) else v) for k, v in variant.items()},
                                      **({"cold": c} if c is not None else {}))
        else:
            cls[free] = rule_class(rugged[free], bw[free], dry[free], p, cap_hills=True)
    rest = (cls >= 0) & np.isnan(d)
    d[rest] = dens[cls[rest]]
    return cls, d


def rule_class(rugged, bw, dry, p: rules.Params, cap_hills=False) -> np.ndarray:
    """Plan §4 for a land without an opened ceiling (owner's rule 1, §11a): relief, then climate.
    `cap_hills` — a land without a source is not above Hills (owner's decision on §13b п. 1):
    level ground that is not dry takes Hills, not the Arable density of the densest land."""
    cls = np.array([TYPES.index(rules.land_type(r, b, d - b, p)) for r, b, d in zip(rugged, bw, dry)], dtype=np.int64)
    if cap_hills:
        cls[cls == A] = H
    return cls


def shares_row(area, cls) -> str:
    tot = area.sum()
    return " | ".join(f"{100 * area[cls == t].sum() / tot:.0f} %" for t in (H, S, D, A))


def plague_balance_fill(engine_params: dict) -> float:
    """The fill at which engine2's logistic growth r·(1 − fill) equals the mean plague loss
    (outbreaks × mortality per year): a land above it shrinks in a world with plague."""
    loss = engine_params["plague"]["outbreaks_per_year"]["value"] * engine_params["plague"]["mortality"]["value"]
    return 1.0 - loss / engine_params["population"]["growth_per_year"]["value"]


def calibration(cfg, obs, top, masks, blocks, base_dir, rep):
    """The alternative to fitting densities (plan §11a, method (А)): the densities stay, and each
    unit's class is chosen so that its land holds its documented ceiling. Lands whose ceiling
    is only «по памяти» take the §4 class instead (rule 1). Measured on blocks for each arable
    density of `[calibration]`: the scenarios' provinces are calibrated with the same function
    at their layouts."""
    with (base_dir / "blocks.csv").open(encoding="utf-8") as f:
        rows = {r["id"]: r for r in csv.DictReader(f)}
    rugged = np.array([float(rows[b]["rugged_km2"]) / max(float(rows[b]["raster_km2"]), 1e-9) for b in blocks["id"]])
    p = rules.Params.from_toml(tomllib.loads((HERE / "params.toml").read_text(encoding="utf-8")))
    # Read only: the engine's constants of growth and plague, for the fill they balance at.
    balance = plague_balance_fill(tomllib.loads((HERE.parent.parent / "engine2" / "params.toml").read_text(encoding="utf-8")))
    area, bw, dry, cold = blocks["area"], blocks["bw"], blocks["dry"], blocks["cold"]
    wet = [o for o in obs if o["region"] in cfg["scan"]["irrigated_regions"]]
    lands = cfg.get("lands", {})

    def region_of(land):
        return lands[land]["region"] if land in lands and "region" in lands[land] else top[land]["region"]

    def opened(land):
        return land in top and top[land].get("opened", True)

    rep("# Альтернатива (А): плотности без изменений, класс — по потолку земли")
    rep()
    rep("Desert — блоки с BW ≥ ½. Из остальных первыми становятся Hills самые изрезанные (шаг 1: перепад "
        "≥ 300 м в окне 5); если земля реже холмов везде — все Hills, и первыми становятся Steppe самые сухие. "
        "Единица меняет класс, пока это приближает ёмкость к потолку. Плотности — E0 (Hills 12, Steppe 1, "
        "Desert 0,5), пашня — по каждому варианту ниже. Земля без открытого источника потолка — по правилу §4 "
        "(правило 1). Здесь единицы — блоки; на раскладке сценария — его провинции.")
    rep()
    rep(f"Заполненность, при которой рост движка равен средней потере от чумы (engine2/params.toml: "
        f"1 − вспышки × смертность / рост): {balance:.2f}. Земля выше неё в мире с чумой убывает.")
    rep()
    for arable in cfg["calibration"]["arable"]:
        dens = np.zeros(5)
        for t in ("Arable", "Hills", "Steppe", "Desert"):
            dens[TYPES.index(t)] = E0[t][0]
        dens[A] = arable
        dens[I] = wet[0]["y"] if wet else 0.0
        sparse = sparse_lands(obs, top, dens, cfg["calibration"]["sparse_below"])
        rep(f"## Каждая земля отдельно — пашня {arable:g}")
        rep()
        rep("| земля (наблюдение) | источник, чел./км² | способ | достигнуто | невязка | Hills по рельефу | Hills | Steppe | Desert | Arable |")
        rep("|---|---|---|---|---|---|---|---|---|---|")
        for land, o in sorted(top.items(), key=lambda kv: -kv[1]["y"]):
            if o["region"] in cfg["scan"]["irrigated_regions"]:
                continue
            m = masks[o["region"]]
            if opened(land):
                desert = (bw[m] >= 0.5) & (not lands.get(land, {}).get("river"))
                cls = calibrate(area[m], rugged[m], dry[m], desert, o["y"] * area[m].sum(), dens,
                                dry_steppe=p.steppe_share, cold=cold[m] if land in sparse else None)
                how = "калибровка" + ("" if o["y"] >= dens[H] else " (ниже холмов: потолок, редкая земля)" if land in sparse
                                      else " (ниже холмов: источник — нижняя граница)")
            else:
                cls = rule_class(rugged[m], bw[m], dry[m], p, cap_hills=True)
                how = "§4 (правило 1)"
            got = float(area[m] @ dens[cls]) / area[m].sum()
            rep(f"| {land} ({o['id']}) | {o['lo']:.1f}–{o['hi']:.1f} | {how} | {got:.1f} | {100 * (got - o['y']) / o['y']:+.0f} % | "
                f"{100 * area[m][rugged[m] >= p.hills_share].sum() / area[m].sum():.0f} % | {shares_row(area[m], cls)} |")
        rep()

        for name, sc in cfg.get("scenarios", {}).items():
            # Inner lands first: an outer land meets its own ceiling with what the inner ones left.
            order = sorted(sc["lands"], key=lambda land: (-land_depth(land, lands), sc["lands"].index(land)))
            cls, _ = calibrate_lands(order, {land: masks[region_of(land)] for land in order},
                                  {land: top[land]["y"] if opened(land) else None for land in order},
                                  area, rugged, dry, bw, dens, p, sparse=sparse, cold=cold,
                                  river={land for land in order if lands.get(land, {}).get("river")})
            start = sc.get("start_population", {})
            rep(f"## Сценарий {name} — пашня {arable:g}")
            rep()
            rep("| земля | внутри | источник, чел./км² | способ | площадь, км² | ёмкость, млн | достигнуто | невязка | "
                f"население на старте, млн | заполненность (равновесие {balance:.2f}) | Hills | Steppe | Desert | Arable |")
            rep("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
            for land in sc["lands"]:
                m = masks[region_of(land)]
                cap = float(area[m] @ dens[cls[m]])
                got = cap / area[m].sum()
                within = lands.get(land, {}).get("within", "—")
                if opened(land):
                    o = top[land]
                    src, how, res = f"{o['lo']:.1f}–{o['hi']:.1f}", "калибровка", f"{100 * (got - o['y']) / o['y']:+.0f} %"
                else:
                    src, how, res = "нет открытого", "§4 (правило 1)", "—"
                if land in start:
                    fill = start[land] * 1e6 / cap
                    pop, fl = f"{start[land]:.2f}", f"{fill:.2f}" + (" — выше" if fill > balance else "")
                else:
                    pop, fl = "—", "—"
                rep(f"| {land} | {within} | {src} | {how} | {area[m].sum():,.0f} | {cap / 1e6:.2f} | {got:.1f} | {res} | "
                    f"{pop} | {fl} | {shares_row(area[m], cls[m])} |")
            rep()
            if start:
                rep(f"Население на старте: {sc['start_population_source']}")
                rep()


if __name__ == "__main__":
    main()
