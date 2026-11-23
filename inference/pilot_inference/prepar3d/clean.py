"""
Trim a Flight to the part that was flown, and report what looks wrong.

The recordings run 77 to 163 minutes but only 22 to 31 minutes of each is
airborne, and the client said ground time does not count. Taxi, holding short
and the post-landing roll all sit on a heading and at an altitude that would
otherwise be scored as if the pilot were flying them.

Nothing here drops a sample for looking odd. Gaps in time and vertical-speed
spikes are counted and returned, because a judgement about whether P004's
2,400 to 3,800 fpm excursions are real dives or export errors is the client's
to make, and silently deleting them would hide the question.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Tuple

import numpy as np
import pandas as pd

from pilot_inference.prepar3d.load import sample_rate_hz

# Airborne means both of these, which is what keeps a fast taxi and a bounce
# on the runway out of the Legs.
DEFAULT_AIRSPEED_MIN_KTS = 40.0
DEFAULT_ALTITUDE_MARGIN_FT = 100.0

# At rest for the purpose of measuring field elevation.
GROUND_AIRSPEED_MAX_KTS = 30.0

# Longer than this between two samples and the recording stalled. The sample
# is logged at about 1.1 Hz, so 5 s is four missing samples.
DEFAULT_MAX_GAP_SECONDS = 5.0

# Above this and a climb or descent rate is worth a human look. The sample's
# other pilots peak around 800 to 1,700 fpm; P004 T2 to T4 reach 2,400 to
# 3,800, which is either a real dive or an export artefact.
DEFAULT_SPIKE_FPM = 2000.0


@dataclass
class CleanReport:
    """What trimming one Flight found. One of these per Flight, printed by the CLI."""

    pilot_id: str = ""
    flight_id: str = ""
    rows_in: int = 0
    rows_out: int = 0
    recorded_minutes: float = float("nan")
    airborne_minutes: float = float("nan")
    sample_rate_hz: float = float("nan")
    ground_altitude_ft: float = float("nan")
    gaps: List[Tuple[float, float]] = field(default_factory=list)
    max_gap_seconds: float = 0.0
    spikes: int = 0
    vertical_speed_max_fpm: float = float("nan")
    vertical_speed_min_fpm: float = float("nan")
    note: str = ""

    def as_row(self) -> dict:
        """Flat enough for a CSV, so a run over the sample is one table."""
        return {
            "pilot_id": self.pilot_id,
            "flight_id": self.flight_id,
            "rows_in": self.rows_in,
            "rows_out": self.rows_out,
            "recorded_minutes": round(self.recorded_minutes, 1),
            "airborne_minutes": round(self.airborne_minutes, 1),
            "sample_rate_hz": round(self.sample_rate_hz, 2),
            "ground_altitude_ft": round(self.ground_altitude_ft, 0),
            "gaps_over_threshold": len(self.gaps),
            "max_gap_seconds": round(self.max_gap_seconds, 1),
            "vertical_speed_spikes": self.spikes,
            "vertical_speed_max_fpm": round(self.vertical_speed_max_fpm, 0),
            "vertical_speed_min_fpm": round(self.vertical_speed_min_fpm, 0),
            "note": self.note,
        }


def ground_altitude(df: pd.DataFrame) -> float:
    """
    Field elevation, as the median altitude of the samples that are at rest.

    Measured from the Flight rather than hardcoded for KOJC, so the same code
    works on a departure from anywhere. NaN when the Flight is never at rest,
    which is a recording that starts after takeoff. Guessing from the lowest
    sample instead would put the floor above the lowest part of the Flight and
    throw away the very samples it was meant to keep.
    """
    at_rest = df.loc[df["airspeed"] < GROUND_AIRSPEED_MAX_KTS, "altitude"].dropna()
    return float(at_rest.median()) if len(at_rest) else float("nan")


def airborne_mask(
    df: pd.DataFrame,
    airspeed_min: float = DEFAULT_AIRSPEED_MIN_KTS,
    altitude_margin: float = DEFAULT_ALTITUDE_MARGIN_FT,
) -> pd.Series:
    """
    True where the aircraft is flying: fast enough, and clear of the field.

    Both conditions are used when the field elevation is known, because
    airspeed alone counts the takeoff roll and altitude alone counts an
    aircraft parked at an airport higher than the margin above its own apron.

    When the Flight is never at rest there is no ground reference to measure
    from, so only airspeed is used. That is the honest fallback: the
    alternative is inventing a field elevation out of the altitudes we are
    trying to judge.
    """
    fast = (df["airspeed"] >= airspeed_min).fillna(False)
    floor = ground_altitude(df)
    if np.isnan(floor):
        return fast
    high = (df["altitude"] >= floor + altitude_margin).fillna(False)
    return fast & high


def find_gaps(df: pd.DataFrame, max_gap_seconds: float = DEFAULT_MAX_GAP_SECONDS):
    """
    Every jump in t_seconds longer than the threshold, as (at_second, length).

    `at_second` is where the gap starts, so it can be marked on a plot.
    """
    seconds = pd.Series(df["t_seconds"]).reset_index(drop=True)
    steps = seconds.diff()
    gaps = []
    for position in np.flatnonzero((steps > max_gap_seconds).fillna(False).to_numpy()):
        gaps.append((float(seconds.iloc[position - 1]), float(steps.iloc[position])))
    return gaps


def find_spikes(df: pd.DataFrame, spike_fpm: float = DEFAULT_SPIKE_FPM) -> pd.Series:
    """True where |vertical speed| is beyond the threshold worth a human look."""
    return (df["vertical_speed"].abs() > spike_fpm).fillna(False)


def clean_flight(
    df: pd.DataFrame,
    airspeed_min: float = DEFAULT_AIRSPEED_MIN_KTS,
    altitude_margin: float = DEFAULT_ALTITUDE_MARGIN_FT,
    max_gap_seconds: float = DEFAULT_MAX_GAP_SECONDS,
    spike_fpm: float = DEFAULT_SPIKE_FPM,
):
    """
    Trim one Flight to takeoff through final landing, and report on it.

    Returns (trimmed table, CleanReport). The trim keeps everything between
    the first and the last airborne sample, so a touch-and-go in the middle
    stays in: the client's scenario flies two circuits, and the ground roll
    between them is part of the Flight, not a second Flight.

    An added `airborne` column marks the samples that are actually flying, so
    the Leg detection can require it without recomputing the thresholds. A
    `vertical_speed_spike` column marks the flagged samples.
    """
    report = CleanReport(
        pilot_id=str(df["pilot_id"].iloc[0]) if len(df) else "",
        flight_id=str(df["flight_id"].iloc[0]) if len(df) else "",
        rows_in=len(df),
    )
    seconds = df["t_seconds"].dropna()
    report.recorded_minutes = (
        float(seconds.max() - seconds.min()) / 60.0 if len(seconds) else float("nan")
    )
    report.sample_rate_hz = sample_rate_hz(df)
    report.ground_altitude_ft = ground_altitude(df)

    flying = airborne_mask(df, airspeed_min, altitude_margin)
    if not flying.any():
        report.rows_out = 0
        report.airborne_minutes = 0.0
        report.note = "never airborne by these thresholds"
        return df.iloc[0:0].copy(), report

    positions = np.flatnonzero(flying.to_numpy())
    trimmed = df.iloc[positions[0] : positions[-1] + 1].copy().reset_index(drop=True)
    trimmed["airborne"] = airborne_mask(trimmed, airspeed_min, altitude_margin).to_numpy()
    trimmed["vertical_speed_spike"] = find_spikes(trimmed, spike_fpm).to_numpy()

    report.rows_out = len(trimmed)
    airborne_seconds = trimmed.loc[trimmed["airborne"], "t_seconds"].dropna()
    report.airborne_minutes = (
        float(airborne_seconds.max() - airborne_seconds.min()) / 60.0
        if len(airborne_seconds)
        else 0.0
    )
    report.gaps = find_gaps(trimmed, max_gap_seconds)
    report.max_gap_seconds = max((length for _, length in report.gaps), default=0.0)
    report.spikes = int(trimmed["vertical_speed_spike"].sum())
    vertical = trimmed["vertical_speed"].dropna()
    if len(vertical):
        report.vertical_speed_max_fpm = float(vertical.max())
        report.vertical_speed_min_fpm = float(vertical.min())
    if report.spikes:
        report.note = (
            f"{report.spikes} sample(s) beyond +/-{spike_fpm:.0f} fpm; "
            f"range {report.vertical_speed_min_fpm:.0f} to "
            f"{report.vertical_speed_max_fpm:.0f} fpm"
        )
    return trimmed, report
