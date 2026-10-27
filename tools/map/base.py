"""Build the common base of the map (plan docs/engine2_map.md §3, §8 step 1): blocks — modern
second-level administrative units cut to the shoreline — and everything the rules of §4 sum
over them: area, rugged and dry land, common borders, coasts by sea area, major rivers.

    python -I -X utf8 tools/map/base.py [--cache D:/map-cache] [--bbox LON0 LAT0 LON1 LAT1]

Reads <cache>/src (fetch.py), writes <cache>/out/base: blocks.csv, borders.csv, coasts.csv,
zones.csv and report.txt — byte for byte the same for the same input — and blocks.gpkg, the
geometry for previews and the scenario layouts.
"""

import argparse
import collections
import hashlib
import pathlib
import sys
import time
import tomllib

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import geopandas as gpd  # noqa: E402
import numpy as np  # noqa: E402
import pyogrio  # noqa: E402
import rasterio  # noqa: E402
import rasterio.features  # noqa: E402
import rasterio.transform  # noqa: E402
import rasterio.windows  # noqa: E402
import shapely  # noqa: E402

import fetch  # noqa: E402
import geo  # noqa: E402
import rules  # noqa: E402

POLYGON_TYPES = (3, 6)  # shapely type ids of Polygon and MultiPolygon
KOPPEN_BW = (4, 5)      # Beck et al. 2018 legend: BWh, BWk
KOPPEN_BS = (6, 7)      # BSh, BSk
# Subarctic and polar: Dsc Dsd Dwc Dwd Dfc Dfd ET EF — the order in which a land below Hills
# turns sparse (plan §15a п. 4: the cold provinces first).
KOPPEN_COLD = (19, 20, 23, 24, 27, 28, 29, 30)


class Report:
    def __init__(self):
        self.lines: list[str] = []
        self.t0 = time.time()

    def __call__(self, line: str = "") -> None:
        self.lines.append(line)
        print(line, flush=True)

    def step(self, what: str) -> None:
        print(f"[{time.time() - self.t0:7.1f} s] {what}", flush=True)


def vsizip(src: pathlib.Path, file: str, inner: str) -> str:
    return f"/vsizip/{(src / file).as_posix()}/{inner}"


def polygonal(g):
    """The polygonal part of a geometry, or None."""
    if g is None or g.is_empty:
        return None
    parts = shapely.get_parts(g)
    polys = [x for x in parts if shapely.get_type_id(x) in POLYGON_TYPES]
    if not polys:
        return None
    out = shapely.union_all(polys) if len(polys) > 1 else polys[0]
    return None if out.is_empty else out


def read_cgaz(src, sources, bbox) -> gpd.GeoDataFrame:
    s = sources["cgaz_adm2"]
    df = pyogrio.read_dataframe(vsizip(src, s["file"], "geoBoundariesCGAZ_ADM2.shp"), bbox=tuple(bbox))
    df = df.rename(columns={"shapeID": "id", "shapeGroup": "country", "shapeName": "name"})
    df["geometry"] = shapely.make_valid(df.geometry.values)
    return df[["id", "country", "name", "geometry"]]


def refine(cg: gpd.GeoDataFrame, src, sources, refine_with: dict, tol: float, rep: Report):
    """Replace a country's CGAZ units with its own finer release, cut to the CGAZ outline of
    the country (so the borders with the neighbours stay CGAZ's). Pieces of the outline the
    finer units miss go to the unit they touch most."""
    out = [cg[~cg.country.isin(refine_with)]]
    for iso in sorted(refine_with):
        if not (cg.country == iso).any():
            continue
        s = sources[refine_with[iso]]
        fine = pyogrio.read_dataframe(src / s["file"])
        outline = shapely.union_all(cg.loc[cg.country == iso, "geometry"].values)
        geoms = shapely.intersection(shapely.make_valid(fine.geometry.values), outline)
        geoms = np.array([polygonal(g) for g in geoms], dtype=object)
        keep = np.array([g is not None for g in geoms])
        fine, geoms = fine[keep].reset_index(drop=True), geoms[keep]
        gaps = shapely.get_parts(shapely.difference(outline, shapely.union_all(geoms)))
        gaps = np.array([g for g in gaps if shapely.get_type_id(g) == 3 and g.area > 0], dtype=object)
        tree = shapely.STRtree(geoms)
        add = collections.defaultdict(list)
        apart, apart_area = 0, 0.0
        for g in gaps:
            near = tree.query(shapely.buffer(g, tol), predicate="intersects")
            if len(near) == 0:
                near = tree.query_nearest(g, all_matches=False)
                apart += 1
                apart_area += g.area
            contact = shapely.area(shapely.intersection(shapely.buffer(g, tol), geoms[near]))
            add[int(near[np.argmax(contact)])].append(g)
        for k, extra in add.items():
            geoms[k] = shapely.union_all([geoms[k], *extra])
        gap_area = float(sum(shapely.area(gaps))) if len(gaps) else 0.0
        rep(f"- {iso}: {s['file']} — {len(fine)} единиц вместо {int((cg.country == iso).sum())} CGAZ; "
            f"кусков контура страны вне них: {len(gaps)} ({gap_area:.4f} °²), присоединены к касающейся единице; "
            f"из них не касаются ни одной (к ближайшей): {apart} ({apart_area:.4f} °²)")
        out.append(gpd.GeoDataFrame({
            "id": fine["shapeID"].values, "country": iso, "name": fine["shapeName"].values,
            "geometry": geoms}, crs=cg.crs))
    return gpd.GeoDataFrame(pd_concat(out), crs=cg.crs)


