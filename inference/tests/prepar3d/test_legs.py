"""Leg detection and Target attachment, with the scenario's four headings."""

import numpy as np
import pandas as pd
import pytest

from pilot_inference.paths import INFERENCE_DIR
from pilot_inference.prepar3d.legs import (
    Scenario,
    attach_targets,
    leg_summary,
    load_scenario,
    nearest_heading,
)

BRIEF = INFERENCE_DIR / "briefs" / "prepar3d_scenario.csv"
SCENARIO = Scenario(headings=(190.0, 40.0, 220.0, 0.0), level_altitude=3100.0)


def flight(n=10, **columns) -> pd.DataFrame:
    """An airborne sample table at 1 Hz, holding 190 at 3,100 ft by default."""
    table = {
        "pilot_id": ["P001"] * n,
        "flight_id": ["P001-T1"] * n,
        "t_seconds": [float(i) for i in range(n)],
        "altitude": [3100.0] * n,
        "heading": [190.0] * n,
        "airspeed": [110.0] * n,
        "vertical_speed": [0.0] * n,
        "bank": [0.0] * n,
        "pitch": [0.0] * n,
        "airborne": [True] * n,
    }
    table.update(columns)
    return pd.DataFrame(table)


def assert_targets(actual, expected) -> None:
    """Compare a Target column to what it should be, treating NaN as a value."""
    np.testing.assert_array_equal(
        np.asarray(actual, dtype=float), np.asarray(expected, dtype=float)
    )


def test_the_committed_brief_holds_the_scenario_from_the_client_meeting():
    scenario = load_scenario(BRIEF)
    assert sorted(scenario.headings) == [0.0, 40.0, 190.0, 220.0]
    assert scenario.level_altitude == 3100.0
    assert scenario.level_vertical_speed == 0.0


def test_a_brief_with_no_headings_is_rejected(tmp_path):
    path = tmp_path / "brief.csv"
    path.write_text("key,value\nlevel_altitude_ft,3100\n")
    with pytest.raises(ValueError, match="no scenario_heading"):
        load_scenario(path)


def test_a_brief_with_an_unknown_key_is_rejected(tmp_path):
    path = tmp_path / "brief.csv"
    path.write_text("key,value\nscenario_heading,190\nlevel_altitude_ft,3100\ntarget_airspeed,100\n")
    with pytest.raises(ValueError, match="unknown key"):
        load_scenario(path)


def test_nearest_heading_resolves_the_overlap_between_190_and_220():
    """At the default tolerance both would claim 200 to 210. Nearest wins."""
    assert nearest_heading([203.0], SCENARIO.headings).tolist() == [190.0]
    assert nearest_heading([212.0], SCENARIO.headings).tolist() == [220.0]


def test_nearest_heading_wraps_across_north():
    assert nearest_heading([358.0, 5.0], SCENARIO.headings).tolist() == [0.0, 0.0]


def test_nearest_heading_of_a_nan_is_nan():
    assert np.isnan(nearest_heading([np.nan], SCENARIO.headings)[0])


def test_a_held_heading_is_scored_after_its_settle_window():
    out = attach_targets(flight(n=10), SCENARIO, settle_seconds=5.0)
    assert_targets(out["target_heading"], [np.nan] * 5 + [190.0] * 5)


def test_the_settle_window_restarts_when_the_target_heading_changes():
    headings = [190.0] * 10 + [40.0] * 10
    out = attach_targets(flight(n=20, heading=headings), SCENARIO, settle_seconds=5.0)
    assert_targets(out["target_heading"], [np.nan] * 5 + [190.0] * 5 + [np.nan] * 5 + [40.0] * 5)


def test_a_banked_turn_is_not_a_heading_hold():
    bank = [0.0] * 5 + [20.0] * 5
    out = attach_targets(flight(n=10, bank=bank), SCENARIO, settle_seconds=0.0)
    assert_targets(out["target_heading"], [190.0] * 5 + [np.nan] * 5)


