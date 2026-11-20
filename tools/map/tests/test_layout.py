"""layout.py on synthetic blocks: the first province that selects a block takes it, and a
province's land is the innermost land holding the majority of its area."""

import numpy as np
import pytest

import density
import layout


def blocks(names, areas):
    n = len(names)
    return {"id": np.array([f"b{k}" for k in range(n)]), "country": np.array(["ITA"] * n), "name": np.array(names),
            "lon": np.zeros(n), "lat": np.zeros(n), "area": np.array(areas, float), "bw": np.zeros(n),
            "dry": np.zeros(n), "coastal": np.zeros(n, bool)}


def test_assign_first_province_wins_and_a_catch_all_takes_the_rest():
    b = blocks(["Milano", "Lodi", "Pavia"], [1, 1, 1])
    plan = {"province": [{"id": "milano", "parts": [{"countries": ["ITA"], "names": ["Milano", "Lodi"]}]},
                         {"id": "rest", "parts": [{"countries": ["ITA"]}]}]}
    assert list(layout.assign(plan, b)) == ["milano", "milano", "rest"]


def test_assign_refuses_a_province_that_selects_nothing():
    b = blocks(["Milano"], [1])
    plan = {"province": [{"id": "all", "parts": [{"countries": ["ITA"]}]},
                         {"id": "late", "parts": [{"countries": ["ITA"], "names": ["Milano"]}]}]}
    with pytest.raises(SystemExit):
        layout.assign(plan, b)


def test_province_land_is_the_innermost_land_of_the_majority():
    # Blocks: two of the inner land (inside the outer), two of the outer only, one of no land.
    b = blocks(["i1", "i2", "o1", "o2", "x"], [30, 30, 40, 40, 100])
    masks = {"outer": np.array([1, 1, 1, 1, 0], bool), "inner": np.array([1, 1, 0, 0, 0], bool)}
    lands = {"outer": {"region": "outer"}, "inner": {"region": "inner", "within": "outer"}}
    owner_of = np.array(["p", "p", "q", "q", "r"], dtype=object)
    owner_of[1] = "q"  # q: 30 of the inner land, 80 of the outer only → the outer land
    got = layout.province_lands(["p", "q", "r"], owner_of, b, masks, lands, ["outer", "inner"])
    assert got == {"p": "inner", "q": "outer", "r": None}


def test_land_fields_write_the_share_only_between_the_plain_classes():
    # No share: the class as it is.
    assert layout.land_fields(density.S, float("nan")) == ("Steppe", None)
    assert layout.land_fields(density.H, float("nan")) == ("Hills", None)
    # A share: arable with `hills` to three places; none is plain Arable, all is plain Hills —
    # whatever class the province had as a whole.
    assert layout.land_fields(density.H, 0.3764) == ("Arable", 0.376)
    assert layout.land_fields(density.A, 0.0004) == ("Arable", None)
    assert layout.land_fields(density.A, 0.9996) == ("Hills", None)
    assert layout.land_fields(density.H, 1.0) == ("Hills", None)


def test_mixed_density_lowers_only_arable_land_toward_the_hills():
    dens = np.zeros(5)
    dens[density.A], dens[density.H], dens[density.S], dens[density.I] = 50.0, 12.0, 1.0, 184.0
    cls = np.array([density.A, density.A, density.H, density.S, density.I])
    hills = np.array([0.0, 0.25, 0.25, 0.25, 0.25])
    # engine2's `capacity_of`: 100 km² at a quarter of hills carry 4,050 — 40.5 per km².
    assert layout.mixed_density(dens, cls, hills).tolist() == [50.0, 40.5, 12.0, 1.0, 184.0]


def test_a_dense_province_carries_its_class_and_no_share_of_hills():
    assert layout.land_fields(density.N, float("nan")) == ("Dense", None)
    dens = np.zeros(6)
    dens[density.A], dens[density.H], dens[density.N] = 50.0, 12.0, 75.0
    # A share of hills never lowers a dense block (the schema refuses the pair).
    assert layout.mixed_density(dens, np.array([density.N]), np.array([0.5])).tolist() == [75.0]


def test_a_province_label_is_inside_its_largest_part():
    import shapely

    # A horseshoe: its centroid lies in the gap, outside the province.
    shoe = shapely.Polygon([(0, 0), (3, 0), (3, 3), (2, 3), (2, 1), (1, 1), (1, 3), (0, 3)])
    assert not shoe.contains(shoe.centroid)
    assert shoe.contains(shapely.Point(layout.label_point(shoe)))
    # A mainland and a small island far off: the centroid is in the sea between them, the label
    # on the mainland — the larger part.
    mainland = shapely.box(0, 0, 4, 4)
    island = shapely.box(20, 0, 21, 1)
    both = shapely.MultiPolygon([island, mainland])
    assert mainland.contains(shapely.Point(layout.label_point(both)))
    # A sliver thinner than the rounding step: the rounded point would fall out, so the point is
    # kept as it is.
    sliver = shapely.Polygon([(0.00001, 0.00001), (2, 0.00001), (2, 0.00004), (0.00001, 0.00004)])
    assert sliver.contains(shapely.Point(layout.label_point(sliver)))


def test_the_written_geojson_carries_a_label_inside_every_province():
    import json
    import pathlib
    import shapely
    from shapely.geometry import shape

    root = pathlib.Path(__file__).resolve().parents[3] / "public" / "geodata2"
    files = sorted(root.glob("*/provinces.geojson"))
    assert len(files) == 3, files
    for f in files:
        feats = json.loads(f.read_text(encoding="utf-8"))["features"]
        for feat in feats:
            g = shape(feat["geometry"])
            assert g.contains(shapely.Point(feat["properties"]["label"])), (f.parent.name, feat["properties"]["id"])
