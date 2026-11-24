"""Tests for the telemetry logger's CSV output, including pilot and flight ids."""

import csv
import socket
import threading

import pytest

from pilot_inference.capture import data_logger
from pilot_inference.paths import DATA_DIR


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


def test_parse_packet_takes_the_pilot_half_not_the_flightmodel_half():
    """
    #184: each entry carries FlightModel* then Pilot*. The dataset is defined
    as pilot-side instrument readings, so the second value is the one logged.
    The helper above sends the same number twice, as a pre-#184 relay did, so
    it cannot tell the two apart -- this uses values that differ.
    """
    line = "E;1;PilotDataSync;;;;;AltitudeSync;1000.0;1234.5\r\n"
    assert data_logger.parse_packet(line) == ("AltitudeSync", 1234.5)


def test_parse_packet_rejects_an_entry_missing_the_pilot_half():
    """A truncated entry is dropped rather than logged as flightmodel data."""
    assert data_logger.parse_packet("E;1;PilotDataSync;;;;;AltitudeSync;1000.0\r\n") is None


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


def test_default_output_is_a_session_file_not_a_committed_csv():
    default = data_logger.build_parser().parse_args([]).csv
    assert default == DATA_DIR / data_logger.DEFAULT_CSV_NAME
    # The committed CSVs were logged before pilot_id and flight_id existed, so a
    # default pointing at one makes the documented command fail on a fresh clone.
    assert default.name not in {"raw_flight_data.csv", "raw_flight_data_updated.csv"}


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


# --- Reporting a run to whatever is running it (#285) ------------------------


def test_handle_connection_counts_the_samples_it_wrote(tmp_path):
    """The relay's window shows this number, so it has to be the real one."""
    csv_path = tmp_path / "telemetry.csv"
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.bind(("127.0.0.1", 0))
    listener.listen()
    port = listener.getsockname()[1]
    written = []

    def serve_once():
        conn, _ = listener.accept()
        data_logger.ensure_csv(csv_path)
        written.append(data_logger.handle_connection(conn, csv_path, "p", "f"))

    server = threading.Thread(target=serve_once, daemon=True)
    server.start()
    with socket.create_connection(("127.0.0.1", port), timeout=5) as client:
        for _ in range(3):
            for event, value in EVENT_ORDER:
                client.sendall(packet(event, value).encode("ascii"))
    server.join(timeout=5)
    listener.close()

    assert written == [3]


def test_the_console_reporter_prints_what_it_always_printed(capsys):
    """
    The console output is a contract with the people who read it. `--json`
    exists so the relay does not have to scrape it, not so it can change.
    """
    data_logger.ConsoleReporter().started("127.0.0.1", 5001, "data/out.csv", "P004")
    out = capsys.readouterr().out
    assert out == (
        "Listening for relay telemetry on 127.0.0.1:5001\n"
        "Writing CSV rows to data/out.csv\n"
        "Pilot id: P004\n"
    )


def test_the_console_reporter_still_nags_about_an_unnamed_pilot(capsys):
    data_logger.ConsoleReporter().started(
        "127.0.0.1", 5001, "data/out.csv", data_logger.DEFAULT_PILOT_ID
    )
    assert "Pass --pilot-id" in capsys.readouterr().out


def test_the_console_reporter_says_nothing_on_a_status(capsys):
    """It never printed a running count, and `--json` must not change that."""
    data_logger.ConsoleReporter().status("recording", 412, "f")
    assert capsys.readouterr().out == ""


def _events(capsys):
    import json

    return [json.loads(line) for line in capsys.readouterr().out.splitlines() if line.strip()]


def test_the_json_reporter_says_it_is_waiting_before_the_relay_connects(capsys):
    data_logger.JsonReporter().started("127.0.0.1", 5001, "data/out.csv", "P004")
    started, status = _events(capsys)
    assert started["event"] == "started"
    assert started["port"] == 5001
    assert started["csv"] == "data/out.csv"
    assert started["pilot_id"] == "P004"
    assert started["unnamed_pilot"] is False
    assert status == {
        "event": "status",
        "v": started["v"],
        "state": "waiting_for_relay",
        "rows": 0,
    }


def test_the_json_reporter_reports_the_count_while_recording(capsys):
    reporter = data_logger.JsonReporter()
    reporter.connected(("127.0.0.1", 50505), "P004-T1")
    reporter.status("recording", 412, "P004-T1")
    reporter.disconnected(412)

    connected, recording, gone = _events(capsys)
    assert connected["state"] == "recording"
    assert connected["flight_id"] == "P004-T1"
    assert recording["rows"] == 412
    assert gone["state"] == "relay_gone"
    assert gone["rows"] == 412


def test_an_unnamed_pilot_is_flagged_as_data_not_as_a_sentence(capsys):
    data_logger.JsonReporter().started(
        "127.0.0.1", 5001, "data/out.csv", data_logger.DEFAULT_PILOT_ID
    )
    started = _events(capsys)[0]
    assert started["unnamed_pilot"] is True


def test_json_mode_writes_nothing_but_events_to_stdout(capsys):
    """
    The relay routes anything it cannot parse to its raw output pane, so a
    stray print is not fatal, but it is still noise where data is expected.
    """
    import json

    reporter = data_logger.JsonReporter()
    reporter.started("127.0.0.1", 5001, "data/out.csv", "P004")
    reporter.connected(("127.0.0.1", 1), "F1")
    reporter.status("recording", 1, "F1")
    reporter.disconnected(1)

    for line in capsys.readouterr().out.splitlines():
        if line.strip():
            json.loads(line)


def test_the_logger_takes_a_json_flag():
    args = data_logger.build_parser().parse_args(["--json", "--pilot-id", "P004"])
    assert args.json is True
    assert data_logger.build_parser().parse_args([]).json is False