def pd_concat(frames):
    import pandas as pd
    return pd.concat(frames, ignore_index=True)


def cut_to_land(df, land_parts, lakes, rep: Report):
    """Blocks = units ∩ land − natural lakes."""
    tree = shapely.STRtree(land_parts)
    shapely.prepare(land_parts)
    lake_tree = shapely.STRtree(lakes) if len(lakes) else None
    out = np.empty(len(df), dtype=object)
    inside = 0
    for k, g in enumerate(df.geometry.values):
        # Sorted: a union depends on the order of its inputs, and the tree's order depends on
        # everything else it holds — that is, on the bbox.
        near = np.sort(tree.query(g, predicate="intersects"))
        if len(near) == 1 and land_parts[near[0]].contains(g):
            cut = g
            inside += 1
        elif len(near) == 0:
            cut = None
        else:
            cut = shapely.intersection(g, shapely.union_all(land_parts[near]) if len(near) > 1 else land_parts[near[0]])
        if cut is not None and lake_tree is not None:
            wet = np.sort(lake_tree.query(cut, predicate="intersects"))
            if len(wet):
                cut = shapely.difference(cut, shapely.union_all(lakes[wet]))
        out[k] = polygonal(cut)
    rep.step(f"cut to land: {inside} of {len(df)} units wholly on land")
    return out


def in_bbox(g, bbox):
    """The unit as the base takes it. A unit whose point on the surface lies in the bbox is
    taken whole, one whose point lies outside is left out — unless it has several parts and
    some of them lie inside: then those parts are taken. CGAZ's Primorsky District reaches from
    the White Sea to Franz Josef Land, and its point falls on the islands (plan §15)."""
    x0, y0, x1, y1 = bbox
    parts = shapely.get_parts(g)
    pt = shapely.point_on_surface(parts)
    inside = (shapely.get_x(pt) >= x0) & (shapely.get_x(pt) <= x1) & (shapely.get_y(pt) >= y0) & (shapely.get_y(pt) <= y1)
    if len(parts) == 1 or inside.all() or not inside.any():
        p = shapely.point_on_surface(g)
        return g if x0 <= p.x <= x1 and y0 <= p.y <= y1 else None
    keep = parts[inside]
    return keep[0] if len(keep) == 1 else shapely.multipolygons(keep)


def tiles_of(geoms, bbox, step: float):
    """Pieces of `geoms` cut by the cells of a fixed `step`-degree grid (aligned to multiples
    of the step, not to the bbox) that meet the bbox; exact intersection, so the pieces are
    valid polygons."""
    x0, y0, x1, y1 = bbox
    out = []
    for g in geoms:
        gx0, gy0, gx1, gy1 = g.bounds
        for iy in range(int(np.floor(max(y0, gy0) / step)), int(np.ceil(min(y1, gy1) / step))):
            for ix in range(int(np.floor(max(x0, gx0) / step)), int(np.ceil(min(x1, gx1) / step))):
                c = shapely.intersection(g, shapely.box(ix * step, iy * step, (ix + 1) * step, (iy + 1) * step))
                out += [p for p in shapely.get_parts(c) if shapely.get_type_id(p) == 3 and p.area > 0]
    return np.array(out, dtype=object)


