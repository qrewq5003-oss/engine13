"""The rules of plan §4 on provinces made of blocks."""

import pytest

import rules

P = rules.Params(l_min_km=5.0, coast_min_km=5.0, hills_share=0.5, desert_share=0.5, steppe_share=0.5,
                 irrigated_min_river_km=50.0)


def block(area=100.0, rugged=0.0, bw=0.0, bs=0.0, river=0.0):
    return rules.Block(area_km2=area, raster_km2=area, rugged_km2=rugged * area, climate_km2=area,
                       bw_km2=bw * area, bs_km2=bs * area, river_km=river)


@pytest.mark.parametrize("rugged,bw,bs,land", [
    (0.5, 0.0, 0.0, "Hills"),
    (0.49, 0.0, 0.0, "Arable"),
    (0.6, 0.9, 0.0, "Hills"),      # order of §4: Hills before the climate
    (0.0, 0.5, 0.0, "Desert"),
    (0.0, 0.3, 0.2, "Steppe"),     # dry overall, not mostly desert
    (0.0, 0.0, 0.49, "Arable"),
])
def test_land_type_order_and_thresholds(rugged, bw, bs, land):
    assert rules.land_type(rugged, bw, bs, P) == land


def test_shares_are_area_weighted_over_blocks():
    blocks = {"a": block(300, rugged=1.0), "b": block(100, rugged=0.0), "c": block(100, rugged=1.0),
              "d": block(500, rugged=0.0)}
    provs = rules.provinces({"a": "P", "b": "P", "c": "Q", "d": "Q"}, blocks, [], [], P)
    assert provs["P"].land == "Hills" and provs["P"].shares["rugged"] == pytest.approx(0.75)
    assert provs["Q"].land == "Arable" and provs["Q"].shares["rugged"] == pytest.approx(1 / 6)
    assert provs["P"].area_km2 == 400 and provs["Q"].area_km2 == 600


def test_neighbours_sum_block_borders_and_need_l_min():
    blocks = {k: block() for k in "abcde"}
    assign = {"a": "P", "b": "P", "c": "Q", "d": "R", "e": "R"}
    edges = [("a", "c", 3.0), ("b", "c", 3.0),   # P–Q: 6 km over two block borders
             ("c", "d", 4.9),                     # Q–R: short of L_min
             ("a", "b", 100.0), ("d", "e", 100.0)]  # inside a province: no link
    provs = rules.provinces(assign, blocks, edges, [], P)
    assert provs["P"].neighbours == ["Q"] and provs["Q"].neighbours == ["P"]
    assert provs["R"].neighbours == []


def test_coast_zones_and_sea_neighbours():
    blocks = {k: block() for k in "abcd"}
    assign = {"a": "P", "b": "Q", "c": "R", "d": "S"}
    coast = [("a", "Ligurian Sea", 20.0), ("a", "Tyrrhenian Sea", 4.0),
             ("b", "Ligurian Sea", 6.0),
             ("c", "Tyrrhenian Sea", 30.0),
             ("d", "Tyrrhenian Sea", 2.0), ("d", "Ligurian Sea", 2.0)]
    provs = rules.provinces(assign, blocks, [], coast, P)
    assert provs["P"].zones == ["Ligurian Sea"]           # 4 km of Tyrrhenian coast is not a face
    assert provs["P"].sea_neighbours == ["Q"] and provs["Q"].sea_neighbours == ["P"]
    assert provs["R"].coastal and provs["R"].sea_neighbours == []
    assert not provs["S"].coastal and provs["S"].sea_neighbours == []   # 4 km in all


def test_sea_neighbours_only_between_coastal_and_symmetric():
    blocks = {k: block() for k in "abc"}
    coast = [("a", "Z", 10.0), ("b", "Z", 10.0), ("c", "Z", 10.0)]
    provs = rules.provinces({"a": "A", "b": "B", "c": "C"}, blocks, [], coast, P)
    for x, v in provs.items():
        assert v.coastal
        for y in v.sea_neighbours:
            assert x in provs[y].sea_neighbours


def test_irrigated_flag_marks_dry_river_land_and_keeps_its_type():
    blocks = {"nile": block(bw=0.9, river=200.0), "po": block(river=300.0), "wadi": block(bw=0.9, river=10.0)}
    provs = rules.provinces({k: k for k in blocks}, blocks, [], [], P)
    assert provs["nile"].irrigated and provs["nile"].land == "Desert"
    assert not provs["po"].irrigated and provs["po"].land == "Arable"
    assert not provs["wadi"].irrigated


def test_components_join_islands_by_sea_only():
    blocks = {k: block() for k in "abc"}
    edges = [("a", "b", 50.0)]
    coast = [("b", "Z", 10.0), ("c", "Z", 10.0)]
    provs = rules.provinces({k: k for k in blocks}, blocks, edges, coast, P)
    assert rules.components(provs) == [["a", "b", "c"]]
    provs = rules.provinces({k: k for k in blocks}, blocks, edges, [("b", "Z", 10.0)], P)
    assert rules.components(provs) == [["a", "b"], ["c"]]
