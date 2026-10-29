import pandas as pd
import pytest

from pilot_inference.capture import data_logger
from pilot_inference.capture.replay import brief_for, packets_for_row, select_flight

ROW = {"altitude": 3000.5, "heading": 271.25, "vertical_speed": -40.0, "velocity": 99.5}


def test_packets_follow_the_relay_order():
    events = [packet.split(";")[7] for packet in packets_for_row(ROW)]
    assert events == [
        "AltitudeSync",
        "AirspeedSync",
        "VerticalVelocitySync",
        "HeadingSync",
        "RollSync",
        "PitchSync",
        "YawSync",
        "GForceSync",
    ]


def test_packets_read_back_as_one_complete_sample():
    sample = data_logger.SampleBuffer()
    for packet in packets_for_row(ROW):
        sample.update(*data_logger.parse_packet(packet))
    assert sample.is_complete()
    assert {k: sample.values[k] for k in ROW} == ROW
    assert sample.values["roll"] == 0.0


def flights():
    return pd.DataFrame(
        {
            "flight_id": ["A", "A", "B"],
            "altitude": [1.0, 2.0, 3.0],
            "target_altitude": [3000.0, 3000.0, 2000.0],
            "target_heading": [90.0, 90.0, 90.0],
            "target_vertical_speed": [0.0, 0.0, 0.0],
            "target_airspeed": [100.0, 100.0, 100.0],
        }
    )


def test_several_flights_need_one_picked():
    with pytest.raises(ValueError, match="--flight"):
        select_flight(flights(), None)
    assert select_flight(flights(), "B")["altitude"].tolist() == [3.0]


def test_an_unknown_flight_is_refused():
    with pytest.raises(ValueError, match="'C'"):
        select_flight(flights(), "C")


def test_brief_holds_a_flights_constant_targets():
    brief = brief_for(select_flight(flights(), "A"))
    assert brief.iloc[0][["altitude", "heading", "vertical_speed", "airspeed"]].tolist() == [
        3000.0,
        90.0,
        0.0,
        100.0,
    ]


def test_brief_is_refused_when_targets_change():
    with pytest.raises(ValueError, match="target_altitude"):
        brief_for(flights())


def test_a_flight_cannot_be_picked_from_a_csv_without_flight_ids():
    with pytest.raises(ValueError, match="flight_id"):
        select_flight(pd.DataFrame({"altitude": [1.0, 2.0]}), "A")