def source_holes(cover, cover_ids, land_parts, lakes, bbox, cell: float, tile: float):
    """Land no unit of the source covers, found cell by cell on a fixed grid of `cell` degrees.
    A cell sees only the units, land and lakes that touch it, each in a fixed order (units by
    id), so a hole does not depend on what lies far from it: widening the bbox leaves the holes
    inside the old one as they were (plan §15a п. 2). The pieces of one hole cut by the cell
    edges are joined back."""
    x0, y0, x1, y1 = bbox
    rank = np.empty(len(cover_ids), dtype=np.int64)
    rank[np.argsort(cover_ids, kind="stable")] = np.arange(len(cover_ids))
    land = tiles_of(land_parts, bbox, tile)
    utree, ltree = shapely.STRtree(cover), shapely.STRtree(land)
    ktree = shapely.STRtree(lakes) if len(lakes) else None
    pieces = []
    for iy in range(int(np.floor(y0 / cell)), int(np.ceil(y1 / cell))):
        for ix in range(int(np.floor(x0 / cell)), int(np.ceil(x1 / cell))):
            c = shapely.box(max(ix * cell, x0), max(iy * cell, y0), min((ix + 1) * cell, x1), min((iy + 1) * cell, y1))
            li = np.sort(ltree.query(c, predicate="intersects"))
            if not len(li):
                continue
            g = shapely.union_all(shapely.intersection(land[li], c))
            if ktree is not None:
                ki = np.sort(ktree.query(c, predicate="intersects"))
                if len(ki):
                    g = shapely.difference(g, shapely.union_all(lakes[ki]))
            ui = utree.query(c, predicate="intersects")
            if len(ui):
                ui = ui[np.argsort(rank[ui])]
                g = shapely.difference(g, shapely.union_all(shapely.intersection(cover[ui], c)))
            pieces += [p for p in shapely.get_parts(g) if shapely.get_type_id(p) == 3 and p.area > 0]
    if not pieces:
        return np.array([], dtype=object)
    pieces = np.array(pieces, dtype=object)
    # Join the pieces of one hole: pieces that touch, in the order the cells gave them.
    a, b = shapely.STRtree(pieces).query(pieces, predicate="dwithin", distance=1e-9)
    root = np.arange(len(pieces))

    def find(i):
        while root[i] != i:
            root[i] = root[root[i]]
            i = root[i]
        return i

    for i, j in zip(a, b):
        ri, rj = find(i), find(j)
        if ri != rj:
            root[max(ri, rj)] = min(ri, rj)
    groups = collections.defaultdict(list)
    for i in range(len(pieces)):
        groups[find(i)].append(i)
    holes = []
    for r in sorted(groups):
        g = pieces[groups[r]]
        u = g[0] if len(g) == 1 else shapely.union_all(g)
        holes += [p for p in shapely.get_parts(u) if shapely.get_type_id(p) == 3 and p.area > 0]
    return np.array(holes, dtype=object)


def fill_holes(df, holes, tol, area_crs, min_block_km2, rep: Report):
    """Land no unit of the source covers (`source_holes`). CGAZ drops some city units — Feodosia
    (Kaffa), Annaba (Hippo), Benghazi — and some islands: a piece of at least `min_block_km2`
    becomes a block of its own (country of the block it touches most, or of the nearest), a
    smaller one joins the block it touches most; a small piece touching no block stays out."""
    blocks = df.geometry.values
    km2 = gpd.GeoSeries(holes, crs="EPSG:4326").to_crs(area_crs).area.values / 1e6 if len(holes) else np.zeros(0)
    tree = shapely.STRtree(blocks)
    add = collections.defaultdict(list)
    joined, own = [], []
    for k, h in enumerate(holes):
        hb = shapely.buffer(h, tol)
        near = np.sort(tree.query(hb, predicate="intersects"))
        if km2[k] >= min_block_km2:
            if len(near) == 0:
                near = np.sort(tree.query_nearest(h, all_matches=True))[:1]
            to = int(near[np.argmax(shapely.area(shapely.intersection(hb, blocks[near])))]) if len(near) > 1 else int(near[0])
            pt = h.representative_point()
            own.append({"id": f"hole_{pt.x:+08.3f}_{pt.y:+07.3f}", "country": df.country.values[to],
                        "name": f"вне единиц источника у {pt.x:.2f}, {pt.y:.2f}", "geometry": h})
            continue
        if len(near) == 0:
            continue
        to = int(near[np.argmax(shapely.area(shapely.intersection(hb, blocks[near])))])
        add[to].append(h)
        joined.append(km2[k])
    out = df.copy()
    geoms = blocks.copy()
    for k, extra in add.items():
        geoms[k] = polygonal(shapely.union_all([blocks[k], *extra]))
    out["geometry"] = geoms
    rep(f"Суша вне единиц источника: {len(holes)} кусков, {km2.sum():,.0f} км². Куски от {min_block_km2:,.0f} км² — "
        f"свои блоки: {len(own)}, {sum(km2[km2 >= min_block_km2]):,.0f} км²"
        + (" (" + "; ".join(f"{o['country']} {o['name'].split(' у ')[1]}" for o in own) + ")" if own else "")
        + f". Меньшие присоединены к касающемуся блоку: {len(joined)}, {sum(joined):,.0f} км².")
    if own:
        out = gpd.GeoDataFrame(pd_concat([out, gpd.GeoDataFrame(own, crs=df.crs)]), crs=df.crs)
    return out.sort_values("id").reset_index(drop=True)


