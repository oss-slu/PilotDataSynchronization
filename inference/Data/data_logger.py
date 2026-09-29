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

CSV_FIELDS = [
    "timestamp",
    "altitude",
    "heading",
    "vertical_speed",
    "velocity",
    "roll",
    "pitch",
    "yaw",
    "g_force",
]


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
        return all(field in self.values for field in CSV_FIELDS[1:])

    def to_row(self) -> dict[str, str]:
        row = {"timestamp": datetime.now(timezone.utc).isoformat(timespec="milliseconds")}
        for field in CSV_FIELDS[1:]:
            row[field] = f"{self.values[field]:.6f}"
        return row


def ensure_csv(csv_path: Path) -> None:
    csv_path.parent.mkdir(parents=True, exist_ok=True)
    if csv_path.exists() and csv_path.stat().st_size > 0:
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


def handle_connection(conn: socket.socket, csv_path: Path) -> None:
    sample = SampleBuffer()
    with conn, conn.makefile("r", encoding="utf-8", newline="") as reader:
        for raw_line in reader:
            parsed = parse_packet(raw_line)
            if parsed is None:
                continue

            event_name, value = parsed
            sample.update(event_name, value)

            if sample.is_complete():
                append_row(csv_path, sample.to_row())
                sample.reset()


def serve(host: str, port: int, csv_path: Path) -> None:
    ensure_csv(csv_path)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server:
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind((host, port))
        server.listen()
        print(f"Listening for relay telemetry on {host}:{port}")
        print(f"Writing CSV rows to {csv_path}")

        while True:
            conn, addr = server.accept()
            print(f"Relay connected from {addr[0]}:{addr[1]}")
            try:
                handle_connection(conn, csv_path)
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
    return parser


def main() -> None:
    args = build_parser().parse_args()
    serve(args.host, args.port, args.csv)


if __name__ == "__main__":
    main()
