#!/usr/bin/env python3
"""Collect relay telemetry packets and append to CSV."""

from __future__ import annotations

import argparse
import csv
import os
import socket
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path


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


def ensure_csv(csv_path: Path) -> None:
    csv_path.parent.mkdir(parents=True, exist_ok=True)
    if csv_path.exists() and csv_path.stat().st_size > 0:
        with csv_path.open("r", newline="", encoding="utf-8") as handle:
            header = next(csv.reader(handle), [])
        if header != CSV_FIELDS:
            raise ValueError(
                f"{csv_path} has columns {header}, but this logger writes {CSV_FIELDS}. "
                "Appending would misalign the rows. Pass --csv with a new path, or move "
                "the old file aside."
            )
        return
    with csv_path.open("a", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=CSV_FIELDS)
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

    try:
        value = float(parts[8])
    except ValueError:
        return None

    return event_name, value


def append_row(csv_path: Path, row: dict[str, str]) -> None:
    with csv_path.open("a", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=CSV_FIELDS)
        writer.writerow(row)
        handle.flush()
        os.fsync(handle.fileno())


def handle_connection(
    conn: socket.socket, csv_path: Path, pilot_id: str, flight_id: str
) -> None:
    sample = SampleBuffer()
    with conn, conn.makefile("r", encoding="utf-8", newline="") as reader:
        for raw_line in reader:
            parsed = parse_packet(raw_line)
            if parsed is None:
                continue

            event_name, value = parsed
            sample.update(event_name, value)

            if sample.is_complete():
                append_row(csv_path, sample.to_row(pilot_id, flight_id))
                sample.reset()


def serve(
    host: str,
    port: int,
    csv_path: Path,
    pilot_id: str = DEFAULT_PILOT_ID,
    flight_id: str | None = None,
) -> None:
    ensure_csv(csv_path)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server:
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind((host, port))
        server.listen()
        print(f"Listening for relay telemetry on {host}:{port}")
        print(f"Writing CSV rows to {csv_path}")
        print(f"Pilot id: {pilot_id}")
        if pilot_id == DEFAULT_PILOT_ID:
            print("Pass --pilot-id to label these rows, or they cannot be grouped per pilot.")

        while True:
            conn, addr = server.accept()
            # Each connection is one flight unless the caller named one.
            connection_flight_id = flight_id or new_flight_id()
            print(f"Relay connected from {addr[0]}:{addr[1]}, flight id {connection_flight_id}")
            try:
                handle_connection(conn, csv_path, pilot_id, connection_flight_id)
            except ConnectionError:
                pass
            finally:
                print("Relay disconnected")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Collect relay telemetry into CSV.")
    parser.add_argument("--host", default="127.0.0.1", help="Host interface to bind.")
    parser.add_argument("--port", type=int, default=5001, help="TCP port for the relay connection.")
    parser.add_argument(
        "--csv",
        type=Path,
        default=Path(__file__).resolve().parent / "raw_flight_data.csv",
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
    return parser


def main() -> None:
    args = build_parser().parse_args()
    serve(args.host, args.port, args.csv, args.pilot_id, args.flight_id)


if __name__ == "__main__":
    main()
