"""Tests for the telemetry logger's CSV output, including pilot and flight ids."""

import csv
import socket
import sys
import threading
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "Data"))

import data_logger  # noqa: E402


EVENT_ORDER = [
    ("AltitudeSync", 1200.0),
    ("HeadingSync", 90.5),
    ("VerticalVelocitySync", 450.0),
    ("AirspeedSync", 135.2),
    ("RollSync", 2.1),
    ("PitchSync", -1.4),
    ("YawSync", 91.0),
    ("GForceSync", 1.03),
]


def packet(event: str, value: float) -> str:
    return f"E;1;PilotDataSync;;;;;{event};{value};{value}\r\n"


def test_csv_fields_start_with_identity_columns():
    assert data_logger.CSV_FIELDS[:3] == ["timestamp", "pilot_id", "flight_id"]
    assert data_logger.CSV_FIELDS[3:] == data_logger.TELEMETRY_FIELDS


def test_sample_is_complete_ignores_identity_columns():
    sample = data_logger.SampleBuffer()
    for event, value in EVENT_ORDER:
        sample.update(event, value)
    assert sample.is_complete()

    row = sample.to_row("p1", "f1")
    assert row["pilot_id"] == "p1"
    assert row["flight_id"] == "f1"
    assert set(row) == set(data_logger.CSV_FIELDS)


def test_header_is_written_once(tmp_path):
    csv_path = tmp_path / "telemetry.csv"
    data_logger.ensure_csv(csv_path)
    data_logger.ensure_csv(csv_path)
    assert csv_path.read_text(encoding="utf-8").strip() == ",".join(data_logger.CSV_FIELDS)


def test_appending_to_an_older_csv_is_refused(tmp_path):
    csv_path = tmp_path / "old.csv"
    csv_path.write_text("timestamp,altitude,heading\n", encoding="utf-8")
    with pytest.raises(ValueError) as err:
        data_logger.ensure_csv(csv_path)
    assert "--csv" in str(err.value)


def test_flight_id_is_generated_when_not_supplied():
    generated = data_logger.new_flight_id()
    assert generated.endswith("Z")
    assert len(generated) == len("20260101T000000Z")


def test_logged_rows_carry_the_ids(tmp_path):
    csv_path = tmp_path / "telemetry.csv"
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.bind(("127.0.0.1", 0))
    listener.listen()
    port = listener.getsockname()[1]

    def serve_once():
        conn, _ = listener.accept()
        data_logger.ensure_csv(csv_path)
        data_logger.handle_connection(conn, csv_path, "pilot-7", "flight-3")

    server = threading.Thread(target=serve_once, daemon=True)
    server.start()

    with socket.create_connection(("127.0.0.1", port), timeout=5) as client:
        for _ in range(2):
            for event, value in EVENT_ORDER:
                client.sendall(packet(event, value).encode("ascii"))
    server.join(timeout=5)
    listener.close()

    with csv_path.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

    assert len(rows) == 2
    for row in rows:
        assert row["pilot_id"] == "pilot-7"
        assert row["flight_id"] == "flight-3"
        assert row["altitude"] == "1200.000000"
        assert row["g_force"] == "1.030000"
