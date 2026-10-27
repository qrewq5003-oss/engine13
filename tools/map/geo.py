"""Geometry and raster helpers of the map pipeline. Coordinates are lon/lat (EPSG:4326);
lengths are on the WGS84 ellipsoid, in km."""

import numpy as np
import pyproj
import shapely

GEOD = pyproj.Geod(ellps="WGS84")
LINE_TYPES = (1, 2)  # shapely type ids of LineString and LinearRing


def _segments(geoms):
    """The straight segments of the linework of `geoms` (points dropped): start and end
    coordinates, the index of the owning geometry, the ellipsoidal length in km."""
    geoms = np.asarray(geoms, dtype=object)
    parts, owner = shapely.get_parts(geoms, return_index=True)
    keep = np.isin(shapely.get_type_id(parts), LINE_TYPES)
    parts, owner = parts[keep], owner[keep]
    xy, line = shapely.get_coordinates(parts, return_index=True)
    if len(xy) < 2:
        empty = np.zeros(0)
        return empty, empty, empty, empty, np.zeros(0, dtype=int), empty
    same = line[1:] == line[:-1]
    x0, y0 = xy[:-1, 0][same], xy[:-1, 1][same]
    x1, y1 = xy[1:, 0][same], xy[1:, 1][same]
    _, _, d = GEOD.inv(x0, y0, x1, y1)
    return x0, y0, x1, y1, owner[line[:-1][same]], d / 1000.0


def lengths_km(geoms) -> np.ndarray:
    """Ellipsoidal length of the linework of each geometry, km."""
    *_, own, d = _segments(geoms)
    return np.bincount(own, weights=d, minlength=len(geoms))


def sample_lines(geoms, spacing_km: float):
    """Points along the linework of `geoms`, about `spacing_km` apart, each carrying the length
    it stands for: (lon, lat, km, owner index)."""
    x0, y0, x1, y1, own, d = _segments(geoms)
    n = np.maximum(1, np.ceil(d / spacing_km)).astype(np.int64)
    seg = np.repeat(np.arange(len(n)), n)
    k = np.arange(n.sum()) - np.repeat(np.cumsum(n) - n, n)
    t = (k + 0.5) / n[seg]
    lon = x0[seg] + t * (x1[seg] - x0[seg])
    lat = y0[seg] + t * (y1[seg] - y0[seg])
    return lon, lat, (d / n)[seg], own[seg]


def borders(blocks, tol_deg: float):
    """Common borders of polygonal blocks. Returns (i, j, km) for every pair i < j whose
    boundaries run within `tol_deg` of each other, and each block's remaining boundary — the
    part along no other block (coast, lake shore, edge of the base)."""
    blocks = np.asarray(blocks, dtype=object)
    bounds = shapely.boundary(blocks)
    buffers = shapely.buffer(blocks, tol_deg, quad_segs=2)
    tree = shapely.STRtree(blocks)
    a, b = tree.query(buffers, predicate="intersects")
    other = a != b
    a, b = a[other], b[other]
    upper = a < b
    i, j = a[upper], b[upper]
    km = lengths_km(shapely.intersection(bounds[i], buffers[j]))
    order = np.lexsort((b, a))  # neighbours in index order: a union depends on the order of its inputs
    a, b = a[order], b[order]
    starts = np.searchsorted(a, np.arange(len(blocks)))
    ends = np.searchsorted(a, np.arange(len(blocks)), side="right")
    rest = np.empty(len(blocks), dtype=object)
    for k in range(len(blocks)):
        near = b[starts[k]:ends[k]]
        rest[k] = bounds[k] if len(near) == 0 else shapely.difference(bounds[k], shapely.union_all(buffers[near]))
    keep = km > 0
    return i[keep], j[keep], km[keep], rest


def nearest_zone(lon, lat, pieces, piece_zone, max_deg: float):
    """For each point, the zone of the nearest sea piece within `max_deg`, or -1."""
    pts = shapely.points(lon, lat)
    tree = shapely.STRtree(pieces)
    p, q = tree.query_nearest(pts, max_distance=max_deg, all_matches=True)
    # A tie goes to the first piece in the pieces' own order, not in the tree's: the tree's order
    # depends on everything it holds, that is, on the bbox.
    order = np.lexsort((q, p))
    p, q = p[order], q[order]
    first = np.r_[True, p[1:] != p[:-1]] if len(p) else np.zeros(0, bool)
    zone = np.full(len(pts), -1, dtype=np.int64)
    zone[p[first]] = np.asarray(piece_zone)[q[first]]
    return zone


def tile(geoms, values, bbox, step_deg: float):
    """Cut large geometries into `step_deg` tiles over `bbox` (for fast nearest queries; the
    pieces are only measured against, never output). Returns (pieces, value per piece)."""
    x0, y0, x1, y1 = bbox
    pieces, vals = [], []
    for g, v in zip(geoms, values):
        gx0, gy0, gx1, gy1 = g.bounds
        for tx in np.arange(np.floor(max(x0, gx0) / step_deg) * step_deg, min(x1, gx1), step_deg):
            for ty in np.arange(np.floor(max(y0, gy0) / step_deg) * step_deg, min(y1, gy1), step_deg):
                c = shapely.clip_by_rect(g, tx, ty, tx + step_deg, ty + step_deg)
                if not c.is_empty:
                    pieces.append(c)
                    vals.append(v)
    return np.array(pieces, dtype=object), np.array(vals)


def local_relief(elev: np.ndarray, window: int) -> np.ndarray:
    """Highest minus lowest value in a `window` × `window` square around each cell (edges
    padded by repetition)."""
    from numpy.lib.stride_tricks import sliding_window_view as win

    h = window // 2
    pad = np.pad(elev, h, mode="edge")
    hi = win(win(pad, window, axis=0).max(axis=-1), window, axis=1).max(axis=-1)
    lo = win(win(pad, window, axis=0).min(axis=-1), window, axis=1).min(axis=-1)
    return hi - lo


def row_cell_area_km2(transform, height: int) -> np.ndarray:
    """Area of one cell in each row of a north-up lon/lat grid, on a sphere of the WGS84
    authalic radius."""
    r = 6371.0072
    dlon = np.radians(abs(transform.a))
    top = transform.f + transform.e * np.arange(height)
    bottom = top + transform.e
    return r * r * dlon * np.abs(np.sin(np.radians(top)) - np.sin(np.radians(bottom)))


def zonal_sums(labels: np.ndarray, weights: np.ndarray, n: int) -> np.ndarray:
    """Sum of `weights` over cells of each label 1..n (label 0 is outside every block)."""
    return np.bincount(labels.ravel(), weights=weights.ravel(), minlength=n + 1)[1:]
