"""
Deviation features for one flight while it is being flown (docs/adr/0004).

For flights we fly ourselves we are the instructor, so we know the targets: a
brief lists the instructions, each with a target for every flight dynamics
factor, and the operator marks each one as it is spoken. LiveFlight attaches
the current instruction's targets to every sample, leaving them blank for a
settle window so the turn or level-off is not scored, and summarises the
scored samples with the same functions the offline pipeline uses. Pure, with
time passed in; live_groups.py is the CLI that owns the socket, threads and
console.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from datetime import datetime, timedelta
from pathlib import Path

import pandas as pd

from pilot_inference.capture.data_logger import CSV_FIELDS, TELEMETRY_FIELDS
from pilot_inference.performance.features import METRICS, TARGET_COLUMN, compute_features

# The telemetry the capture logger writes, plus the target in force for each
# sample, so a saved flight goes straight into compute_features.
LIVE_CSV_FIELDS = CSV_FIELDS + [TARGET_COLUMN[metric] for metric in METRICS]

BRIEF_COLUMNS = ["instruction", *METRICS]

# Synthetic pilots fly only steady holds, so the turn or level-off after an
# instruction is left out rather than scored against them.
DEFAULT_SETTLE = timedelta(seconds=30)

# Longest gap between two scored samples that still counts as scored time. The
# stream is 20 Hz, so anything longer is a stall or dropped copies, not flying.
MAX_SAMPLE_GAP_SECONDS = 1.0


@dataclass(frozen=True)
class Instruction:
    """One line of a brief: what the pilot is told, and the targets it sets."""

    text: str
    targets: dict[str, float]


def load_brief(path: Path) -> list[Instruction]:
    """
    Read a brief: a CSV with an instruction column and one target column per
    flight dynamics factor, one row per instruction in the order given.

    Raises on a missing column, an empty brief or a blank target. A level hold
    judges all four factors, and a blank one would leave the pilot with NaN
    features that cannot be placed in a group.
    """
    brief = pd.read_csv(path)
    missing = [c for c in BRIEF_COLUMNS if c not in brief.columns]
    if missing:
        raise ValueError(f"{path} is missing column(s): {', '.join(missing)}")
    if brief.empty:
        raise ValueError(f"{path} has no instructions")

    instructions = []
    for number, row in enumerate(brief.itertuples(index=False), start=1):
        targets = {metric: float(getattr(row, metric)) for metric in METRICS}
        blank = [metric for metric, value in targets.items() if math.isnan(value)]
        if blank:
            raise ValueError(
                f"{path} instruction {number} has no target for {', '.join(blank)}; "
                "every instruction sets all four"
            )
        instructions.append(Instruction(str(row.instruction), targets))
    return instructions


class LiveFlight:
    """
    One flight in progress: the brief, which instruction is in force, and the
    samples scored against it.

    Not thread-safe; the CLI holds a lock around every call.
    """

    def __init__(
        self,
        brief: list[Instruction],
        pilot_id: str,
        flight_id: str,
        settle: timedelta = DEFAULT_SETTLE,
    ):
        if not brief:
            raise ValueError("A brief needs at least one instruction")
        self.brief = brief
        self.pilot_id = pilot_id
        self.flight_id = flight_id
        self.settle = settle
        # (when given, instruction), in the order given.
        self._history: list[tuple[datetime, Instruction]] = []
        self.scored_seconds = 0.0
        self._last_scored: tuple[datetime, int] | None = None
        self._scored_rows: list[dict] = []

    @property
    def given(self) -> int:
        """How many instructions have been given."""
        return len(self._history)

    @property
    def current(self) -> Instruction | None:
        return self._history[-1][1] if self._history else None

    @property
    def given_at(self) -> datetime | None:
        return self._history[-1][0] if self._history else None

    def instruct(self, at: datetime) -> Instruction | None:
        """
        Put the next instruction in force from `at`, and return it.

        Returns None, changing nothing, once the brief is used up, so the last
        instruction stays in force until the flight ends.
        """
        if self.given >= len(self.brief):
            return None
        instruction = self.brief[self.given]
        self._history.append((at, instruction))
        return instruction

    def _in_force(self, at: datetime) -> int | None:
        """Index into the history of the instruction in force at `at`, if any."""
        for index in range(len(self._history) - 1, -1, -1):
            if self._history[index][0] <= at:
                return index
        return None

    def targets_at(self, at: datetime) -> dict[str, float]:
        """
        The targets in force at `at`: NaN before the first instruction and while settling.

        Goes by `at`, not by which instruction is newest, so a sample that
        arrived just before an instruction but is handled just after it is
        still judged against the one before.
        """
        index = self._in_force(at)
        if index is None:
            return {metric: math.nan for metric in METRICS}
        given_at, instruction = self._history[index]
        if at < given_at + self.settle:
            return {metric: math.nan for metric in METRICS}
        return dict(instruction.targets)

    def add_sample(self, values: dict[str, float], at: datetime) -> dict:
        """
        Record one sample that arrived at `at`, and return it as a LIVE_CSV_FIELDS row.

        values has one entry per TELEMETRY_FIELDS column. Targets in the row
        are NaN where nothing is being scored.
        """
        targets = self.targets_at(at)
        row = {
            "timestamp": at.isoformat(timespec="milliseconds"),
            "pilot_id": self.pilot_id,
            "flight_id": self.flight_id,
            **{name: values[name] for name in TELEMETRY_FIELDS},
            **{TARGET_COLUMN[metric]: targets[metric] for metric in METRICS},
        }
        # Instructions set all four targets together, so one check covers them all.
        if not math.isnan(targets[METRICS[0]]):
            index = self._in_force(at)
            # Time across an instruction change is not scored time.
            if self._last_scored is not None and self._last_scored[1] == index:
                gap = (at - self._last_scored[0]).total_seconds()
                self.scored_seconds += min(max(gap, 0.0), MAX_SAMPLE_GAP_SECONDS)
            self._last_scored = (at, index)
            self._scored_rows.append(row)
        return row

    def pilot_features(self) -> pd.DataFrame | None:
        """
        The pilot's deviation features so far, shaped like
        features.pilot_features output, or None before anything is scored.
        """
        if not self._scored_rows:
            return None
        _, pilots = compute_features(pd.DataFrame(self._scored_rows))
        return pilots
