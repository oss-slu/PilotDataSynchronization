#!/usr/bin/env python3
"""Collect relay telemetry packets and append to CSV."""

from __future__ import annotations

import argparse
import csv
import os
import socket
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

from pilot_inference.events import emit
from pilot_inference.paths import DATA_DIR

# How often a `--json` run reports how many samples it has written.
#
# The relay's window shows that number, and "recording, 412 samples" is the
# answer to the only question anyone asks mid-session. Two seconds is often
# enough to look live and rare enough to cost nothing against a 20 Hz stream.
STATUS_EVERY_SECONDS = 2.0


EVENT_TO_COLUMN = {
    "AltitudeSync": "altitude",
    "HeadingSync": "heading",
    "VerticalVelocitySync": "vertical_speed",
    "AirspeedSync": "velocity",
    "RollSync": "roll",
    "PitchSync": "pitch",
    "YawSync": "yaw",
    "GForceSync": "g_force",
}

# The telemetry the plugin sends, one value per column.
TELEMETRY_FIELDS = [
    "altitude",
    "heading",
    "vertical_speed",
    "velocity",
    "roll",
    "pitch",
    "yaw",
    "g_force",
]

# pilot_id and flight_id identify who flew and which flight, so samples can be
# grouped per pilot and joined to pilot metadata. Both are constant for a
# flight, so they are recorded here rather than sent through the 20 Hz stream.
CSV_FIELDS = ["timestamp", "pilot_id", "flight_id", *TELEMETRY_FIELDS]

DEFAULT_PILOT_ID = "unknown"

# A session file of its own, not the committed raw_flight_data.csv: that one was
# logged before these columns existed, and before the #196 unit fix, so appending
# to it would both misalign the header and mix two scales in one file. Copy this
# file over raw_flight_data.csv to feed it into pilot_inference.event_classification.label_generator.
DEFAULT_CSV_NAME = "session_flight_data.csv"


@dataclass
class SampleBuffer:
    values: dict[str, float] = field(default_factory=dict)

    def reset(self) -> None:
        self.values.clear()

    def update(self, event_name: str, value: float) -> None:
        column = EVENT_TO_COLUMN.get(event_name)
        if column is None:
            return

        if event_name == "AltitudeSync" and self.values:
            self.reset()

        self.values[column] = value

    def is_complete(self) -> bool:
        return all(field in self.values for field in TELEMETRY_FIELDS)

    def to_row(self, pilot_id: str, flight_id: str) -> dict[str, str]:
        row = {
            "timestamp": datetime.now(timezone.utc).isoformat(timespec="milliseconds"),
            "pilot_id": pilot_id,
            "flight_id": flight_id,
        }
        for field in TELEMETRY_FIELDS:
            row[field] = f"{self.values[field]:.6f}"
        return row


