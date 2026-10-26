"""
Deviation features for one flight while it is being flown (docs/adr/0004).

For flights we fly ourselves we are the instructor, so we know the targets: a
brief lists the instructions with the targets each one sets, and the operator
marks each one as it is spoken. Targets an instruction does not name carry
over. LiveFlight attaches the targets in force to every sample, leaving the
ones an instruction changed blank for a settle window so the turn or level-off
is not scored, and summarises the scored samples with the same functions the
offline pipeline uses. Pure, with
time passed in; live_groups.py is the CLI that owns the socket, threads and
console.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from datetime import datetime, timedelta
from pathlib import Path

import pandas as pd

from pilot_inference.angles import angular_diff
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
    """
    One line of a brief: what the pilot is told, and the targets it sets.

    targets holds only the factors the instruction names; the others carry
    over from the instruction before.
    """

    text: str
    targets: dict[str, float]


def load_brief(path: Path) -> list[Instruction]:
    """
    Read a brief: a CSV with an instruction column and one target column per
    flight dynamics factor, one row per instruction in the order given. A blank
    target carries over from the instruction before.

    Raises on a missing column, an empty brief, or a blank target in the first
    instruction, which has nothing to carry over from and would leave the
    pilot with NaN features that cannot be placed in a group.
    """
    brief = pd.read_csv(path)
    missing = [c for c in BRIEF_COLUMNS if c not in brief.columns]
    if missing:
        raise ValueError(f"{path} is missing column(s): {', '.join(missing)}")
    if brief.empty:
        raise ValueError(f"{path} has no instructions")

    instructions = []
    for number, row in enumerate(brief.itertuples(index=False), start=1):
        given = {metric: float(getattr(row, metric)) for metric in METRICS}
        blank = [metric for metric, value in given.items() if math.isnan(value)]
        if number == 1 and blank:
            raise ValueError(
                f"{path} instruction 1 has no target for {', '.join(blank)}; "
                "the first instruction sets all four"
            )
        targets = {metric: value for metric, value in given.items() if metric not in blank}
        instructions.append(Instruction(str(row.instruction), targets))
    return instructions


def _changed(metric: str, before: float, after: float) -> bool:
    """Whether a target moved. Headings compare wrapped, so 0 after 360 is the same north."""
    if metric == "heading":
        return angular_diff(after, before) != 0
    return after != before


@dataclass(frozen=True)
class _Given:
    """
    An instruction as given: when, every target in force from then on, and
    when each target starts being scored, after its settle window.
    """

    at: datetime
    instruction: Instruction
    targets: dict[str, float]
    scored_from: dict[str, datetime]


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
        # Instructions as given, in order.
        self._history: list[_Given] = []
        self.scored_seconds_by_factor = {metric: 0.0 for metric in METRICS}
        # Per factor: when it was last scored, and when that scored stretch began.
        self._last_scored: dict[str, tuple[datetime, datetime]] = {}
        self._scored_rows: list[dict] = []

    @property
    def scored_seconds(self) -> float:
        """Scored time of the least-scored factor, which is what a group waits on."""
        return min(self.scored_seconds_by_factor.values())

    @property
    def given(self) -> int:
        """How many instructions have been given."""
        return len(self._history)

    @property
    def current(self) -> Instruction | None:
        return self._history[-1].instruction if self._history else None

    @property
    def current_targets(self) -> dict[str, float]:
        """Every target the latest instruction left in force, settling or not."""
        return dict(self._history[-1].targets) if self._history else {}

    def settling(self, at: datetime) -> dict[str, float]:
        """
        The factors still inside their settle window at `at`, with the seconds
        each has left, in METRICS order. An earlier instruction's window can
        still be running for a factor the latest one left alone.
        """
        index = self._in_force(at)
        if index is None:
            return {}
        given = self._history[index]
        return {
            metric: (given.scored_from[metric] - at).total_seconds()
            for metric in METRICS
            if at < given.scored_from[metric]
        }

    def instruct(self, at: datetime) -> Instruction | None:
        """
        Put the next instruction in force from `at`, and return it.

        Returns None, changing nothing, once the brief is used up, so the last
        instruction stays in force until the flight ends.
        """
        if self.given >= len(self.brief):
            return None
        instruction = self.brief[self.given]
        before = self._history[-1] if self._history else None
        targets = {**(before.targets if before else {}), **instruction.targets}
        # Only a target whose value changes settles; one left alone or restated
        # at the same value keeps being scored.
        scored_from = {
            metric: at + self.settle
            if before is None or _changed(metric, before.targets[metric], targets[metric])
            else before.scored_from[metric]
            for metric in METRICS
        }
        self._history.append(_Given(at, instruction, targets, scored_from))
        return instruction

    def _in_force(self, at: datetime) -> int | None:
        """Index into the history of the instruction in force at `at`, if any."""
        for index in range(len(self._history) - 1, -1, -1):
            if self._history[index].at <= at:
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
        given = self._history[index]
        return {
            metric: math.nan if at < given.scored_from[metric] else given.targets[metric]
            for metric in METRICS
        }

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
        scored = [metric for metric in METRICS if not math.isnan(targets[metric])]
        if not scored:
            return row
        given = self._history[self._in_force(at)]
        for metric in scored:
            stretch = given.scored_from[metric]
            last = self._last_scored.get(metric)
            # A settle window ends a stretch, so the time across it is not scored time.
            if last is not None and last[1] == stretch:
                gap = (at - last[0]).total_seconds()
                self.scored_seconds_by_factor[metric] += min(max(gap, 0.0), MAX_SAMPLE_GAP_SECONDS)
            self._last_scored[metric] = (at, stretch)
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
