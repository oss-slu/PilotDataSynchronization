"""Trimming to the airborne part, and flagging rather than dropping."""

import numpy as np
import pandas as pd
import pytest

from pilot_inference.prepar3d.clean import (
    airborne_mask,
    clean_flight,
    find_gaps,
    find_spikes,
    ground_altitude,
)

GROUND_FT = 1100.0


def flight(**columns) -> pd.DataFrame:
    """A sample table from per-column lists, with the ids filled in."""
    length = len(next(iter(columns.values())))
    table = {
        "pilot_id": ["P001"] * length,
        "flight_id": ["P001-T1"] * length,
        "t_seconds": [float(i) for i in range(length)],
        "altitude": [GROUND_FT] * length,
        "heading": [190.0] * length,
        "airspeed": [0.0] * length,
        "vertical_speed": [0.0] * length,
        "bank": [0.0] * length,
        "pitch": [0.0] * length,
    }
    table.update(columns)
    return pd.DataFrame(table)


def parked(n):
    return [GROUND_FT] * n, [0.0] * n


def flying(n, altitude=3100.0, airspeed=110.0):
    return [altitude] * n, [airspeed] * n


def test_ground_altitude_is_the_median_of_the_samples_at_rest():
    df = flight(
        altitude=[1100.0, 1100.0, 1102.0, 5000.0, 5000.0],
        airspeed=[0.0, 5.0, 10.0, 110.0, 120.0],
    )
    assert ground_altitude(df) == pytest.approx(1100.0)


def test_ground_altitude_is_unknown_when_the_flight_is_never_at_rest():
    df = flight(altitude=[2000.0, 3000.0], airspeed=[110.0, 120.0])
    assert np.isnan(ground_altitude(df))


def test_without_a_ground_reference_airspeed_alone_decides():
    """A recording that starts after takeoff still has to produce Legs."""
    # Every sample is above the at-rest speed, so no field elevation can be
    # measured; the last is still below the airborne speed.
    df = flight(altitude=[2000.0, 3000.0, 3100.0], airspeed=[110.0, 120.0, 35.0])
    assert np.isnan(ground_altitude(df))
    assert airborne_mask(df).tolist() == [True, True, False]


def test_airborne_needs_both_speed_and_height():
    df = flight(
        # parked, takeoff roll (fast but low), climbing away, slow but high
        altitude=[GROUND_FT, GROUND_FT + 10, GROUND_FT + 500, GROUND_FT + 500],
        airspeed=[0.0, 60.0, 90.0, 10.0],
    )
    assert airborne_mask(df).tolist() == [False, False, True, False]


def test_clean_trims_the_ground_time_off_both_ends():
    altitude = [GROUND_FT] * 3 + [3100.0] * 4 + [GROUND_FT] * 3
    airspeed = [0.0] * 3 + [110.0] * 4 + [0.0] * 3
    trimmed, report = clean_flight(flight(altitude=altitude, airspeed=airspeed))
    assert len(trimmed) == 4
    assert trimmed["t_seconds"].tolist() == [3.0, 4.0, 5.0, 6.0]
    assert report.rows_in == 10
    assert report.rows_out == 4


def test_clean_keeps_a_touch_and_go_in_the_middle():
    """The scenario flies two circuits, so the roll between them is one Flight."""
    altitude = [GROUND_FT] * 2 + [3100.0] * 3 + [GROUND_FT] * 2 + [3100.0] * 3 + [GROUND_FT] * 2
    airspeed = [0.0] * 2 + [110.0] * 3 + [20.0] * 2 + [110.0] * 3 + [0.0] * 2
    trimmed, _ = clean_flight(flight(altitude=altitude, airspeed=airspeed))
    assert len(trimmed) == 8
    assert trimmed["airborne"].tolist() == [True] * 3 + [False] * 2 + [True] * 3


def test_a_flight_that_never_leaves_the_ground_yields_nothing_and_says_so():
    trimmed, report = clean_flight(flight(altitude=[GROUND_FT] * 4, airspeed=[0.0] * 4))
    assert trimmed.empty
    assert report.rows_out == 0
    assert "never airborne" in report.note


def test_recorded_and_airborne_minutes_are_both_reported():
    altitude = [GROUND_FT] * 60 + [3100.0] * 120 + [GROUND_FT] * 60
    airspeed = [0.0] * 60 + [110.0] * 120 + [0.0] * 60
    _, report = clean_flight(flight(altitude=altitude, airspeed=airspeed))
    assert report.recorded_minutes == pytest.approx(239 / 60.0)
    assert report.airborne_minutes == pytest.approx(119 / 60.0)


def test_find_gaps_reports_where_and_how_long():
    df = flight(t_seconds=[0.0, 1.0, 2.0, 9.0, 10.0], altitude=[3100.0] * 5)
    assert find_gaps(df, max_gap_seconds=5.0) == [(2.0, 7.0)]


def test_find_gaps_ignores_a_gap_at_the_threshold():
    df = flight(t_seconds=[0.0, 5.0, 10.0], altitude=[3100.0] * 3)
    assert find_gaps(df, max_gap_seconds=5.0) == []


def test_gaps_are_flagged_not_dropped():
    altitude = [3100.0] * 5
    airspeed = [110.0] * 5
    df = flight(t_seconds=[0.0, 1.0, 2.0, 20.0, 21.0], altitude=altitude, airspeed=airspeed)
    trimmed, report = clean_flight(df, max_gap_seconds=5.0)
    assert len(trimmed) == 5
    assert len(report.gaps) == 1
    assert report.max_gap_seconds == 18.0


def test_find_spikes_marks_only_the_samples_past_the_limit():
    df = flight(vertical_speed=[700.0, -1700.0, 2400.0, -3800.0])
    assert find_spikes(df, spike_fpm=2000.0).tolist() == [False, False, True, True]


def test_spikes_are_counted_and_described_rather_than_removed():
    """P004 T2 to T4 reach 2,400 to 3,800 fpm. That is a question, not a filter."""
    df = flight(
        altitude=[3100.0] * 4,
        airspeed=[110.0] * 4,
        vertical_speed=[0.0, 2400.0, -3800.0, 0.0],
    )
    trimmed, report = clean_flight(df, spike_fpm=2000.0)
    assert len(trimmed) == 4
    assert report.spikes == 2
    assert report.vertical_speed_max_fpm == 2400.0
    assert report.vertical_speed_min_fpm == -3800.0
    assert "2 sample(s)" in report.note


def test_the_report_row_is_flat_enough_for_a_csv():
    _, report = clean_flight(flight(altitude=[3100.0] * 3, airspeed=[110.0] * 3))
    row = report.as_row()
    assert row["flight_id"] == "P001-T1"
    assert all(not isinstance(value, (list, dict)) for value in row.values())


def test_a_nan_altitude_does_not_count_as_airborne():
    df = flight(altitude=[GROUND_FT, np.nan, 3100.0], airspeed=[0.0, 110.0, 110.0])
    assert airborne_mask(df).tolist() == [False, False, True]
