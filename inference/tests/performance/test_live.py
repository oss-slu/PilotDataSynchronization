import math
from datetime import datetime, timedelta, timezone

import pytest

from pilot_inference.performance.features import METRICS
from pilot_inference.performance.live import (
    MAX_SAMPLE_GAP_SECONDS,
    Instruction,
    LiveFlight,
    load_brief,
)

T0 = datetime(2026, 10, 26, 15, 0, tzinfo=timezone.utc)
HOLD = {"altitude": 3000.0, "heading": 270.0, "vertical_speed": 0.0, "airspeed": 100.0}


def at(seconds: float) -> datetime:
    return T0 + timedelta(seconds=seconds)


def sample(**overrides) -> dict:
    values = {
        "altitude": 3010.0,
        "heading": 268.0,
        "vertical_speed": 20.0,
        "velocity": 98.0,
        "roll": 0.0,
        "pitch": 0.0,
        "yaw": 268.0,
        "g_force": 1.0,
    }
    values.update(overrides)
    return values


def two_holds(settle: float = 10.0) -> LiveFlight:
    brief = [Instruction("hold 270", dict(HOLD)), Instruction("turn 360", dict(HOLD, heading=360.0))]
    return LiveFlight(brief, "p1", "f1", settle=timedelta(seconds=settle))


def write_brief(tmp_path, text: str):
    path = tmp_path / "brief.csv"
    path.write_text(text, encoding="utf-8")
    return path


def test_brief_keeps_order_and_targets(tmp_path):
    path = write_brief(
        tmp_path,
        "instruction,heading,altitude,airspeed,vertical_speed\n"
        '"Fly heading 270, 3000 ft",270,3000,100,0\n'
        "Turn 360,360,3000,100,0\n",
    )
    brief = load_brief(path)
    assert [i.text for i in brief] == ["Fly heading 270, 3000 ft", "Turn 360"]
    assert brief[1].targets == dict(HOLD, heading=360.0)


def test_brief_missing_a_factor_is_refused(tmp_path):
    path = write_brief(tmp_path, "instruction,heading,altitude,airspeed\nhold,270,3000,100\n")
    with pytest.raises(ValueError, match="vertical_speed"):
        load_brief(path)


def test_a_blank_target_is_left_to_carry_over(tmp_path):
    path = write_brief(
        tmp_path,
        "instruction,heading,altitude,airspeed,vertical_speed\nhold,270,3000,100,0\nturn,360,,,\n",
    )
    assert load_brief(path)[1].targets == {"heading": 360.0}


def test_the_first_instruction_must_set_every_target(tmp_path):
    path = write_brief(
        tmp_path,
        "instruction,heading,altitude,airspeed,vertical_speed\nhold,270,,100,0\nturn,360,,,\n",
    )
    with pytest.raises(ValueError, match="instruction 1 has no target for altitude"):
        load_brief(path)


def test_nothing_is_scored_before_the_first_instruction():
    flight = two_holds()
    row = flight.add_sample(sample(), at(0))
    assert all(math.isnan(row[f"target_{m}"]) for m in METRICS)
    assert flight.pilot_features() is None


def test_targets_are_blank_while_settling_then_set():
    flight = two_holds(settle=10)
    flight.instruct(at(0))
    settling = flight.add_sample(sample(), at(9.9))
    scored = flight.add_sample(sample(), at(10))
    assert math.isnan(settling["target_heading"])
    assert scored["target_heading"] == 270.0
    assert scored["target_altitude"] == 3000.0


def test_an_exhausted_brief_keeps_the_last_instruction():
    flight = two_holds(settle=0)
    flight.instruct(at(0))
    flight.instruct(at(5))
    assert flight.instruct(at(10)) is None
    assert flight.given == 2
    assert flight.targets_at(at(11))["heading"] == 360.0


def test_scored_time_follows_the_samples():
    flight = two_holds(settle=0)
    flight.instruct(at(0))
    for i in range(41):
        flight.add_sample(sample(), at(i * 0.05))
    assert flight.scored_seconds == pytest.approx(2.0)


def test_a_stall_counts_as_at_most_one_gap():
    flight = two_holds(settle=0)
    flight.instruct(at(0))
    flight.add_sample(sample(), at(0))
    flight.add_sample(sample(), at(30))
    assert flight.scored_seconds == pytest.approx(MAX_SAMPLE_GAP_SECONDS)


