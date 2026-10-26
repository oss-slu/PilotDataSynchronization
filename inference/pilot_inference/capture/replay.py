"""
Send a telemetry CSV to a listener the way the relay would, to test without X-Plane.

Each row becomes the relay's eight packets in the relay's order
(packets_for_fields in relay/src/state.rs), so data_logger and live_groups read
it exactly like a live flight. A telemetry column the CSV lacks is sent as 0:
synthetic telemetry has no roll, pitch, yaw or g_force. Target columns are not
sent, because the relay carries none; --brief-out writes them as a brief to
start live_groups with, and exits without sending.

Rows are sent as they are. The committed CSVs in data/ predate #196, so their
altitude and velocity are mis-scaled (docs/telemetry_schema.md section 1);
replaying one sends those wrong values as if the plugin had.

Usage (from inference/, after uv sync):
    uv run python -m pilot_inference.capture.replay --input data/synthetic_pilot_telemetry.csv --flight P01-F1
"""

from __future__ import annotations

import argparse
import socket
import time
from pathlib import Path

import pandas as pd

from pilot_inference.performance.features import METRICS, TARGET_COLUMN

# The relay's send order, not the CSV's column order.
RELAY_EVENTS = [
    ("AltitudeSync", "altitude"),
    ("AirspeedSync", "velocity"),
    ("VerticalVelocitySync", "vertical_speed"),
    ("HeadingSync", "heading"),
    ("RollSync", "roll"),
    ("PitchSync", "pitch"),
    ("YawSync", "yaw"),
    ("GForceSync", "g_force"),
]


def packets_for_row(row: dict) -> list[str]:
    """One sample as the relay's iMotions packets, with each value sent twice as the relay does."""
    packets = []
    for event, column in RELAY_EVENTS:
        value = row.get(column, 0.0)
        packets.append(f"E;1;PilotDataSync;;;;;{event};{value:.6f};{value:.6f}\r\n")
    return packets


def select_flight(telemetry: pd.DataFrame, flight_id: str | None) -> pd.DataFrame:
    """
    The rows of one flight. Without a flight id the CSV must hold only one,
    so two flights are never sent back to back as if they were one.
    """
    if "flight_id" not in telemetry.columns:
        return telemetry
    flights = telemetry["flight_id"].astype(str).unique().tolist()
    if flight_id is None:
        if len(flights) > 1:
            shown = ", ".join(flights[:5]) + (", ..." if len(flights) > 5 else "")
            raise ValueError(f"The CSV has {len(flights)} flights ({shown}); pick one with --flight")
        return telemetry
    if flight_id not in flights:
        raise ValueError(f"No flight {flight_id!r} in the CSV")
    return telemetry[telemetry["flight_id"].astype(str) == flight_id]


def brief_for(rows: pd.DataFrame) -> pd.DataFrame:
    """
    A one-instruction brief holding the flight's targets, for a flight whose
    targets never change, as a synthetic flight's do.
    """
    brief = {"instruction": "Hold the replayed flight's targets"}
    for metric in METRICS:
        column = TARGET_COLUMN[metric]
        if column not in rows.columns:
            raise ValueError(f"No {column} column to build a brief from")
        values = rows[column].dropna().unique()
        if len(values) != 1:
            raise ValueError(f"{column} is not one constant value, so one instruction cannot hold it")
        brief[metric] = values[0]
    return pd.DataFrame([brief])


def replay(rows: pd.DataFrame, host: str, port: int, hz: float) -> int:
    """Send every row to host:port at hz samples a second (0 for as fast as possible)."""
    period = 1.0 / hz if hz > 0 else 0.0
    sent = 0
    with socket.create_connection((host, port)) as sock:
        start = time.monotonic()
        for row in rows.to_dict("records"):
            sock.sendall("".join(packets_for_row(row)).encode("utf-8"))
            sent += 1
            if period:
                # Pace against the start, so a slow send does not stretch the flight.
                delay = start + sent * period - time.monotonic()
                if delay > 0:
                    time.sleep(delay)
    return sent


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--input", type=Path, required=True, help="Telemetry CSV to send")
    parser.add_argument("--flight", default=None, help="flight_id to send, if the CSV has several")
    parser.add_argument("--host", default="127.0.0.1", help="Listener host")
    parser.add_argument("--port", type=int, default=5001, help="Listener port")
    parser.add_argument(
        "--hz", type=float, default=20.0, help="Samples a second; 0 sends as fast as possible"
    )
    parser.add_argument(
        "--brief-out",
        type=Path,
        default=None,
        help="Write the flight's targets as a brief CSV and exit, without sending",
    )
    args = parser.parse_args(argv)

    rows = select_flight(pd.read_csv(args.input), args.flight)
    if args.brief_out:
        brief_for(rows).to_csv(args.brief_out, index=False)
        print(f"Wrote a brief with this flight's targets to {args.brief_out}")
        return
    print(f"Sending {len(rows)} samples to {args.host}:{args.port} at {args.hz:g} Hz")
    try:
        sent = replay(rows, args.host, args.port, args.hz)
    except ConnectionRefusedError:
        raise SystemExit(f"Nothing is listening on {args.host}:{args.port}. Start the listener first.")
    except ConnectionError:
        raise SystemExit("The listener closed the connection before the flight was fully sent.")
    print(f"Sent {sent} samples")


if __name__ == "__main__":
    main()
