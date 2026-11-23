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
from pilot_inference.events import emit
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


class RelayListener:
    """
    Accepts relay connections one after another on `server`, reading each into
    `samples` on a thread of its own, until closed.

    A reconnect continues the same flight, as a restarted recording does.
    """

    def __init__(self, server: socket.socket, samples: queue.Queue):
        self._server = server
        self._samples = samples
        self._lock = threading.Lock()
        self._conn: socket.socket | None = None
        self._closed = False
        self._thread = threading.Thread(target=self._serve, daemon=True)

    @property
    def port(self) -> int:
        return self._server.getsockname()[1]

    def start(self) -> None:
        self._thread.start()

    def close(self) -> None:
        """
        Stop accepting, end the connection in progress and wait for the reader,
        so nothing is queued once this returns.
        """
        with self._lock:
            self._closed = True
            conn = self._conn
        # On Linux and macOS closing a listening socket does not wake a thread
        # blocked in accept(); shutting it down does. Windows raises here, and
        # close() alone is enough there.
        try:
            self._server.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self._server.close()
        if conn is not None:
            # Unblocks the reader's readline, which then ends the connection itself.
            try:
                conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        self._thread.join()

    def _serve(self) -> None:
        while True:
            try:
                conn, addr = self._server.accept()
            except OSError:
                return
            with self._lock:
                if self._closed:
                    conn.close()
                    return
                self._conn = conn
            print(f"Relay connected from {addr[0]}:{addr[1]}")
            try:
                read_relay(conn, self._samples)
            except ConnectionError:
                pass
            except (OSError, ValueError) as error:
                # Garbled bytes or a socket error end this connection, not the
                # listener: the mirror never reconnects by itself (docs/adr/0003),
                # so the next Connect mirror must still find someone listening.
                if not self._closed:
                    print(f"Relay connection failed: {error}")
            with self._lock:
                self._conn = None
                if self._closed:
                    return
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


def status_payload(
    flight: LiveFlight, reference: CohortGroups, min_scored: float, at: datetime
) -> dict:
    """
    Everything a status line says, as data rather than a sentence.

    `status_line` formats this, and `--json` sends it as it is, so the console
    and the relay's window can never drift apart (see pilot_inference.events).

    `state` is one of:
        waiting         no instruction has been given yet
        nothing_scored  an instruction is in force, but no sample counts yet
        scoring         there are features; `group` is None until enough time
                        is scored, and `provisional` says which
    """
    if flight.current is None:
        return {"state": "waiting", "given": 0, "total": len(flight.brief)}

    settling = flight.settling(at)
    payload = {
        "state": "nothing_scored",
        "given": flight.given,
        "total": len(flight.brief),
        "settling": sorted(settling),
        "settling_all": len(settling) == len(METRICS),
        "scored_seconds": float(flight.scored_seconds),
        "min_scored": float(min_scored),
        "provisional": True,
        "group": None,
        "features": {},
    }

    with warnings.catch_warnings():
        # A metric with nothing scored yet warns; the payload already says so.
        warnings.simplefilter("ignore", UserWarning)
        pilots = flight.pilot_features()
    if pilots is None:
        return payload

    payload["state"] = "scoring"
    payload["features"] = {
        metric: {
            "std": float(pilots[f"{metric}_std"].iloc[0]),
            "abs_bias": float(pilots[f"{metric}_abs_bias"].iloc[0]),
        }
        for metric in METRICS
    }
    enough = flight.scored_seconds >= min_scored
    complete = not pilots[FEATURE_COLUMNS].isna().to_numpy().any()
    if enough and complete:
        payload["provisional"] = False
        payload["group"] = int(reference.assign(pilots)["group"].iloc[0])
    return payload


def status_line(
    flight: LiveFlight, reference: CohortGroups, min_scored: float, at: datetime
) -> str:
    """One console line: where the brief is, how much is scored, and the group if there is one."""
    return format_status(status_payload(flight, reference, min_scored, at))


def format_status(payload: dict) -> str:
    """The console sentence for one status payload."""
    if payload["state"] == "waiting":
        return "Waiting for the first instruction. Press Enter as you give it."

    progress = f"instruction {payload['given']}/{payload['total']}"
    settling = payload["settling"]
    if payload["settling_all"]:
        progress += " (settling)"
    elif settling:
        progress += f" (settling {', '.join(settling)})"

    if payload["state"] == "nothing_scored":
        return f"{progress}, nothing scored yet"

    scored = f"scored {payload['scored_seconds']:.0f}s"
    features = ", ".join(
        f"{metric} std {values['std']:.1f} |bias| {values['abs_bias']:.1f}"
        for metric, values in payload["features"].items()
    )
    if payload["provisional"]:
        group = f"provisional, {payload['min_scored']:.0f}s scored needed for a group"
    else:
        group = f"group {payload['group']}"
    return f"{progress}, {scored}, {group} | {features}"


