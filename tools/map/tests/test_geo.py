"""Geometry and raster helpers on synthetic shapes with known answers."""

import numpy as np
import pytest
import rasterio
import shapely

import geo

DEG_MERIDIAN_EQUATOR_KM = 110.574  # one degree of latitude at the equator, WGS84


def test_lengths_on_the_ellipsoid():
    meridian = shapely.LineString([(10, 0), (10, 1)])
    parallel_60 = shapely.LineString([(0, 60), (1, 60)])
    km = geo.lengths_km([meridian, parallel_60, shapely.Point(0, 0)])
    assert km[0] == pytest.approx(DEG_MERIDIAN_EQUATOR_KM, abs=0.01)
    assert km[1] == pytest.approx(55.8, abs=0.1)   # a degree of longitude at 60°
    assert km[2] == 0.0


def test_samples_carry_the_length_they_stand_for():
    line = shapely.LineString([(10, 0), (10, 1)])
    lon, lat, km, own = geo.sample_lines([line], 0.5)
    assert km.sum() == pytest.approx(DEG_MERIDIAN_EQUATOR_KM, abs=0.01)
    assert km.max() <= 0.5 and (own == 0).all()
    assert lat.min() > 0 and lat.max() < 1


def test_borders_need_a_common_edge_not_a_corner():
    a = shapely.box(0, 0, 1, 1)
    b = shapely.box(1, 0, 2, 1)          # shares the meridian x = 1
    c = shapely.box(2, 1, 3, 2)          # touches b only at the corner (2, 1)
    i, j, km, rest = geo.borders([a, b, c], 0.002)
    pairs = {(int(x), int(y)): k for x, y, k in zip(i, j, km)}
    assert pairs[(0, 1)] == pytest.approx(DEG_MERIDIAN_EQUATOR_KM, abs=0.6)
    assert pairs.get((1, 2), 0.0) < 1.0
    # a's remaining boundary is its three other sides
    assert geo.lengths_km([rest[0]])[0] == pytest.approx(3 * 111.0, rel=0.01)


def test_borders_absorb_a_sliver_narrower_than_the_tolerance():
    a = shapely.box(0, 0, 1, 1)
    b = shapely.box(1.001, 0, 2, 1)      # a 0.001° gap
    i, j, km, _ = geo.borders([a, b], 0.002)
    assert len(i) == 1 and km[0] > 100


def test_nearest_zone_within_reach_only():
    sea = [shapely.box(0, -1, 10, 0), shapely.box(0, 1, 10, 2)]
    lon = np.array([5.0, 5.0, 5.0])
    lat = np.array([0.01, 0.99, 0.5])
    zone = geo.nearest_zone(lon, lat, np.array(sea, dtype=object), np.array([7, 8]), 0.03)
    assert zone.tolist() == [7, 8, -1]


def test_tile_cuts_into_pieces_that_cover_the_shape():
    g = shapely.box(0.5, 0.5, 2.5, 1.5)
    pieces, vals = geo.tile([g], ["z"], (0, 0, 3, 3), 1.0)
    assert len(pieces) == 6 and set(vals) == {"z"}
    assert sum(p.area for p in pieces) == pytest.approx(g.area)


def test_local_relief():
    elev = np.zeros((9, 9), dtype=np.float32)
    elev[4, 4] = 500.0
    r = geo.local_relief(elev, 5)
    assert r[4, 4] == 500 and r[2, 2] == 500 and r[6, 6] == 500
    assert r[1, 4] == 0 and r[4, 7] == 0


def test_row_cell_area_sums_to_the_sphere_band():
    t = rasterio.Affine(1.0, 0, 0.0, 0, -1.0, 90.0)
    rows = geo.row_cell_area_km2(t, 180)
    sphere = 4 * np.pi * 6371.0072 ** 2
    assert rows.sum() * 360 == pytest.approx(sphere, rel=1e-9)
    assert rows[0] < rows[89]   # cells shrink toward the pole


def test_zonal_sums_skip_label_zero():
    labels = np.array([[0, 1], [2, 2]])
    weights = np.array([[5.0, 1.0], [2.0, 3.0]])
    assert geo.zonal_sums(labels, weights, 2).tolist() == [1.0, 5.0]
