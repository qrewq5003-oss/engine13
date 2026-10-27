"""density.py on synthetic lands with known answers: the fit (nnls_relative) and the
calibration of classes (calibrate)."""

import numpy as np
import pytest

import density
from density import A, D, H, S

DENS = np.zeros(5)
DENS[[A, H, S, D]] = [40.0, 12.0, 1.0, 0.5]


def test_nnls_two_lands_recovers_their_densities():
    # Shares of (Arable, Hills) in three regions; the densities are 40 and 12.
    X = np.array([[1.0, 0.0], [0.0, 1.0], [0.5, 0.5]])
    d = density.nnls_relative(X, X @ np.array([40.0, 12.0]))
    assert d == pytest.approx([40.0, 12.0])


def test_nnls_clamps_a_negative_density_to_zero():
    # Unconstrained, d = (20, -10). With d >= 0 the second is 0 and the first minimises
    # ((d - 10) / 10)^2 + ((d - 20) / 20)^2, that is d = 12.
    X = np.array([[1.0, 1.0], [1.0, 0.0]])
    d = density.nnls_relative(X, np.array([10.0, 20.0]))
    assert d == pytest.approx([12.0, 0.0])


def land(rugged, dry=None, bw=None):
    n = len(rugged)
    return (np.full(n, 100.0), np.array(rugged, float), np.zeros(n) if dry is None else np.array(dry, float),
            np.zeros(n, bool) if bw is None else np.array(bw, float) >= 0.5)


def test_calibrate_dense_land_turns_the_most_rugged_to_hills():
    area, rugged, dry, desert = land([0.2, 0.9, 0.5])
    # 300 km² at 30.67 per km² = 9200 people = 2 × 100 × 40 + 100 × 12.
    cls = density.calibrate(area, rugged, dry, desert, 9200.0, DENS)
    assert list(cls) == [A, H, A]


def test_calibrate_sparse_land_is_hills_with_the_driest_as_steppe():
    area, rugged, dry, desert = land([0.0, 0.0], dry=[0.2, 0.9])
    # 200 km² at 6.5 per km² = 1300 people = 100 × 12 + 100 × 1.
    cls = density.calibrate(area, rugged, dry, desert, 1300.0, DENS)
    assert list(cls) == [H, S]


def test_calibrate_keeps_desert_and_stops_at_the_closest_capacity():
    area, rugged, dry, desert = land([0.9, 0.8, 0.0], bw=[0.0, 0.0, 0.9])
    # Desert stays: 50. Arable × 2 → 8050; one Hills → 5250; two → 2450. A target of 6000 is
    # closer to 5250 than to 8050, and 2450 would be farther again.
    cls = density.calibrate(area, rugged, dry, desert, 6000.0, DENS)
    assert list(cls) == [H, A, D]


def test_calibrate_variants_order_to_hills_and_steppe_only_where_dry():
    # Order to Hills by another key: the second unit, not the more rugged first, goes to Hills.
    area, rugged, dry, desert = land([0.9, 0.2])
    cls = density.calibrate(area, rugged, dry, desert, 5200.0, DENS, key=np.array([0.2, 0.9]))
    assert list(cls) == [A, H]
    # Steppe only where the climate is dry: no unit is dry enough, so the land stays Hills.
    area, rugged, dry, desert = land([0.0, 0.0], dry=[0.2, 0.4])
    assert list(density.calibrate(area, rugged, dry, desert, 1300.0, DENS)) == [H, S]
    assert list(density.calibrate(area, rugged, dry, desert, 1300.0, DENS, dry_steppe=0.5)) == [H, H]


def test_calibrate_a_ceiling_below_hills_turns_the_coldest_sparse_whatever_the_dryness():
    # 300 km² at 4.67 per km² = 1400 people = 100 × 12 + 2 × 100 × 1: two units turn Steppe — the
    # two coldest, though none is dry; without `cold` no unit may (Steppe only where dry).
    area, rugged, dry, desert = land([0.0, 0.0, 0.0], dry=[0.0, 0.1, 0.0])
    cold = np.array([0.9, 0.0, 0.6])
    assert list(density.calibrate(area, rugged, dry, desert, 1400.0, DENS, dry_steppe=0.5, cold=cold)) == [S, H, S]
    assert list(density.calibrate(area, rugged, dry, desert, 1400.0, DENS, dry_steppe=0.5)) == [H, H, H]


