"""The live sidecar end to end: relay packets in, samples with targets and a group out."""

import queue
import socket
import threading
from datetime import datetime, timedelta, timezone

import pandas as pd
import pytest

from pilot_inference.capture.replay import replay
from pilot_inference.performance.features import METRICS, compute_features
from pilot_inference.performance.live import LIVE_CSV_FIELDS, Instruction, LiveFlight
from pilot_inference.performance.live_groups import (
    STOP,
    CsvLog,
    instruction_row,
    instructions_path,
    record_samples,
    reference_cohort,
    serve_relay,
    status_line,
)

T0 = datetime(2026, 10, 26, 15, 0, tzinfo=timezone.utc)
HOLD = {"altitude": 3000.0, "heading": 270.0, "vertical_speed": 0.0, "airspeed": 100.0}


def at(seconds: float) -> datetime:
    return T0 + timedelta(seconds=seconds)


def wobbly_flight(n: int) -> pd.DataFrame:
    """n samples at 20 Hz wobbling round HOLD, with a heading that crosses north."""
    i = pd.Series(range(n), dtype=float)
    return pd.DataFrame(
        {
            "altitude": 3000.0 + 20.0 * (i % 7 - 3),
            "heading": (270.0 + 3.0 * (i % 5 - 2)) % 360.0,
            "vertical_speed": 50.0 * (i % 3 - 1),
            "velocity": 100.0 + (i % 4 - 1.5),
        }
    )


@pytest.fixture(scope="module")
def reference():
    groups, _ = reference_cohort(n_pilots=15, k=3, seed=0)
    return groups


def listening_socket() -> socket.socket:
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.bind(("127.0.0.1", 0))
    server.listen()
    return server


def test_replayed_packets_arrive_as_samples():
    server = listening_socket()
    samples: queue.Queue = queue.Queue()
    reader = threading.Thread(target=serve_relay, args=(server, samples), daemon=True)
    reader.start()
    flight = wobbly_flight(25)

    replay(flight, "127.0.0.1", server.getsockname()[1], hz=0)

    received = [samples.get(timeout=5) for _ in range(len(flight))]
    server.close()
    assert [values["altitude"] for values, _ in received] == pytest.approx(flight["altitude"].tolist())
    assert [values["velocity"] for values, _ in received] == pytest.approx(flight["velocity"].tolist())
    # Columns the CSV lacks are sent as zero, so every sample is complete.
    assert all(values["g_force"] == 0.0 for values, _ in received)


def test_saved_flight_gives_the_live_features_offline(tmp_path):
    brief = [Instruction("hold 270", dict(HOLD)), Instruction("turn 360", dict(HOLD, heading=360.0))]
    flight = LiveFlight(brief, "p1", "f1", settle=timedelta(seconds=2))
    log = CsvLog(tmp_path / "live.csv", LIVE_CSV_FIELDS)
    samples: queue.Queue = queue.Queue()
    recorder = threading.Thread(
        target=record_samples, args=(samples, flight, threading.Lock(), log), daemon=True
    )
    recorder.start()

    telemetry = wobbly_flight(200)
    telemetry[["roll", "pitch", "yaw", "g_force"]] = 0.0
    flight.instruct(at(0))
    for i, values in enumerate(telemetry.to_dict("records")):
        if i == 100:
            flight.instruct(at(i * 0.05))
        samples.put((values, at(i * 0.05)))
    samples.put(STOP)
    recorder.join(timeout=5)
    log.close()

    saved = pd.read_csv(tmp_path / "live.csv")
    assert list(saved.columns) == LIVE_CSV_FIELDS
    assert len(saved) == 200
    # Settling rows are saved with blank targets, so offline scoring skips them too.
    assert saved["target_heading"].isna().sum() == 2 * 40
    _, offline = compute_features(saved)
    live = flight.pilot_features()
    columns = [f"{m}_{s}" for m in METRICS for s in ("mad", "std", "bias", "abs_bias")]
    pd.testing.assert_frame_equal(offline[columns], live[columns], check_exact=False)


def test_status_waits_for_the_first_instruction(reference):
    flight = LiveFlight([Instruction("hold", dict(HOLD))], "p1", "f1")
    assert "Waiting for the first instruction" in status_line(flight, reference, 60, at(0))


def test_status_says_settling_until_scoring_starts(reference):
    flight = LiveFlight([Instruction("hold", dict(HOLD))], "p1", "f1", settle=timedelta(seconds=30))
    flight.instruct(at(0))
    line = status_line(flight, reference, 60, at(10))
    assert "instruction 1/1 (settling)" in line
    assert "nothing scored yet" in line


def test_status_names_a_group_only_after_enough_scored_time(reference):
    flight = LiveFlight([Instruction("hold", dict(HOLD))], "p1", "f1", settle=timedelta(0))
    flight.instruct(at(0))
    telemetry = wobbly_flight(2000)
    telemetry[["roll", "pitch", "yaw", "g_force"]] = 0.0
    rows = telemetry.to_dict("records")
    for i, values in enumerate(rows[:400]):
        flight.add_sample(values, at(i * 0.05))
    assert "provisional" in status_line(flight, reference, 60, at(20))

    for i, values in enumerate(rows[400:], start=400):
        flight.add_sample(values, at(i * 0.05))
    line = status_line(flight, reference, 60, at(100))
    expected = reference.assign(flight.pilot_features())["group"].iloc[0]
    assert f"group {expected} |" in line


def test_instructions_are_logged_with_their_targets(tmp_path):
    flight = LiveFlight([Instruction("hold 270", dict(HOLD))], "p1", "f1")
    instruction = flight.instruct(at(0))
    row = instruction_row(flight, instruction, at(0))
    assert row["number"] == 1
    assert row["instruction"] == "hold 270"
    assert row["target_heading"] == 270.0
    assert instructions_path(tmp_path / "live.csv").name == "live_instructions.csv"
