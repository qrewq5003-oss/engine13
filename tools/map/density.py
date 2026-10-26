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

TYPES = ["Irrigated", "Hills", "Desert", "Steppe", "Arable"]
I, H, D, S, A = range(5)
FITTED = [A, H, S]
E0 = {"Arable": (40.0, 26.0, 42.0), "Hills": (12.0, 8.0, 19.0), "Steppe": (1.0, 0.5, 2.0),
      "Desert": (0.5, 0.1, 1.0)}


def read_blocks(base_dir: pathlib.Path):
    with (base_dir / "blocks.csv").open(encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    gdf = gpd.read_file(base_dir / "blocks.gpkg", layer="blocks").set_index("id").loc[[r["id"] for r in rows]]
    climate = np.array([float(r["climate_km2"]) for r in rows])
    with np.errstate(invalid="ignore", divide="ignore"):
        bw = np.nan_to_num(np.array([float(r["bw_km2"]) for r in rows]) / climate)
        bs = np.nan_to_num(np.array([float(r["bs_km2"]) for r in rows]) / climate)
    return {
        "id": np.array([r["id"] for r in rows]), "country": np.array([r["country"] for r in rows]),
        "name": np.array([r["name"] for r in rows]),
        "lon": np.array([float(r["lon"]) for r in rows]), "lat": np.array([float(r["lat"]) for r in rows]),
        "area": np.array([float(r["area_km2"]) for r in rows]), "bw": bw, "dry": bw + bs,
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
        m |= k
    return m


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
    masks = {r: region_mask(blocks, v["parts"]) for r, v in cfg["regions"].items()}
    irrigated = np.any(np.stack([masks[r] for r in scan["irrigated_regions"]]), axis=0)
    members = np.any(np.stack(list(masks.values())), axis=0)
    obs = [o for o in cfg["obs"]]
    for o in obs:
        a = blocks["area"][masks[o["region"]]].sum()
        lo, hi = o["density"] if "density" in o else (o["population"][0] * 1e6 / a, o["population"][1] * 1e6 / a)
        o["lo"], o["hi"], o["y"], o["area"] = lo, hi, (lo + hi) / 2, a
    # The envelope: a land's capacity is the highest density it is documented to have held
    # before a plague (E0 П1: «ёмкость — свойство земли, не года»); of the observations of one
    # land, only the highest is kept.
    top = {}
    for o in obs:
        g = o.get("land", o["id"])
        if g not in top or o["y"] > top[g]["y"]:
            top[g] = o
    sets = {"все наблюдения": [o for o in obs if o["region"] not in scan["irrigated_regions"]],
            "огибающая (наибольшая плотность земли)": [o for o in top.values()
                                                      if o["region"] not in scan["irrigated_regions"]]}
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
    calibrated(top, masks, blocks, cache / "out" / "base", scan, rep)
    (out / "report.txt").write_text("\n".join(rep.lines) + "\n", encoding="utf-8")
    rep.step("done")


def calibrated(top, masks, blocks, base_dir, scan, rep):
    """The alternative to fitting densities: E0 densities stay, and the class of each block is
    chosen so that the region holds its documented ceiling. Desert blocks (BW ≥ ½) stay Desert;
    of the rest, the most rugged become Hills, or — below the Hills density — the driest
    become Steppe, until the region's capacity meets the source."""
    with (base_dir / "blocks.csv").open(encoding="utf-8") as f:
        rows = {r["id"]: r for r in csv.DictReader(f)}
    rugged = np.array([float(rows[b]["rugged_km2"]) / max(float(rows[b]["raster_km2"]), 1e-9) for b in blocks["id"]])
    d_a, d_h, d_s, d_d = (E0[t][0] for t in ("Arable", "Hills", "Steppe", "Desert"))
    rep("# Альтернатива: плотности E0 без изменений, класс провинции — по потолку региона")
    rep()
    rep("Блоки с BW ≥ ½ — Desert; из остальных самые изрезанные (шаг 1: перепад ≥ 300 м в окне 5) становятся "
        "Hills, а если нужна плотность ниже холмов — самые сухие становятся Steppe, пока ёмкость региона не "
        "сравняется с источником. Плотности — E0 (Arable 40, Hills 12, Steppe 1, Desert 0,5).")
    rep()
    rep("| земля (наблюдение) | источник, чел./км² | достигнуто | невязка | Hills по рельефу | Hills после | Steppe после | Desert |")
    rep("|---|---|---|---|---|---|---|---|")
    for o in sorted(top.values(), key=lambda o: -o["y"]):
        if o["region"] in scan["irrigated_regions"]:
            continue
        m = np.flatnonzero(masks[o["region"]])
        area = blocks["area"][m]
        total = area.sum()
        desert = blocks["bw"][m] >= 0.5
        cls = np.where(desert, D, A)
        need = o["y"] * total - d_d * area[desert].sum()
        rest = ~desert
        if need < d_h * area[rest].sum():  # below Hills everywhere: Hills, then the driest as Steppe
            cls[rest] = H
            for k in np.argsort(-blocks["dry"][m] - 1e-3 * rugged[m], kind="stable"):
                if not rest[k]:
                    continue
                cap = d_h * area[(cls == H)].sum() + d_s * area[(cls == S)].sum()
                if cap <= need:
                    break
                cls[k] = S
        else:  # Arable, then the most rugged as Hills
            for k in np.argsort(-rugged[m], kind="stable"):
                if not rest[k]:
                    continue
                cap = d_a * area[cls == A].sum() + d_h * area[cls == H].sum()
                if cap - (d_a - d_h) * area[k] < need:
                    break
                cls[k] = H
        cap = d_a * area[cls == A].sum() + d_h * area[cls == H].sum() + d_s * area[cls == S].sum() + d_d * area[desert].sum()
        got = cap / total
        terrain = area[rest & (rugged[m] >= 0.5)].sum() / total
        rep(f"| {o.get('land', o['id'])} ({o['id']}) | {o['lo']:.1f}–{o['hi']:.1f} | {got:.1f} | {100 * (got - o['y']) / o['y']:+.0f} % | "
            f"{100 * terrain:.0f} % | {100 * area[cls == H].sum() / total:.0f} % | {100 * area[cls == S].sum() / total:.0f} % | "
            f"{100 * area[desert].sum() / total:.0f} % |")
    rep()


if __name__ == "__main__":
    main()