def test_hills_share_holds_the_ceiling_without_flipping_a_unit():
    # 200 km², poor shares 0.2 and 0.6, target 31 per km² = 6200 people. With h = min(1, k·poor):
    # 100·(40 − 28·0.2k) + 100·(40 − 28·0.6k) = 6200 → k = 1800 / 2240; no unit flips whole.
    area, poor = np.full(2, 100.0), np.array([0.2, 0.6])
    h, k = density.hills_share(area, poor, np.zeros(2, bool), 6200.0, DENS)
    assert k == pytest.approx(1800 / 2240)
    assert float(area @ (40 - 28 * h)) == pytest.approx(6200.0)
    # A land at Arable or above keeps h = 0.
    assert list(density.hills_share(area, poor, np.zeros(2, bool), 9000.0, DENS)[0]) == [0.0, 0.0]


def test_a_nested_rule_1_land_keeps_its_class_under_hills_share():
    # Outer land of three units at a ceiling of 40 (= Arable); the inner «alps» (rule 1, Hills) is
    # its third unit. The alps keep 12; the outer land cannot reach 40 with them and keeps the
    # residual — it does not lift the alps to Arable.
    area, poor, bw = np.full(3, 100.0), np.array([0.3, 0.5, 0.9]), np.zeros(3)
    member = {"alps": np.array([False, False, True]), "outer": np.ones(3, bool)}
    ceiling = {"alps": None, "outer": 40.0}
    cls = np.array([A, A, H])
    hshare, hdens, under, hland = density.hills_share_lands(["alps", "outer"], member, ceiling, area, poor, bw, DENS, cls)
    assert list(hdens) == [40.0, 40.0, 12.0]
    assert list(under) == [True, True, False] and np.isnan(hshare[2])
    assert float(area @ hdens) / area.sum() == pytest.approx(30.67, abs=0.01)


def test_a_river_land_is_not_held_at_desert_by_its_climate():
    # Two desert-climate units, a source of 26 per km² (5200 people on 200 km²): as a river land
    # they calibrate like any other land; otherwise both stay Desert.
    area, rugged, dry = np.full(2, 100.0), np.array([0.1, 0.0]), np.ones(2)
    member, ceiling = {"sawad": np.ones(2, bool)}, {"sawad": 26.0}
    p = type("P", (), {"steppe_share": 0.5})()
    bw = np.ones(2)
    cls, _ = density.calibrate_lands(["sawad"], member, ceiling, area, rugged, dry, bw, DENS, p, river={"sawad"})
    assert list(cls) == [H, A]
    cls, _ = density.calibrate_lands(["sawad"], member, ceiling, area, rugged, dry, bw, DENS, p)
    assert list(cls) == [D, D]


def test_sparse_lands_are_those_a_later_source_keeps_under_half_of_hills():
    top = {"north": {"y": 0.6}, "plain": {"y": 6.0}, "hungary": {"y": 6.3}}
    obs = [{"land": "north", "y": 1.9, "later": True}, {"land": "plain", "y": 16.0, "later": True}]
    # «hungary» has no later point: its source stays a lower bound.
    assert density.sparse_lands(obs, top, DENS, 0.5) == {"north"}


def test_calibrate_nearest_gives_continuous_densities_and_the_nearest_class():
    area, rugged, dry, desert = land([0.5, 1.0])
    # 100·(40 − 28·0.5k) + 100·(40 − 28·min(1, k)) = 3900 → k = 4100/4200; d = 26.33 and 12.67.
    cls, d, k = density.calibrate_nearest(area, rugged, dry, desert, 3900.0, DENS)
    assert k == pytest.approx(4100 / 4200)
    assert d == pytest.approx([40 - 14 * 4100 / 4200, 40 - 28 * 4100 / 4200])
    assert list(cls) == [A, H]  # 26.33 is above the middle of 40 and 12, 12.67 below


def test_calibrate_nearest_above_arable_everywhere_is_all_arable():
    area, rugged, dry, desert = land([0.9, 0.1])
    cls, d, k = density.calibrate_nearest(area, rugged, dry, desert, 10000.0, DENS)
    assert (list(cls), list(d), k) == ([A, A], [40.0, 40.0], 0.0)


def test_plague_balance_fill_is_where_growth_meets_the_loss():
    engine = {"plague": {"outbreaks_per_year": {"value": 0.08}, "mortality": {"value": 0.03}},
              "population": {"growth_per_year": {"value": 0.012}}}
    assert density.plague_balance_fill(engine) == pytest.approx(0.8)


def test_calibrate_above_arable_everywhere_leaves_a_residual():
    area, rugged, dry, desert = land([0.9, 0.1])
    cls = density.calibrate(area, rugged, dry, desert, 10000.0, DENS)
    assert list(cls) == [A, A]
    assert float(area @ DENS[cls]) == pytest.approx(8000.0)
