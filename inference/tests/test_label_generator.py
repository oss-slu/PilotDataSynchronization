import pytest

from label_generator import FlightEventLabeler


@pytest.mark.parametrize(
    "prev, curr, expected",
    [
        (90, 100, 10),
        (100, 90, -10),
        (355, 5, 10),
        (5, 355, -10),
        (0, 0, 0),
    ],
)
def test_heading_change_wraps_around_360(prev, curr, expected):
    labeler = FlightEventLabeler()
    assert labeler._calculate_heading_change(prev, curr) == pytest.approx(expected)


def test_heading_change_at_exactly_180_matches_deviation_convention():
    # Shared with heading deviation: [-180, 180), so +180 maps to -180 (#215).
    labeler = FlightEventLabeler()
    assert labeler._calculate_heading_change(0, 180) == -180
