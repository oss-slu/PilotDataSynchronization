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
    RelayListener,
    STOP,
    CsvLog,
    instruction_row,
    instructions_path,
    main,
    record_samples,
    reference_cohort,
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
    samples: queue.Queue = queue.Queue()
    listener = RelayListener(listening_socket(), samples)
    listener.start()
    flight = wobbly_flight(25)

    replay(flight, "127.0.0.1", listener.port, hz=0)

    received = [samples.get(timeout=5) for _ in range(len(flight))]
    listener.close()
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


def test_a_busy_port_stops_before_anything_is_written(tmp_path):
    # Held the way the capture logger holds it.
    occupant = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    occupant.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    occupant.bind(("127.0.0.1", 0))
    occupant.listen()
    port = occupant.getsockname()[1]
    brief = tmp_path / "brief.csv"
    brief.write_text("instruction,heading,altitude,airspeed,vertical_speed\nhold,270,3000,100,0\n")
    csv_path = tmp_path / "live.csv"

    with pytest.raises(SystemExit) as stopped:
        main(["--brief", str(brief), "--port", str(port), "--csv", str(csv_path)])
    occupant.close()

    assert f"Port {port}" in str(stopped.value)
    assert "data_logger" in str(stopped.value)
    assert not csv_path.exists()
    assert not instructions_path(csv_path).exists()


def test_a_garbled_connection_does_not_stop_the_next_one():
    samples: queue.Queue = queue.Queue()
    listener = RelayListener(listening_socket(), samples)
    listener.start()
    port = listener.port

    with socket.create_connection(("127.0.0.1", port)) as garbled:
        garbled.sendall(b"\xff\xfe not utf-8\r\n")
    replay(wobbly_flight(3), "127.0.0.1", port, hz=0)

    received = [samples.get(timeout=5) for _ in range(3)]
    listener.close()
    assert len(received) == 3


def test_status_names_the_factors_still_settling(reference):
    brief = [Instruction("hold 270", dict(HOLD)), Instruction("turn 360", {"heading": 360.0})]
    flight = LiveFlight(brief, "p1", "f1", settle=timedelta(seconds=10))
    flight.instruct(at(0))
    flight.instruct(at(20))
    line = status_line(flight, reference, 60, at(25))
    assert "instruction 2/2 (settling heading)" in line


def test_a_logged_instruction_carries_its_unchanged_targets(tmp_path):
    brief = [Instruction("hold 270", dict(HOLD)), Instruction("turn 360", {"heading": 360.0})]
    flight = LiveFlight(brief, "p1", "f1")
    flight.instruct(at(0))
    turn = flight.instruct(at(20))
    row = instruction_row(flight, turn, at(20))
    assert row["target_heading"] == 360.0
    assert row["target_altitude"] == 3000.0


def log_flight(log: CsvLog, flight: LiveFlight, telemetry: pd.DataFrame) -> None:
    flight.instruct(at(0))
    for i, values in enumerate(telemetry.to_dict("records")):
        log.write(flight.add_sample(values, at(i * 0.05)))


def test_an_appended_csv_still_gives_each_flight_its_live_features(tmp_path):
    telemetry = wobbly_flight(120)
    telemetry[["roll", "pitch", "yaw", "g_force"]] = 0.0
    log = CsvLog(tmp_path / "live.csv", LIVE_CSV_FIELDS)
    earlier = LiveFlight([Instruction("hold", dict(HOLD, altitude=2000.0))], "p1", "f1", timedelta(0))
    log_flight(log, earlier, telemetry)
    later = LiveFlight([Instruction("hold", dict(HOLD))], "p1", "f2", timedelta(0))
    log_flight(log, later, telemetry)
    log.close()

    offline, _ = compute_features(pd.read_csv(tmp_path / "live.csv"))
    columns = [f"{m}_{s}" for m in METRICS for s in ("mad", "std", "bias", "abs_bias")]
    # The pilot's offline features average both flights; the flight's own row is the live one.
    pd.testing.assert_frame_equal(
        offline.loc[offline["flight_id"] == "f2", columns].reset_index(drop=True),
        later.pilot_features()[columns],
        check_exact=False,
    )


def test_a_group_stays_named_while_one_factor_settles(reference):
    brief = [Instruction("hold 270", dict(HOLD)), Instruction("turn 360", {"heading": 360.0})]
    flight = LiveFlight(brief, "p1", "f1", settle=timedelta(seconds=30))
    flight.instruct(at(0))
    # 100 s of flying, 70 s of it scored once the first settle window ends.
    telemetry = wobbly_flight(2000)
    telemetry[["roll", "pitch", "yaw", "g_force"]] = 0.0
    for i, values in enumerate(telemetry.to_dict("records")):
        flight.add_sample(values, at(i * 0.05))
    flight.instruct(at(100))
    line = status_line(flight, reference, 60, at(105))
    assert "(settling heading)" in line
    assert "provisional" not in line
    assert "group " in line


def test_closing_the_listener_stops_a_connected_relay():
    samples: queue.Queue = queue.Queue()
    listener = RelayListener(listening_socket(), samples)
    listener.start()

    def stream_until_cut_off():
        try:
            replay(wobbly_flight(100_000), "127.0.0.1", listener.port, 200)
        except ConnectionError:
            pass  # what the relay sees when the sidecar closes

    relay = threading.Thread(target=stream_until_cut_off, daemon=True)
    relay.start()
    samples.get(timeout=5)  # the relay is connected and streaming

    listener.close()
    queued = samples.qsize()
    relay.join(timeout=5)
    # Nothing arrives once close() returns, even though the relay was still sending.
    assert samples.qsize() == queued
    assert not relay.is_alive()


def test_closing_the_listener_with_no_relay_returns():
    listener = RelayListener(listening_socket(), queue.Queue())
    listener.start()
    closer = threading.Thread(target=listener.close, daemon=True)
    closer.start()
    closer.join(timeout=5)
    assert not closer.is_alive()
