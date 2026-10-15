import numpy as np
import pytest

from angles import angular_diff


@pytest.mark.parametrize(
    "actual, target, expected",
    [
        (90, 90, 0),
        (100, 90, 10),
        (80, 90, -10),
        (5, 355, 10),
        (355, 5, -10),
        (0, 359, 1),
        (270, 90, -180),
        (90, 270, -180),
        (180, 0, -180),
        (0, 180, -180),
    ],
)
def test_angular_diff_wraps_into_half_open_range(actual, target, expected):
    assert angular_diff(actual, target) == pytest.approx(expected)


def test_angular_diff_is_vectorised():
    actual = np.array([5.0, 355.0, 180.0])
    target = np.array([355.0, 5.0, 0.0])
    np.testing.assert_allclose(angular_diff(actual, target), [10.0, -10.0, -180.0])


def test_angular_diff_propagates_nan():
    assert np.isnan(angular_diff(90.0, np.nan))


def test_angular_diff_stays_in_range_for_any_input():
    rng = np.random.default_rng(0)
    diffs = angular_diff(rng.uniform(-720, 720, 1000), rng.uniform(-720, 720, 1000))
    assert diffs.min() >= -180
    assert diffs.max() < 180


@pytest.mark.parametrize("delta", [1e-15, 2.8e-14, 1e-13])
def test_angular_diff_stays_below_180_just_past_the_boundary(delta):
    # np.mod rounds a tiny negative remainder up to 360, which would give +180.
    assert -180 <= angular_diff(0.0, 180.0 + delta) < 180