def sea_zones(src, sources, bbox, land_parts, inland: dict):
    """IHO sea areas near the bbox, plus inland seas IHO does not name: the hole in the land
    that contains the given point (params [sea.inland])."""
    s = sources["iho_sea_areas"]
    z = pyogrio.read_dataframe(src / s["file"])
    x0, y0, x1, y1 = bbox
    z = z[shapely.intersects(z.geometry.values, shapely.box(x0 - 2, y0 - 2, x1 + 2, y1 + 2))]
    names = list(z["name"].values)
    geoms = list(shapely.make_valid(z.geometry.values))
    for name in sorted(inland):
        pt = shapely.Point(inland[name])
        holes = [shapely.Polygon(r) for p in land_parts for r in getattr(p, "interiors", [])]
        hit = [h for h in holes if h.contains(pt)]
        if len(hit) != 1:
            sys.exit(f"inland sea {name}: {len(hit)} land holes contain {inland[name]}")
        names.append(name)
        geoms.append(hit[0])
    order = sorted(range(len(names)), key=lambda k: names[k])
    names = [names[k] for k in order]
    geoms = [geoms[k] for k in order]
    pieces, piece_zone = geo.tile(geoms, np.arange(len(names)), (x0 - 2, y0 - 2, x1 + 2, y1 + 2), 1.0)
    return names, pieces, piece_zone


def apply_cuts(lon, lat, zone_names: list[str], zone, cuts):
    """Name of each coast sample's sea area after the cuts of params [[sea.cut]]."""
    names = np.array(zone_names, dtype=object)[zone]
    for c in cuts:
        on = names == c["zone"]
        if "lon" in c:
            names[on] = np.where(lon[on] < c["lon"], c["zone"] + " W", c["zone"] + " E")
        else:
            names[on] = np.where(lat[on] < c["lat"], c["zone"] + " S", c["zone"] + " N")
    return names


def read_window(path: str, bounds):
    """A north-up window of a lon/lat raster covering `bounds`, and its transform."""
    with rasterio.open(path) as r:
        w = rasterio.windows.from_bounds(*bounds, transform=r.transform).round_offsets().round_lengths()
        w = w.intersection(rasterio.windows.Window(0, 0, r.width, r.height))
        data = r.read(1, window=w)
        transform = r.window_transform(w)
        nodata = r.nodata
    if transform.e > 0:  # bottom-up grid: flip to north-up
        data = data[::-1]
        transform = rasterio.Affine(transform.a, transform.b, transform.c, transform.d, -transform.e,
                                    transform.f + transform.e * data.shape[0])
    return data, transform, nodata


def label_blocks(blocks, transform, shape):
    shapes = ((g, k + 1) for k, g in enumerate(blocks))
    return rasterio.features.rasterize(shapes, out_shape=shape, transform=transform, fill=0, dtype="int32")


def chunked_sums(labels, row_area, values, masks: dict, n: int, transform, rows: int = 512):
    """Zonal sums of cell area under each mask (a function of a chunk of `values`), per
    block, in row chunks: a full-size float grid per mask would not fit. Chunks start at rows
    of the global grid counted from 90° N, not at the top of the window, so a block's sum adds
    its cells in the same groups whatever the bbox (plan §15a п. 2)."""
    out = {k: np.zeros(n) for k in masks}
    row0 = int(round((90.0 - transform.f) / abs(transform.e)))
    starts = [0] + list(range(rows - row0 % rows, labels.shape[0], rows))
    for r0, r1 in zip(starts, starts[1:] + [labels.shape[0]]):
        lab = labels[r0:r1]
        area = np.broadcast_to(row_area[r0:r1, None], lab.shape)
        chunk = values[r0:r1]
        for k, fn in masks.items():
            out[k] += geo.zonal_sums(lab, area * fn(chunk), n)
    return out


def point_values(raster, transform, lon, lat):
    rows, cols = rasterio.transform.rowcol(transform, lon, lat)
    rows = np.clip(np.asarray(rows), 0, raster.shape[0] - 1)
    cols = np.clip(np.asarray(cols), 0, raster.shape[1] - 1)
    return raster[rows, cols]


