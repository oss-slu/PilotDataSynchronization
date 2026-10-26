"""
Show a pilot's performance group while they fly (docs/adr/0004).

Listens for the relay's mirror (or Connect TCP), assembles samples the way the
capture logger does, attaches the brief's targets as the operator marks each
instruction, and writes every sample to CSV. Every few seconds it prints the
pilot's deviation features and the nearest group of a reference cohort. The
cohort is synthetic until real pilots have targets, so the group demonstrates
the pipeline rather than measures the pilot.

Usage (from inference/, after uv sync):
    uv run python -m pilot_inference.performance.live_groups --brief briefs/example_level_holds.csv --pilot-id pilot-07

Press Enter as each instruction is spoken, and type q then Enter to stop.
"""

from __future__ import annotations

import argparse
import csv
import math
import os
import queue
import socket
import threading
import warnings
from datetime import datetime, timedelta, timezone
from pathlib import Path

import pandas as pd

from pilot_inference.capture.data_logger import (
    DEFAULT_PILOT_ID,
    SampleBuffer,
    ensure_csv,
    new_flight_id,
    parse_packet,
)
from pilot_inference.paths import DATA_DIR
from pilot_inference.performance.clustering import (
    FEATURE_COLUMNS,
    CohortGroups,
    fit_reference,
    truth_agreement,
)
from pilot_inference.performance.features import METRICS, TARGET_COLUMN, compute_features
from pilot_inference.performance.generate_synthetic_pilots import generate_pilots
from pilot_inference.performance.live import (
    DEFAULT_SETTLE,
    LIVE_CSV_FIELDS,
    Instruction,
    LiveFlight,
    load_brief,
)

INSTRUCTION_FIELDS = [
    "timestamp",
    "pilot_id",
    "flight_id",
    "number",
    "instruction",
    *[TARGET_COLUMN[metric] for metric in METRICS],
]

UNITS = {"altitude": "ft", "heading": "deg", "vertical_speed": "ft/min", "airspeed": "kts"}

# Port 5001 is what the relay's mirror box suggests. This replaces the capture
# logger for a live flight, since it writes the same columns and more.
DEFAULT_PORT = 5001


def now() -> datetime:
    return datetime.now(timezone.utc)


def listen(host: str, port: int) -> socket.socket:
    """
    A listening socket on host:port that nothing else can share.

    On Windows SO_REUSEADDR would let this bind a port the capture logger is
    already listening on, leaving the relay to reach either one, so there the
    port is claimed exclusively instead. Elsewhere SO_REUSEADDR only allows a
    quick restart. Exits with the likely cause if the port is taken.
    """
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    if hasattr(socket, "SO_EXCLUSIVEADDRUSE"):
        server.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
    else:
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        server.bind((host, port))
    except OSError as error:
        server.close()
        raise SystemExit(
            f"Port {port} on {host} is in use ({error.strerror}). Is data_logger or another "
            "live_groups still running? Stop it, or pass --port and point the relay's mirror there."
        ) from None
    server.listen()
    return server


def read_relay(conn: socket.socket, samples: queue.Queue) -> None:
    """
    Turn one relay connection into (values, arrival time) pairs on `samples`.

    Does nothing slower than parsing, so it keeps up with the mirror: the
    mirror drops copies, and then closes, if its reader stops reading
    (docs/adr/0003).
    """
    sample = SampleBuffer()
    with conn, conn.makefile("r", encoding="utf-8", newline="") as reader:
        for raw_line in reader:
            parsed = parse_packet(raw_line)
            if parsed is None:
                continue
            sample.update(*parsed)
            if sample.is_complete():
                samples.put((dict(sample.values), now()))
                sample.reset()


def serve_relay(server: socket.socket, samples: queue.Queue) -> None:
    """
    Accept relay connections one after another until the server socket closes.

    A reconnect continues the same flight, as a restarted recording does.
    """
    while True:
        try:
            conn, addr = server.accept()
        except OSError:
            return
        print(f"Relay connected from {addr[0]}:{addr[1]}")
        try:
            read_relay(conn, samples)
        except ConnectionError:
            pass
        except (OSError, ValueError) as error:
            # Garbled bytes or a socket error end this connection, not the
            # listener: the mirror never reconnects by itself (docs/adr/0003),
            # so the next Connect mirror must still find someone listening.
            print(f"Relay connection failed: {error}")
        print("Relay disconnected. Press Connect mirror again to resume.")


def _cell(value) -> str:
    if isinstance(value, float):
        return "" if math.isnan(value) else f"{value:.6f}"
    return str(value)


