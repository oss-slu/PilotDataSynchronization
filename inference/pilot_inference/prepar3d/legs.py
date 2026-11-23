"""
Find the Legs in a Flight and attach the scenario's Targets.

The client has not sent the 7-Segment split, so Legs are detected from the
telemetry instead. The scenario is known: two circuits climbing on headings
190 and 40 to 3,100 ft, with approach legs on 220 and 0. All 19 Flights show
time on all four headings.

Two kinds of hold are detected, independently, because they overlap in time:

    heading hold   airborne, wings level, holding near a scenario heading
    level hold     airborne, near 3,100 ft, with a small vertical speed

A sample can be in both, in one, or in neither. Targets are attached per
sample, and a sample with no Target for a factor gets NaN, which is what
features.py already skips. So "unscored" needs no special case downstream.

Every tolerance is a parameter. The third issue in this sequence varies them,
and the defaults here are the issue's starting points, not measured optima.

One thing to know before reading any feature built on this. A hold is detected
by how close the aircraft already is to the Target, so a pilot who wanders is
in a hold less of the time than a pilot who does not, and their deviation
features are measured over the stretches where they happened to be closest.
That flatters the wandering pilot. The per-Flight report counts the scored
samples for exactly this reason: a Flight scored over a tenth of the samples
of another is not comparable to it, whatever its std says. Widening the
tolerances reduces the effect and mixes in more manoeuvring, which is the
trade the tolerance sweep in the third issue has to make.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Tuple

import numpy as np
import pandas as pd

from pilot_inference.angles import angular_diff
from pilot_inference.prepar3d.clean import DEFAULT_MAX_GAP_SECONDS

DEFAULT_HEADING_TOLERANCE_DEG = 20.0
DEFAULT_BANK_LIMIT_DEG = 5.0
DEFAULT_ALTITUDE_TOLERANCE_FT = 300.0
DEFAULT_VERTICAL_SPEED_LIMIT_FPM = 300.0

# Matches performance/live.py's settle window, so a turn onto a heading or a
# level-off at the top of a climb is not scored as a failure to hold.
DEFAULT_SETTLE_SECONDS = 30.0

SCENARIO_KEYS = ("scenario_heading", "level_altitude_ft", "level_vertical_speed_fpm")


@dataclass(frozen=True)
class Scenario:
    """The Targets the scenario sets, read from inference/briefs/."""

    headings: Tuple[float, ...]
    level_altitude: float
    level_vertical_speed: float = 0.0


def load_scenario(path: Path) -> Scenario:
    """
    Read the scenario brief: a two-column key,value CSV, `#` for comments.

    `scenario_heading` repeats, once per heading. The other keys appear once.
    """
    table = pd.read_csv(path, comment="#")
    missing = {"key", "value"} - set(table.columns)
    if missing:
        raise ValueError(f"{Path(path).name}: expected key,value columns, got {list(table.columns)}")

    values = {}
    headings = []
    for key, value in zip(table["key"], table["value"]):
        key = str(key).strip()
        if key == "scenario_heading":
            headings.append(float(value))
        else:
            values[key] = float(value)

    unknown = set(values) - set(SCENARIO_KEYS)
    if unknown:
        raise ValueError(f"{Path(path).name}: unknown key(s) {', '.join(sorted(unknown))}")
    if not headings:
        raise ValueError(f"{Path(path).name}: no scenario_heading rows")
    if "level_altitude_ft" not in values:
        raise ValueError(f"{Path(path).name}: no level_altitude_ft row")

    return Scenario(
        headings=tuple(headings),
        level_altitude=values["level_altitude_ft"],
        level_vertical_speed=values.get("level_vertical_speed_fpm", 0.0),
    )


def nearest_heading(heading, headings) -> np.ndarray:
    """
    The scenario heading each sample is closest to, wrapped.

    Needed because the scenario's headings are close enough to overlap at the
    default tolerance: 190 and 220 are 30 degrees apart, so plus or minus 20
    would claim 200 to 210 twice. Taking the nearest gives every sample one
    Target instead of two.
    """
    actual = np.asarray(heading, dtype=float)
    candidates = np.asarray(headings, dtype=float)
    distances = np.abs(angular_diff(actual[:, None], candidates[None, :]))
    nearest = np.full(len(actual), np.nan)
    usable = ~np.all(np.isnan(distances), axis=1)
    if usable.any():
        choice = np.nanargmin(distances[usable], axis=1)
        nearest[usable] = candidates[choice]
    return nearest


def _run_ids(keep, labels, seconds, max_gap_seconds: float) -> np.ndarray:
    """
    Label contiguous runs of `keep`, as a Leg each. -1 where not kept.

    A run also ends when the label changes, so turning from one scenario
    heading to the next starts a new Leg, and when the clock jumps by more
    than `max_gap_seconds`, so the samples after a stall in the recording
    settle again instead of inheriting the Leg from before it.
    """
    keep = np.asarray(keep, dtype=bool)
    seconds = np.asarray(seconds, dtype=float)
    ids = np.full(len(keep), -1, dtype=int)
    current = -1
    previous: Optional[int] = None

    for index in range(len(keep)):
        if not keep[index]:
            previous = None
            continue
        gap = 0.0
        if previous is not None:
            step = seconds[index] - seconds[previous]
            gap = step if np.isfinite(step) else 0.0
        changed = (
            previous is None
            or gap > max_gap_seconds
            or (labels is not None and labels[index] != labels[previous])
        )
        if changed:
            current += 1
        ids[index] = current
        previous = index
    return ids


def _after_settle(ids: np.ndarray, seconds, settle_seconds: float) -> np.ndarray:
    """Within each Leg, the samples past its settle window."""
    seconds = np.asarray(seconds, dtype=float)
    scored = np.zeros(len(ids), dtype=bool)
    for run in np.unique(ids[ids >= 0]):
        positions = np.flatnonzero(ids == run)
        start = seconds[positions[0]]
        if not np.isfinite(start):
            # No usable clock for this Leg; scoring all of it beats dropping it.
            scored[positions] = True
            continue
        scored[positions] = (seconds[positions] - start) >= settle_seconds
    return scored


def attach_targets(
    df: pd.DataFrame,
    scenario: Scenario,
    heading_tolerance: float = DEFAULT_HEADING_TOLERANCE_DEG,
    bank_limit: float = DEFAULT_BANK_LIMIT_DEG,
    altitude_tolerance: float = DEFAULT_ALTITUDE_TOLERANCE_FT,
    vertical_speed_limit: float = DEFAULT_VERTICAL_SPEED_LIMIT_FPM,
    settle_seconds: float = DEFAULT_SETTLE_SECONDS,
    max_gap_seconds: float = DEFAULT_MAX_GAP_SECONDS,
) -> pd.DataFrame:
    """
    A cleaned Flight -> the same samples with Target columns attached.

    Adds `target_heading`, `target_altitude`, `target_vertical_speed` (NaN
    where that factor is not being scored), `target_airspeed` (always NaN, the
    scenario sets no airspeed Target and inventing one would make up a
    result), `heading_leg` and `level_leg` for diagnostics, and `scored`,
    which is true where any factor is being scored.

    `velocity` is added as a copy of `airspeed` because that is the name
    features.py expects for it.

    The input is not modified.
    """
    out = df.copy().reset_index(drop=True)
    if out.empty:
        for column in (
            "target_heading",
            "target_altitude",
            "target_vertical_speed",
            "target_airspeed",
            "velocity",
        ):
            out[column] = pd.Series(dtype=float)
        out["heading_leg"] = pd.Series(dtype=int)
        out["level_leg"] = pd.Series(dtype=int)
        out["scored"] = pd.Series(dtype=bool)
        return out

    seconds = out["t_seconds"].to_numpy(dtype=float)
    airborne = (
        out["airborne"].fillna(False).to_numpy(dtype=bool)
        if "airborne" in out.columns
        else np.ones(len(out), dtype=bool)
    )

    # Heading holds: nearest scenario heading, within tolerance, wings level.
    candidate = nearest_heading(out["heading"], scenario.headings)
    off_target = np.abs(angular_diff(out["heading"].to_numpy(dtype=float), candidate))
    level_wings = (out["bank"].abs() < bank_limit).fillna(False).to_numpy(dtype=bool)
    # A NaN heading compares False here, so an unparseable sample is simply
    # not in any Leg.
    with np.errstate(invalid="ignore"):
        within = off_target <= heading_tolerance
    holding = airborne & level_wings & within
    heading_legs = _run_ids(holding, candidate, seconds, max_gap_seconds)
    heading_scored = _after_settle(heading_legs, seconds, settle_seconds)

    # Level holds: near the scenario altitude, not climbing or descending.
    near_altitude = (
        ((out["altitude"] - scenario.level_altitude).abs() <= altitude_tolerance)
        .fillna(False)
        .to_numpy(dtype=bool)
    )
    steady = (
        (out["vertical_speed"].abs() < vertical_speed_limit).fillna(False).to_numpy(dtype=bool)
    )
    level = airborne & near_altitude & steady
    level_legs = _run_ids(level, None, seconds, max_gap_seconds)
    level_scored = _after_settle(level_legs, seconds, settle_seconds)

    out["target_heading"] = np.where(heading_scored, candidate, np.nan)
    out["target_altitude"] = np.where(level_scored, scenario.level_altitude, np.nan)
    out["target_vertical_speed"] = np.where(level_scored, scenario.level_vertical_speed, np.nan)
    out["target_airspeed"] = np.nan
    out["velocity"] = out["airspeed"]
    out["heading_leg"] = np.where(heading_scored, heading_legs, -1)
    out["level_leg"] = np.where(level_scored, level_legs, -1)
    out["scored"] = heading_scored | level_scored
    return out


def leg_summary(df: pd.DataFrame) -> dict:
    """How much of one Flight ended up scored, for the run report."""
    scored = df["scored"] if "scored" in df.columns else pd.Series([], dtype=bool)
    return {
        "samples": len(df),
        "scored_samples": int(scored.sum()),
        "heading_legs": int(df.loc[df["heading_leg"] >= 0, "heading_leg"].nunique())
        if "heading_leg" in df.columns
        else 0,
        "level_legs": int(df.loc[df["level_leg"] >= 0, "level_leg"].nunique())
        if "level_leg" in df.columns
        else 0,
        "heading_scored_samples": int(df["target_heading"].notna().sum())
        if "target_heading" in df.columns
        else 0,
        "altitude_scored_samples": int(df["target_altitude"].notna().sum())
        if "target_altitude" in df.columns
        else 0,
    }