def test_a_heading_outside_the_tolerance_is_not_held():
    out = attach_targets(
        flight(n=4, heading=[190.0, 215.0, 150.0, 190.0]), SCENARIO, settle_seconds=0.0
    )
    # 215 is 5 off 220, so nearest picks 220. 150 is 40 off every scenario
    # heading, so it is in no Leg at all.
    assert_targets(out["target_heading"], [190.0, 220.0, np.nan, 190.0])


def test_a_heading_exactly_between_two_scenario_headings_takes_the_first():
    """205 is 15 from both 190 and 220. The tie is resolved, not left ambiguous."""
    assert nearest_heading([205.0], SCENARIO.headings).tolist() == [190.0]


def test_a_level_hold_needs_both_the_altitude_and_a_small_vertical_speed():
    out = attach_targets(
        flight(
            n=4,
            altitude=[3100.0, 3100.0, 3900.0, 3100.0],
            vertical_speed=[0.0, 900.0, 0.0, 0.0],
        ),
        SCENARIO,
        settle_seconds=0.0,
    )
    assert_targets(out["target_altitude"], [3100.0, np.nan, np.nan, 3100.0])
    assert_targets(out["target_vertical_speed"], [0.0, np.nan, np.nan, 0.0])


def test_a_climb_is_not_scored_for_altitude():
    climb = flight(n=6, altitude=[1500.0, 1900.0, 2300.0, 2700.0, 3100.0, 3100.0],
                   vertical_speed=[700.0] * 5 + [0.0])
    out = attach_targets(climb, SCENARIO, settle_seconds=0.0)
    assert_targets(out["target_altitude"], [np.nan] * 5 + [3100.0])


def test_the_two_hold_types_are_independent():
    """A sample can be scored for heading, for altitude, for both or neither."""
    out = attach_targets(
        flight(n=2, heading=[190.0, 150.0], vertical_speed=[0.0, 0.0]),
        SCENARIO,
        settle_seconds=0.0,
    )
    assert_targets(out["target_heading"], [190.0, np.nan])
    assert_targets(out["target_altitude"], [3100.0, 3100.0])


def test_a_gap_in_the_recording_splits_a_leg_and_settles_again():
    seconds = [0.0, 1.0, 2.0, 3.0, 30.0, 31.0, 32.0, 33.0]
    out = attach_targets(
        flight(n=8, t_seconds=seconds), SCENARIO, settle_seconds=2.0, max_gap_seconds=5.0
    )
    assert_targets(
        out["target_heading"], [np.nan, np.nan, 190.0, 190.0, np.nan, np.nan, 190.0, 190.0]
    )


def test_samples_on_the_ground_are_not_in_any_leg():
    airborne = [False] * 4 + [True] * 4
    out = attach_targets(flight(n=8, airborne=airborne), SCENARIO, settle_seconds=0.0)
    assert_targets(out["target_heading"], [np.nan] * 4 + [190.0] * 4)
    assert_targets(out["target_altitude"], [np.nan] * 4 + [3100.0] * 4)


def test_there_is_never_an_airspeed_target():
    """The scenario sets no airspeed. Inventing one would manufacture a result."""
    out = attach_targets(flight(), SCENARIO)
    assert out["target_airspeed"].isna().all()


def test_velocity_is_added_under_the_name_features_py_expects():
    out = attach_targets(flight(), SCENARIO)
    assert out["velocity"].tolist() == out["airspeed"].tolist()


def test_attach_targets_does_not_modify_its_input():
    df = flight()
    before = df.copy()
    attach_targets(df, SCENARIO)
    pd.testing.assert_frame_equal(df, before)


def test_an_empty_flight_still_gets_the_target_columns():
    out = attach_targets(flight(n=0), SCENARIO)
    assert out.empty
    for column in ("target_heading", "target_altitude", "target_vertical_speed", "scored"):
        assert column in out.columns


def test_leg_summary_counts_the_legs_and_the_scored_samples():
    headings = [190.0] * 10 + [40.0] * 10
    out = attach_targets(flight(n=20, heading=headings), SCENARIO, settle_seconds=5.0)
    summary = leg_summary(out)
    assert summary["samples"] == 20
    assert summary["heading_legs"] == 2
    assert summary["heading_scored_samples"] == 10
    assert summary["level_legs"] == 1