class CsvLog:
    """Rows appended to a CSV and flushed one at a time, like the capture logger."""

    def __init__(self, path: Path, fields: list[str]):
        ensure_csv(path, fields)
        self.path = path
        self._handle = path.open("a", newline="", encoding="utf-8")
        self._writer = csv.DictWriter(self._handle, fieldnames=fields)

    def write(self, row: dict) -> None:
        self._writer.writerow({name: _cell(value) for name, value in row.items()})
        self._handle.flush()
        os.fsync(self._handle.fileno())

    def close(self) -> None:
        self._handle.close()


# Put on the sample queue to stop record_samples once everything before it is written.
STOP = None


def record_samples(
    samples: queue.Queue, flight: LiveFlight, lock: threading.Lock, log: CsvLog
) -> None:
    """Score and write each sample as it comes off the queue, until STOP."""
    while True:
        item = samples.get()
        if item is STOP:
            return
        values, at = item
        with lock:
            row = flight.add_sample(values, at)
        log.write(row)


def reference_cohort(n_pilots: int, k: int, seed: int) -> tuple[CohortGroups, float]:
    """Synthetic pilots with their groups, and how well the groups match their hidden tiers."""
    telemetry, truth = generate_pilots(n_pilots=n_pilots, seed=seed)
    _, pilots = compute_features(telemetry)
    reference = fit_reference(pilots, k, seed)
    return reference, truth_agreement(reference.assignments, truth)


def describe_groups(reference: CohortGroups) -> str:
    """The cohort's groups as raw-unit centres, one line per group."""
    lines = []
    for centre in reference.centres.itertuples(index=False):
        parts = [
            f"{metric} std {getattr(centre, f'{metric}_std'):.1f} "
            f"|bias| {getattr(centre, f'{metric}_abs_bias'):.1f} {UNITS[metric]}"
            for metric in METRICS
        ]
        lines.append(f"  group {centre.group} ({centre.pilots} pilots): " + ", ".join(parts))
    return "\n".join(lines)


def status_line(
    flight: LiveFlight, reference: CohortGroups, min_scored: float, at: datetime
) -> str:
    """One console line: where the brief is, how much is scored, and the group if there is one."""
    if flight.current is None:
        return "Waiting for the first instruction. Press Enter as you give it."
    progress = f"instruction {flight.given}/{len(flight.brief)}"
    settling = flight.settling(at)
    if len(settling) == len(METRICS):
        progress += " (settling)"
    elif settling:
        progress += f" (settling {', '.join(settling)})"
    scored = f"scored {flight.scored_seconds:.0f}s"
    with warnings.catch_warnings():
        # A metric with nothing scored yet warns; the line already says so.
        warnings.simplefilter("ignore", UserWarning)
        pilots = flight.pilot_features()
    if pilots is None:
        return f"{progress}, nothing scored yet"
    features = ", ".join(
        f"{metric} std {pilots[f'{metric}_std'].iloc[0]:.1f} "
        f"|bias| {pilots[f'{metric}_abs_bias'].iloc[0]:.1f}"
        for metric in METRICS
    )
    if flight.scored_seconds < min_scored or pilots[FEATURE_COLUMNS].isna().to_numpy().any():
        group = f"provisional, {min_scored:.0f}s scored needed for a group"
    else:
        group = f"group {reference.assign(pilots)['group'].iloc[0]}"
    return f"{progress}, {scored}, {group} | {features}"


def print_status(
    flight: LiveFlight,
    reference: CohortGroups,
    lock: threading.Lock,
    min_scored: float,
    interval: float,
    stop: threading.Event,
) -> None:
    while not stop.wait(interval):
        with lock:
            line = status_line(flight, reference, min_scored, now())
        print(f"[{now().astimezone():%H:%M:%S}] {line}")


def instruction_row(flight: LiveFlight, instruction: Instruction, at: datetime) -> dict:
    return {
        "timestamp": at.isoformat(timespec="milliseconds"),
        "pilot_id": flight.pilot_id,
        "flight_id": flight.flight_id,
        "number": flight.given,
        "instruction": instruction.text,
        # The targets in force after it, so a turn's row still shows the altitude it keeps.
        **{TARGET_COLUMN[metric]: value for metric, value in flight.current_targets.items()},
    }