def geom_sha(g) -> str:
    return hashlib.sha256(shapely.to_wkb(shapely.normalize(g), output_dimension=2, byte_order=1)).hexdigest()[:16]


def write_csv(path: pathlib.Path, header: list[str], rows) -> None:
    with path.open("w", encoding="utf-8", newline="\n") as f:
        f.write(",".join(header) + "\n")
        for r in rows:
            f.write(",".join(_cell(x) for x in r) + "\n")


def _cell(x) -> str:
    if isinstance(x, float):
        return f"{x:.3f}"
    s = "" if x is None else str(x)
    return '"' + s.replace('"', '""') + '"' if any(c in s for c in ',"\n') else s


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", default="D:/map-cache")
    ap.add_argument("--bbox", type=float, nargs=4)
    ap.add_argument("--out")
    args = ap.parse_args()
    cache = pathlib.Path(args.cache)
    src = cache / "src"
    out = pathlib.Path(args.out) if args.out else cache / "out" / "base"
    out.mkdir(parents=True, exist_ok=True)
    params = tomllib.loads((HERE / "params.toml").read_text(encoding="utf-8"))
    sources = tomllib.loads((HERE / "sources.toml").read_text(encoding="utf-8"))
    pb = params["base"]
    bbox = args.bbox or pb["bbox"]
    tol = pb["border_tol_deg"]
    rep = Report()

    rep("# Основа карты (E1b, шаг 1) — замер")
    rep()
    rep("## Входы")
    for name, s in sources.items():
        path = src / s["file"]
        digest = fetch.sha256_of(path)
        if digest != s["sha256"]:
            sys.exit(f"{name}: {path} differs from sources.toml — run fetch.py")
        rep(f"- {name}: {s['file']} sha256 {digest[:16]}… ({s['license']})")
    rep()
    rep("## Параметры (tools/map/params.toml)")
    for sec, vals in params.items():
        rep(f"- [{sec}] " + "; ".join(f"{k} = {v}" for k, v in vals.items()))
    rep(f"- bbox этого прогона: {bbox}")
    rep()

    rep.step("reading CGAZ")
    cg = read_cgaz(src, sources, bbox)
    rep("## Блоки")
    refine_with = pb.get("refine", {})
    if refine_with:
        rep("Замена грубых единиц CGAZ ([base.refine]):")
        cg = refine(cg, src, sources, refine_with, tol, rep)
    cover, cover_ids = cg.geometry.values, cg.id.values  # every unit read: land under any of them is no hole
    taken = np.array([in_bbox(g, bbox) for g in cg.geometry.values], dtype=object)
    split = sum(1 for g, t in zip(cg.geometry.values, taken) if t is not None and t is not g)
    cg = cg.assign(geometry=taken)[[t is not None for t in taken]].sort_values("id").reset_index(drop=True)
    rep(f"Единиц за краем bbox, взятых частями внутри: {split}")
    rep.step(f"{len(cg)} units in the base")

    rep.step("reading land and lakes")
    ext = shapely.total_bounds(cg.geometry.values)
    ne_land = sources["ne_land"]
    land = pyogrio.read_dataframe(vsizip(src, ne_land["file"], "ne_10m_land.shp"), bbox=tuple(ext))
    land_parts = shapely.get_parts(land.geometry.values)
    ne_lakes = sources["ne_lakes"]
    lakes = pyogrio.read_dataframe(vsizip(src, ne_lakes["file"], "ne_10m_lakes.shp"), bbox=tuple(ext))
    natural = lakes.featurecla.isin(pb["lake_classes"])
    lake_geoms = shapely.make_valid(lakes.loc[natural].geometry.values)
    rep(f"Суша — Natural Earth 10m land; вычтены естественные озёра ({int(natural.sum())}: "
        f"{', '.join(pb['lake_classes'])}), водохранилища ({int((~natural).sum())}) — нет: их не было.")

    geoms = cut_to_land(cg, land_parts, lake_geoms, rep)
    dropped = cg[[g is None for g in geoms]]
    cg = cg.assign(geometry=geoms)[[g is not None for g in geoms]].reset_index(drop=True)
    rep.step("filling holes of the source")
    holes = source_holes(cover, cover_ids, land_parts, lake_geoms, bbox, pb["hole_cell_deg"], pb["hole_tile_deg"])
    cg = fill_holes(cg, holes, tol, pb["area_crs"], pb["hole_block_km2"], rep)
    land_ext = shapely.difference(shapely.intersection(shapely.union_all(land_parts), shapely.box(*bbox)),
                                  shapely.union_all(lake_geoms))
    cg["geometry"] = shapely.normalize(cg.geometry.values)
    blocks = cg.geometry.values
    n = len(cg)
    rep(f"Блоков: {n}; без суши после обрезки (выброшены): {len(dropped)}"
        + (f" — {', '.join(sorted(dropped.country.unique()))}" if len(dropped) else ""))

    gs = gpd.GeoSeries(blocks, crs="EPSG:4326")
    area = gs.to_crs(pb["area_crs"]).area.values / 1e6
    rep_pt = shapely.point_on_surface(blocks)
    lon, lat = shapely.get_x(rep_pt), shapely.get_y(rep_pt)

    by_country = collections.defaultdict(list)
    for c, a in zip(cg.country.values, area):
        by_country[c].append(a)
    rep()
    rep("| страна | блоков | площадь, км² | медиана блока, км² |")
    rep("|---|---|---|---|")
    for c in sorted(by_country, key=lambda c: -sum(by_country[c])):
        a = np.array(by_country[c])
        rep(f"| {c} | {len(a)} | {a.sum():,.0f} | {np.median(a):,.0f} |")
    rep()

    # Land coverage: blocks against the land of the bbox.
    box = shapely.box(*bbox)
    land_in = land_ext
    land_in_area = gpd.GeoSeries([land_in], crs="EPSG:4326").to_crs(pb["area_crs"]).area.values[0] / 1e6
    blocks_in_area = gpd.GeoSeries(shapely.intersection(blocks, box), crs="EPSG:4326").to_crs(pb["area_crs"]).area.values.sum() / 1e6
    rep(f"Покрытие суши в bbox: блоки {blocks_in_area:,.0f} км² из {land_in_area:,.0f} км² суши без озёр "
        f"({100 * blocks_in_area / land_in_area:.2f} %)")
    holes = shapely.get_parts(shapely.difference(land_in, shapely.union_all(blocks)))
    holes = holes[shapely.get_type_id(holes) == 3]
    hole_km2 = gpd.GeoSeries(holes, crs="EPSG:4326").to_crs(pb["area_crs"]).area.values / 1e6
    inner_box = shapely.box(bbox[0] + 1, bbox[1] + 1, bbox[2] - 1, bbox[3] - 1)
    inner = shapely.within(holes, inner_box)
    rep(f"Непокрытая суша: у края основы (ближе 1° к краю bbox; единицы с точкой вне bbox) "
        f"{hole_km2[~inner].sum():,.0f} км²; внутри — {hole_km2[inner].sum():,.0f} км² в {int(inner.sum())} кусках"
        + (", крупнейшие: " + "; ".join(
            f"{hole_km2[k]:,.0f} км² у {holes[k].representative_point().x:.2f}, {holes[k].representative_point().y:.2f}"
            for k in np.argsort(-np.where(inner, hole_km2, 0))[:8] if inner[k]) if inner.any() else ""))

    rep.step("borders")
    bi, bj, bkm, rest = geo.borders(blocks, tol)
    ov = shapely.intersection(blocks[bi], blocks[bj])
    overlap = gpd.GeoSeries(ov, crs="EPSG:4326").to_crs(pb["area_crs"]).area.values.sum() / 1e6
    l_min = params["neighbours"]["l_min_km"]
    rep(f"Общих границ: {len(bi)} пар; короче L_min = {l_min} км (касание углом и обрывки): "
        f"{int((bkm < l_min).sum())}; перекрытие блоков: {overlap:,.1f} км²")

    rep.step("coasts")
    cuts = params.get("sea", {}).get("cut", [])
    rep(f"Морские зоны: IHO Sea Areas v3; внутренние моря, которых нет в IHO, — дыра суши Natural Earth "
        f"([sea.inland]): {', '.join(sorted(params.get('sea', {}).get('inland', {}))) or 'нет'}; "
        f"разрезы крупных зон ([[sea.cut]]): {len(cuts) or 'нет'}")
    sea = params.get("sea", {})
    zone_names, pieces, piece_zone = sea_zones(src, sources, bbox, land_parts, sea.get("inland", {}))
    slon, slat, skm, sown = geo.sample_lines(rest, pb["sample_km"])
    szone = geo.nearest_zone(slon, slat, pieces, piece_zone, params["coast"]["snap_deg"])
    on = szone >= 0
    znames = apply_cuts(slon[on], slat[on], zone_names, szone[on], cuts)
    coast = collections.defaultdict(float)
    for b, z, km in zip(sown[on], znames, skm[on]):
        coast[(int(b), z)] += float(km)
    zone_km = collections.defaultdict(float)
    zone_blocks = collections.defaultdict(set)
    for (b, z), km in coast.items():
        zone_km[z] += km
        zone_blocks[z].add(b)
    rep(f"Внешняя граница блоков (не общая): {skm.sum():,.0f} км; из неё берег морской зоны IHO: "
        f"{skm[on].sum():,.0f} км; остальное (озёра, край основы, щели источника): {skm[~on].sum():,.0f} км")
    rep()
    rep("| морская зона | берег, км | блоков |")
    rep("|---|---|---|")
    for z in sorted(zone_km, key=lambda z: -zone_km[z]):
        rep(f"| {z} | {zone_km[z]:,.0f} | {len(zone_blocks[z])} |")
    rep()

    rep.step("relief")
    bounds = tuple(shapely.total_bounds(blocks))
    pr = params["land"]
    etopo = sources["etopo_2022_60s"]
    elev, et, _ = read_window(str(src / etopo["file"]), bounds)
    elev = elev.astype(np.float32)
    landmask = rasterio.features.rasterize(((g, 1) for g in land_parts), out_shape=elev.shape, transform=et,
                                           fill=0, dtype="uint8")
    elev[landmask == 0] = 0.0
    rugged = geo.local_relief(elev, pr["relief_window_cells"]) >= pr["relief_m"]
    labels = label_blocks(blocks, et, elev.shape)
    ra = geo.row_cell_area_km2(et, elev.shape[0])
    s = chunked_sums(labels, ra, rugged, {"all": lambda c: 1.0, "rugged": lambda c: c}, n, et)
    raster_km2, rugged_km2 = s["all"], s["rugged"]
    tiny = raster_km2 == 0
    if tiny.any():
        flag = point_values(rugged, et, lon[tiny], lat[tiny])
        raster_km2[tiny] = area[tiny]
        rugged_km2[tiny] = area[tiny] * flag
    rep(f"Рельеф: ETOPO 2022 1′, окно {pr['relief_window_cells']}×{pr['relief_window_cells']} клеток, "
        f"порог {pr['relief_m']} м; блоков меньше клетки (значение по точке): {int(tiny.sum())}")
    del elev, landmask, rugged, labels

    rep.step("climate")
    kop = sources["koppen_beck_2018"]
    cls, kt, _ = read_window(vsizip(src, kop["file"], pb["koppen_raster"]), bounds)
    labels = label_blocks(blocks, kt, cls.shape)
    ka = geo.row_cell_area_km2(kt, cls.shape[0])
    s = chunked_sums(labels, ka, cls, {
        "climate": lambda c: c > 0,
        "bw": lambda c: np.isin(c, KOPPEN_BW),
        "bs": lambda c: np.isin(c, KOPPEN_BS),
        "cold": lambda c: np.isin(c, KOPPEN_COLD)}, n, kt)
    climate_km2, bw_km2, bs_km2, cold_km2 = s["climate"], s["bw"], s["bs"], s["cold"]
    tiny = climate_km2 == 0
    if tiny.any():
        c = point_values(cls, kt, lon[tiny], lat[tiny])
        climate_km2[tiny] = np.where(c > 0, area[tiny], 0.0)
        bw_km2[tiny] = np.where(np.isin(c, KOPPEN_BW), area[tiny], 0.0)
        bs_km2[tiny] = np.where(np.isin(c, KOPPEN_BS), area[tiny], 0.0)
        cold_km2[tiny] = np.where(np.isin(c, KOPPEN_COLD), area[tiny], 0.0)
    rep(f"Климат: Beck et al. 2018, {pb['koppen_raster']}; блоков без клетки климата: {int((climate_km2 == 0).sum())}")
    del cls, labels

    rep.step("rivers")
    ne_riv = sources["ne_rivers"]
    riv = pyogrio.read_dataframe(vsizip(src, ne_riv["file"], "ne_10m_rivers_lake_centerlines.shp"), bbox=bounds)
    major = riv[riv.scalerank <= params["irrigated"]["river_scalerank_max"]]
    ri, bk = shapely.STRtree(blocks).query(major.geometry.values, predicate="intersects")
    order = np.lexsort((ri, bk))  # each block adds its rivers in one order, whatever the tree holds
    ri, bk = ri[order], bk[order]
    river_km = np.bincount(bk, weights=geo.lengths_km(shapely.intersection(major.geometry.values[ri], blocks[bk])),
                           minlength=n)
    rep(f"Большие реки (scalerank ≤ {params['irrigated']['river_scalerank_max']}): "
        f"{', '.join(sorted(set(major['name'].dropna())))}")
    rep()

    rep.step("writing")
    ids = cg.id.values
    write_csv(out / "blocks.csv",
              ["id", "country", "name", "lon", "lat", "area_km2", "raster_km2", "rugged_km2", "climate_km2",
               "bw_km2", "bs_km2", "cold_km2", "river_km", "geom_sha"],
              ([ids[k], cg.country.values[k], cg.name.values[k], f"{lon[k]:.4f}", f"{lat[k]:.4f}",
                float(area[k]), float(raster_km2[k]), float(rugged_km2[k]), float(climate_km2[k]),
                float(bw_km2[k]), float(bs_km2[k]), float(cold_km2[k]), float(river_km[k]), geom_sha(blocks[k])] for k in range(n)))
    border_rows = sorted((min(ids[i], ids[j]), max(ids[i], ids[j]), float(km)) for i, j, km in zip(bi, bj, bkm))
    write_csv(out / "borders.csv", ["a", "b", "km"], border_rows)
    coast_rows = sorted((ids[b], z, km) for (b, z), km in coast.items())
    write_csv(out / "coasts.csv", ["block", "zone", "km"], coast_rows)
    write_csv(out / "zones.csv", ["zone", "coast_km", "blocks"],
              sorted((z, zone_km[z], len(zone_blocks[z])) for z in zone_km))
    gpd.GeoDataFrame({"id": ids, "country": cg.country.values, "name": cg.name.values}, geometry=blocks,
                     crs="EPSG:4326").to_file(out / "blocks.gpkg", driver="GPKG", layer="blocks")

    # Each block as its own province: the rules of §4 at the finest grain.
    p = rules.Params.from_toml(params)
    bdata = {ids[k]: rules.Block(float(area[k]), float(raster_km2[k]), float(rugged_km2[k]), float(climate_km2[k]),
                                 float(bw_km2[k]), float(bs_km2[k]), float(river_km[k])) for k in range(n)}
    provs = rules.provinces({b: b for b in ids}, bdata, border_rows, coast_rows, p)
    rep("## Правила §4 на блоках (каждый блок — своя провинция)")
    lt = collections.Counter(v.land for v in provs.values())
    lt_area = collections.defaultdict(float)
    for b, v in provs.items():
        lt_area[v.land] += bdata[b].area_km2
    total = sum(lt_area.values())
    for land_name in ("Arable", "Hills", "Steppe", "Desert"):
        rep(f"- {land_name}: {lt[land_name]} блоков, {100 * lt_area[land_name] / total:.1f} % площади")
    rep()
    rep("| страна | Arable | Hills | Steppe | Desert | прибрежных | орошаемая долина |")
    rep("|---|---|---|---|---|---|---|")
    country_of = dict(zip(ids, cg.country.values))
    per = collections.defaultdict(collections.Counter)
    for b, v in provs.items():
        c = country_of[b]
        per[c][v.land] += bdata[b].area_km2
        per[c]["coastal"] += v.coastal
        per[c]["irrigated"] += v.irrigated
    for c in sorted(per, key=lambda c: -sum(per[c][x] for x in ("Arable", "Hills", "Steppe", "Desert"))):
        tot = sum(per[c][x] for x in ("Arable", "Hills", "Steppe", "Desert"))
        rep(f"| {c} | " + " | ".join(f"{100 * per[c][x] / tot:.0f} %" for x in ("Arable", "Hills", "Steppe", "Desert"))
            + f" | {per[c]['coastal']} | {per[c]['irrigated']} |")
    rep()
    # Connectivity of the base itself: any common border, any coast. L_min and the coast
    # minimum are rules for provinces; on blocks of a few km² (Cypriot villages, London
    # boroughs) they cut every border and would hide the real breaks.
    topo = rules.Params(0.0, 0.0, p.hills_share, p.desert_share, p.steppe_share, p.irrigated_min_river_km)
    comps = rules.components(rules.provinces({b: b for b in ids}, bdata, border_rows, coast_rows, topo))
    rep(f"Связность основы (любая общая граница, любой берег зоны): компонент {len(comps)}; "
        f"в главной {len(comps[0])} из {n}.")
    small = comps[1:]
    name_of = dict(zip(ids, cg.name.values))
    if small:
        rep("Вне главной компоненты (блок: страна, имя, площадь):")
        for c in small[:60]:
            rep("- " + "; ".join(f"{b}: {country_of[b]}, {name_of[b]}, {bdata[b].area_km2:,.0f} км²" for b in c))
        if len(small) > 60:
            rep(f"- … ещё {len(small) - 60}")
    (out / "report.txt").write_text("\n".join(rep.lines) + "\n", encoding="utf-8")
    rep.step("done")


if __name__ == "__main__":
    main()