class ConsoleReporter:
    """What someone reading a terminal sees. Unchanged from before --json."""

    def fitting(self, *, k, pilots, seed):
        print(f"Fitting {k} groups on {pilots} synthetic pilots (seed {seed})...")

    def cohort(self, *, reference, agreement):
        print(
            "Performance groups of the synthetic reference cohort "
            "(a demonstration, not a measurement):"
        )
        print(describe_groups(reference))
        print(f"  agreement with the synthetic pilots' hidden tiers (adjusted Rand): {agreement:.2f}")

    def started(self, *, host, port, pilot_id, flight_id, csv, instructions_csv, brief, unnamed):
        print(f"Listening for the relay on {host}:{port}")
        print(f"Pilot {pilot_id}, flight {flight_id}")
        print(f"Writing samples to {csv} and instructions to {instructions_csv}")
        if unnamed:
            print("Pass --pilot-id to label these rows, or they cannot be grouped per pilot.")
        print(f"Brief: {len(brief)} instruction(s).")
        print("Press Enter as you give each one, q then Enter to stop.")
        print(f"  next: {brief[0].text}")

    def status(self, payload, at):
        print(f"[{at.astimezone():%H:%M:%S}] {format_status(payload)}")

    def instruction(self, *, given, total, text, settling, next_text):
        print(f"Instruction {given}/{total} given: {text}")
        if not settling:
            print("  no target changed, so nothing settles")
        # Group by whole seconds left, since an earlier window may still be running.
        left = {metric: math.ceil(seconds) for metric, seconds in settling.items()}
        for seconds in sorted(set(left.values()), reverse=True):
            names = ", ".join(m for m, value in left.items() if value == seconds)
            print(f"  {names} scored again in {seconds}s")
        if next_text is not None:
            print(f"  next: {next_text}")

    def exhausted(self):
        print("No instructions left in the brief. The last one stays in force; q to stop.")

    def stopped(self, *, scored_seconds, given):
        print(f"Stopped. {scored_seconds:.0f}s scored over {given} instruction(s).")


class JsonReporter:
    """The same events as data, for the relay's window (#285)."""

    def fitting(self, *, k, pilots, seed):
        emit("fitting", k=k, cohort_pilots=pilots, seed=seed)

    def cohort(self, *, reference, agreement):
        emit("cohort", agreement=float(agreement), description=describe_groups(reference))

    def started(self, *, host, port, pilot_id, flight_id, csv, instructions_csv, brief, unnamed):
        emit(
            "started",
            host=host,
            port=port,
            pilot_id=pilot_id,
            flight_id=flight_id,
            csv=str(csv),
            instructions_csv=str(instructions_csv),
            total=len(brief),
            next=brief[0].text,
            unnamed_pilot=unnamed,
        )

    def status(self, payload, at):
        emit("status", at=at.astimezone().isoformat(), **payload)

    def instruction(self, *, given, total, text, settling, next_text):
        emit(
            "instruction",
            given=given,
            total=total,
            text=text,
            settling={metric: math.ceil(seconds) for metric, seconds in settling.items()},
            next=next_text,
        )

    def exhausted(self):
        emit("error", message="No instructions left in the brief. The last one stays in force.")

    def stopped(self, *, scored_seconds, given):
        emit("stopped", scored_seconds=float(scored_seconds), given=given)


def print_status(
    flight: LiveFlight,
    reference: CohortGroups,
    lock: threading.Lock,
    min_scored: float,
    interval: float,
    stop: threading.Event,
    reporter=None,
) -> None:
    reporter = reporter or ConsoleReporter()
    while not stop.wait(interval):
        # Copy under the lock, recompute outside it, so the recorder is never held up.
        with lock:
            snapshot = flight.snapshot()
        payload = status_payload(snapshot, reference, min_scored, now())
        reporter.status(payload, now())


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
    parser.add_argument(
        "--json",
        action="store_true",
        help="Report events as one JSON object per line instead of console text, "
        "for a program running this one (see pilot_inference.events).",
    )
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

    reporter = JsonReporter() if args.json else ConsoleReporter()

    reporter.fitting(k=args.k, pilots=args.cohort_pilots, seed=args.seed)
    reference, agreement = reference_cohort(args.cohort_pilots, args.k, args.seed)
    reporter.cohort(reference=reference, agreement=agreement)

    samples_log = CsvLog(args.csv, LIVE_CSV_FIELDS)
    instructions_log = CsvLog(instructions_path(args.csv), INSTRUCTION_FIELDS)
    lock = threading.Lock()
    stop = threading.Event()
    samples: queue.Queue = queue.Queue()

    recorder = threading.Thread(
        target=record_samples, args=(samples, flight, lock, samples_log), daemon=True
    )
    listener = RelayListener(server, samples)
    listener.start()
    threads = [
        recorder,
        threading.Thread(
            target=print_status,
            args=(flight, reference, lock, args.min_scored, args.interval, stop, reporter),
            daemon=True,
        ),
    ]
    for thread in threads:
        thread.start()

    reporter.started(
        host=args.host,
        port=args.port,
        pilot_id=flight.pilot_id,
        flight_id=flight.flight_id,
        csv=args.csv,
        instructions_csv=instructions_log.path,
        brief=brief,
        unnamed=flight.pilot_id == DEFAULT_PILOT_ID,
    )

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
                reporter.exhausted()
                continue
            instructions_log.write(row)
            reporter.instruction(
                given=flight.given,
                total=len(brief),
                text=instruction.text,
                settling=settling,
                next_text=brief[flight.given].text if flight.given < len(brief) else None,
            )
    except (EOFError, KeyboardInterrupt):
        pass
    finally:
        stop.set()
        # Stop the reader first, so nothing is queued after STOP; then write out
        # everything it queued before closing the files.
        listener.close()
        samples.put(STOP)
        recorder.join()
        samples_log.close()
        instructions_log.close()
        with lock:
            reporter.stopped(scored_seconds=flight.scored_seconds, given=flight.given)


if __name__ == "__main__":
    main()