def new_flight_id() -> str:
    """A flight id for one relay connection, when the caller does not supply one."""
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def ensure_csv(csv_path: Path, fields: list[str] = CSV_FIELDS) -> None:
    csv_path.parent.mkdir(parents=True, exist_ok=True)
    if csv_path.exists() and csv_path.stat().st_size > 0:
        with csv_path.open("r", newline="", encoding="utf-8") as handle:
            header = next(csv.reader(handle), [])
        if header != fields:
            raise ValueError(
                f"{csv_path} has columns {header}, but these rows have {fields}. "
                "Appending would misalign the rows. Pass --csv with a new path, or move "
                "the old file aside."
            )
        return
    with csv_path.open("a", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        handle.flush()
        os.fsync(handle.fileno())


def parse_packet(line: str) -> tuple[str, float] | None:
    parts = line.strip().split(";")
    if len(parts) < 10 or parts[0] != "E" or parts[2] != "PilotDataSync":
        return None

    event_name = parts[7]
    if event_name not in EVENT_TO_COLUMN:
        return None

    # Each entry carries two values, FlightModel* then Pilot*. The dataset is
    # defined as pilot-side instrument readings, carrying simulated sensor
    # error rather than the aircraft's true state (see GLOSSARY.md, "Pilot
    # reading"), so the second one is the one to record. Before #184 the relay
    # sent the pilot value in both slots, which made the first slot look
    # correct; it carries the flightmodel value now.
    try:
        value = float(parts[9])
    except ValueError:
        return None

    return event_name, value


def append_row(csv_path: Path, row: dict[str, str]) -> None:
    with csv_path.open("a", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=CSV_FIELDS)
        writer.writerow(row)
        handle.flush()
        os.fsync(handle.fileno())


class ConsoleReporter:
    """
    What this module has always printed, unchanged.

    `status` prints nothing, because it never did: the console shows the
    connection, not a running count. Keeping it a no-op is what lets the
    `--json` run report progress without changing a line of console output.
    """

    def started(self, host: str, port: int, csv_path: Path, pilot_id: str) -> None:
        print(f"Listening for relay telemetry on {host}:{port}")
        print(f"Writing CSV rows to {csv_path}")
        print(f"Pilot id: {pilot_id}")
        if pilot_id == DEFAULT_PILOT_ID:
            print("Pass --pilot-id to label these rows, or they cannot be grouped per pilot.")

    def connected(self, addr: tuple[str, int], flight_id: str) -> None:
        print(f"Relay connected from {addr[0]}:{addr[1]}, flight id {flight_id}")

    def status(self, state: str, rows: int, flight_id: str) -> None:
        pass

    def disconnected(self, rows: int) -> None:
        print("Relay disconnected")


class JsonReporter:
    """
    The same run, reported as events for the program running this one.

    The relay's window reads these (#285). The contract lives in
    `pilot_inference.events`.
    """

    def started(self, host: str, port: int, csv_path: Path, pilot_id: str) -> None:
        emit(
            "started",
            host=host,
            port=port,
            csv=csv_path,
            pilot_id=pilot_id,
            unnamed_pilot=pilot_id == DEFAULT_PILOT_ID,
        )
        emit("status", state="waiting_for_relay", rows=0)

    def connected(self, addr: tuple[str, int], flight_id: str) -> None:
        emit("status", state="recording", rows=0, flight_id=flight_id, peer=addr[0])

    def status(self, state: str, rows: int, flight_id: str) -> None:
        emit("status", state=state, rows=rows, flight_id=flight_id)

    def disconnected(self, rows: int) -> None:
        emit("status", state="relay_gone", rows=rows)


def handle_connection(
    conn: socket.socket,
    csv_path: Path,
    pilot_id: str,
    flight_id: str,
    reporter: ConsoleReporter | JsonReporter | None = None,
) -> int:
    """Write every complete sample this connection sends. Returns the count."""
    sample = SampleBuffer()
    rows = 0
    next_status = time.monotonic() + STATUS_EVERY_SECONDS
    with conn, conn.makefile("r", encoding="utf-8", newline="") as reader:
        # Caught here rather than by the caller, so a relay that drops mid
        # flight still reports how much of it was written.
        try:
            for raw_line in reader:
                parsed = parse_packet(raw_line)
                if parsed is None:
                    continue

                event_name, value = parsed
                sample.update(event_name, value)

                if sample.is_complete():
                    append_row(csv_path, sample.to_row(pilot_id, flight_id))
                    sample.reset()
                    rows += 1

                # Checked here rather than on a timer thread: this loop runs
                # on every packet, so the clock is already being read often.
                if reporter is not None:
                    now = time.monotonic()
                    if now >= next_status:
                        reporter.status("recording", rows, flight_id)
                        next_status = now + STATUS_EVERY_SECONDS
        except ConnectionError:
            pass
    return rows


def serve(
    host: str,
    port: int,
    csv_path: Path,
    pilot_id: str = DEFAULT_PILOT_ID,
    flight_id: str | None = None,
    reporter: ConsoleReporter | JsonReporter | None = None,
) -> None:
    reporter = reporter if reporter is not None else ConsoleReporter()
    ensure_csv(csv_path)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server:
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind((host, port))
        server.listen()
        reporter.started(host, port, csv_path, pilot_id)

        while True:
            conn, addr = server.accept()
            # Each connection is one flight unless the caller named one.
            connection_flight_id = flight_id or new_flight_id()
            reporter.connected(addr, connection_flight_id)
            rows = handle_connection(conn, csv_path, pilot_id, connection_flight_id, reporter)
            reporter.disconnected(rows)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Collect relay telemetry into CSV.")
    parser.add_argument("--host", default="127.0.0.1", help="Host interface to bind.")
    parser.add_argument("--port", type=int, default=5001, help="TCP port for the relay connection.")
    parser.add_argument(
        "--csv",
        type=Path,
        default=DATA_DIR / DEFAULT_CSV_NAME,
        help="Output CSV path.",
    )
    parser.add_argument(
        "--pilot-id",
        default=DEFAULT_PILOT_ID,
        help="Identifier for the pilot flying, written to every row.",
    )
    parser.add_argument(
        "--flight-id",
        default=None,
        help="Identifier for this flight. Defaults to a UTC timestamp per relay connection.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Report events as one JSON object per line instead of console text, "
        "for a program running this one (see pilot_inference.events).",
    )
    return parser


def main() -> None:
    args = build_parser().parse_args()
    reporter = JsonReporter() if args.json else ConsoleReporter()
    try:
        serve(args.host, args.port, args.csv, args.pilot_id, args.flight_id, reporter)
    except KeyboardInterrupt:
        # Ctrl-C at a console, and the nearest thing to a clean stop until
        # #277 gives this module one. Every row is already flushed and fsynced
        # as it arrives, so nothing written is at risk either way.
        if args.json:
            emit("stopped")


if __name__ == "__main__":
    main()
