"""layout.py on synthetic blocks: the first province that selects a block takes it, and a
province's land is the innermost land holding the majority of its area."""

import numpy as np
import pytest

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