def instructions_path(csv_path: Path) -> Path:
    return csv_path.with_name(f"{csv_path.stem}_instructions.csv")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--brief", type=Path, required=True, help="Brief CSV: instruction plus one target per factor"
    )
    parser.add_argument("--host", default="127.0.0.1", help="Host interface to bind")
    parser.add_argument(
        "--port", type=int, default=DEFAULT_PORT, help="TCP port the relay connects to"
    )
    parser.add_argument("--pilot-id", default=DEFAULT_PILOT_ID, help="Identifier for the pilot flying")
    parser.add_argument(
        "--flight-id", default=None, help="Identifier for this flight; defaults to the start time"
    )
    parser.add_argument(
        "--csv",
        type=Path,
        default=DATA_DIR / "live_flight_data.csv",
        help="Samples with targets; instructions go to <name>_instructions.csv beside it",
    )
    parser.add_argument(
        "--settle",
        type=float,
        default=DEFAULT_SETTLE.total_seconds(),
        help="Seconds after each instruction that are not scored",
    )
    parser.add_argument(
        "--min-scored", type=float, default=60.0, help="Scored seconds before naming a group"
    )
    parser.add_argument("--interval", type=float, default=5.0, help="Seconds between status lines")
    parser.add_argument("--k", type=int, default=3, help="Number of performance groups")
    parser.add_argument("--seed", type=int, default=0, help="Seed for the synthetic reference cohort")
    parser.add_argument(
        "--cohort-pilots", type=int, default=15, help="Synthetic pilots in the reference cohort"
    )
    return parser


def main(argv=None) -> None:
    args = build_parser().parse_args(argv)
    brief = load_brief(args.brief)
    # Before the cohort fit and the CSVs, so a taken port fails at once and leaves nothing behind.
    server = listen(args.host, args.port)
    flight = LiveFlight(
        brief,
        args.pilot_id,
        args.flight_id or new_flight_id(),
        settle=timedelta(seconds=args.settle),
    )

    print(f"Fitting {args.k} groups on {args.cohort_pilots} synthetic pilots (seed {args.seed})...")
    reference, agreement = reference_cohort(args.cohort_pilots, args.k, args.seed)
    print("Performance groups of the synthetic reference cohort (a demonstration, not a measurement):")
    print(describe_groups(reference))
    print(f"  agreement with the synthetic pilots' hidden tiers (adjusted Rand): {agreement:.2f}")

    samples_log = CsvLog(args.csv, LIVE_CSV_FIELDS)
    instructions_log = CsvLog(instructions_path(args.csv), INSTRUCTION_FIELDS)
    lock = threading.Lock()
    stop = threading.Event()
    samples: queue.Queue = queue.Queue()

    recorder = threading.Thread(
        target=record_samples, args=(samples, flight, lock, samples_log), daemon=True
    )
    threads = [
        threading.Thread(target=serve_relay, args=(server, samples), daemon=True),
        recorder,
        threading.Thread(
            target=print_status,
            args=(flight, reference, lock, args.min_scored, args.interval, stop),
            daemon=True,
        ),
    ]
    for thread in threads:
        thread.start()

    print(f"Listening for the relay on {args.host}:{args.port}")
    print(f"Pilot {flight.pilot_id}, flight {flight.flight_id}")
    print(f"Writing samples to {args.csv} and instructions to {instructions_log.path}")
    if flight.pilot_id == DEFAULT_PILOT_ID:
        print("Pass --pilot-id to label these rows, or they cannot be grouped per pilot.")
    print(f"Brief: {len(brief)} instruction(s).")
    print("Press Enter as you give each one, q then Enter to stop.")
    print(f"  next: {brief[0].text}")

    try:
        while True:
            if input().strip().lower() == "q":
                break
            at = now()
            with lock:
                instruction = flight.instruct(at)
                if instruction is not None:
                    row = instruction_row(flight, instruction, at)
                    settling = flight.settling(at)
            if instruction is None:
                print("No instructions left in the brief. The last one stays in force; q to stop.")
                continue
            instructions_log.write(row)
            print(f"Instruction {flight.given}/{len(brief)} given: {instruction.text}")
            if not settling:
                print("  no target changed, so nothing settles")
            # Group by whole seconds left, since an earlier window may still be running.
            left = {metric: math.ceil(seconds) for metric, seconds in settling.items()}
            for seconds in sorted(set(left.values()), reverse=True):
                names = ", ".join(m for m, value in left.items() if value == seconds)
                print(f"  {names} scored again in {seconds}s")
            if flight.given < len(brief):
                print(f"  next: {brief[flight.given].text}")
    except (EOFError, KeyboardInterrupt):
        pass
    finally:
        stop.set()
        server.close()
        # Write out whatever the reader has already queued before closing the files.
        samples.put(STOP)
        recorder.join()
        samples_log.close()
        instructions_log.close()
        with lock:
            print(f"Stopped. {flight.scored_seconds:.0f}s scored over {flight.given} instruction(s).")


if __name__ == "__main__":
    main()