def test_settling_after_a_new_instruction_is_not_scored_time():
    flight = two_holds(settle=10)
    flight.instruct(at(0))
    flight.add_sample(sample(), at(10))
    flight.add_sample(sample(), at(10.5))
    flight.instruct(at(11))
    flight.add_sample(sample(), at(15))  # heading settling, altitude still scored
    flight.add_sample(sample(), at(21))  # first scored heading of the new hold
    flight.add_sample(sample(), at(21.5))
    assert flight.scored_seconds_by_factor["heading"] == pytest.approx(1.0)
    # Altitude never settled: 0.5 s, two stalls of at most a second each, then 0.5 s.
    assert flight.scored_seconds_by_factor["altitude"] == pytest.approx(3.0)
    # A group waits on the least-scored factor.
    assert flight.scored_seconds == pytest.approx(1.0)


def test_features_score_each_sample_against_its_own_instruction():
    flight = two_holds(settle=0)
    flight.instruct(at(0))
    flight.add_sample(sample(heading=272.0), at(0))
    flight.add_sample(sample(heading=272.0), at(0.05))
    flight.instruct(at(1))
    # 2 deg past north is 2 deg off 360, not 358.
    flight.add_sample(sample(heading=2.0), at(1))
    flight.add_sample(sample(heading=2.0), at(1.05))
    pilots = flight.pilot_features()
    assert pilots["pilot_id"].tolist() == ["p1"]
    assert pilots["heading_bias"].iloc[0] == pytest.approx(2.0)
    assert pilots["heading_std"].iloc[0] == pytest.approx(0.0)
    assert pilots["altitude_abs_bias"].iloc[0] == pytest.approx(10.0)


def test_a_sample_handled_after_an_instruction_keeps_the_one_it_arrived_under():
    flight = two_holds(settle=0)
    flight.instruct(at(0))
    flight.add_sample(sample(), at(4.95))
    flight.instruct(at(10))
    # Arrived at 5 s, before the turn to 360, but handled after it.
    row = flight.add_sample(sample(), at(5))
    assert row["target_heading"] == 270.0
    assert flight.scored_seconds == pytest.approx(0.05)


def hold_then_turn(settle: float = 10.0) -> LiveFlight:
    """A level hold on 270, then a turn to 360 that names only the heading."""
    brief = [Instruction("hold 270", dict(HOLD)), Instruction("turn 360", {"heading": 360.0})]
    return LiveFlight(brief, "p1", "f1", settle=timedelta(seconds=settle))


def test_targets_an_instruction_does_not_name_carry_over():
    flight = hold_then_turn(settle=0)
    flight.instruct(at(0))
    flight.instruct(at(5))
    assert flight.targets_at(at(6)) == dict(HOLD, heading=360.0)


def test_only_the_targets_an_instruction_changes_settle():
    flight = hold_then_turn(settle=10)
    flight.instruct(at(0))
    flight.instruct(at(20))
    during_turn = flight.targets_at(at(25))
    assert math.isnan(during_turn["heading"])
    assert during_turn["altitude"] == 3000.0
    assert flight.targets_at(at(30))["heading"] == 360.0


def test_a_target_restated_at_the_same_value_does_not_settle():
    flight = two_holds(settle=10)
    flight.instruct(at(0))
    # The turn to 360 restates 3000 ft, 100 kts and level flight.
    flight.instruct(at(20))
    during_turn = flight.targets_at(at(25))
    assert math.isnan(during_turn["heading"])
    assert during_turn["altitude"] == 3000.0


def test_settling_says_how_long_each_factor_has_left():
    flight = hold_then_turn(settle=10)
    flight.instruct(at(0))
    # The turn comes before the first settle window ends: only heading restarts.
    flight.instruct(at(4))
    left = flight.settling(at(4))
    assert left["heading"] == pytest.approx(10.0)
    assert left["altitude"] == pytest.approx(6.0)
    assert flight.settling(at(12)) == {"heading": pytest.approx(2.0)}


def test_north_restated_as_0_after_360_does_not_settle():
    brief = [Instruction("hold 360", dict(HOLD, heading=360.0)), Instruction("heading 0", {"heading": 0.0})]
    flight = LiveFlight(brief, "p1", "f1", settle=timedelta(seconds=10))
    flight.instruct(at(0))
    flight.instruct(at(20))
    assert flight.settling(at(21)) == {}
