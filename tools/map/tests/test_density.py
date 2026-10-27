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


def test_plague_balance_fill_is_where_growth_meets_the_loss():
    engine = {"plague": {"outbreaks_per_year": {"value": 0.08}, "mortality": {"value": 0.03}},
              "population": {"growth_per_year": {"value": 0.012}}}
    assert density.plague_balance_fill(engine) == pytest.approx(0.8)


def test_calibrate_above_arable_everywhere_leaves_a_residual():
    area, rugged, dry, desert = land([0.9, 0.1])
    cls = density.calibrate(area, rugged, dry, desert, 10000.0, DENS)
    assert list(cls) == [A, A]
    assert float(area @ DENS[cls]) == pytest.approx(8000.0)
