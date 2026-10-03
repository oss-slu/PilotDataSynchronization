"""
Deviation features for pilot clustering.

Turns telemetry that carries targets into deviations per sample, then into
deviation features per flight and per pilot. Pure DataFrame-in, DataFrame-out
functions; compute_features.py is the CLI wrapper.

Input units must be post-#196 (altitude in feet, airspeed in knots). The
committed CSVs under Data/ are not, see docs/telemetry_schema.md section 1.

Outputs are in raw units (ft, deg, ft/min, kts). Scaling across pilots belongs
to the clustering step, not here.
"""

import numpy as np
import pandas as pd

from angles import angular_diff

METRICS = ("altitude", "heading", "vertical_speed", "airspeed")

# The telemetry CSV still calls airspeed "velocity".
ACTUAL_COLUMN = {
    "altitude": "altitude",
    "heading": "heading",
    "vertical_speed": "vertical_speed",
    "airspeed": "velocity",
}
TARGET_COLUMN = {metric: f"target_{metric}" for metric in METRICS}
ID_COLUMNS = ["pilot_id", "flight_id"]
STATISTICS = ("mad", "std", "bias")
REQUIRED_COLUMNS = ID_COLUMNS + list(ACTUAL_COLUMN.values()) + list(TARGET_COLUMN.values())


def _require_columns(df: pd.DataFrame, columns) -> None:
    missing = [c for c in columns if c not in df.columns]
    if missing:
        raise ValueError(f"Missing required column(s): {', '.join(missing)}")


def _require_ids(df: pd.DataFrame) -> None:
    """Rows with a null id would silently vanish from the groupby."""
    for column in ID_COLUMNS:
        nulls = int(df[column].isna().sum())
        if nulls:
            raise ValueError(f"{nulls} row(s) have a null {column}")


def compute_deviations(df: pd.DataFrame) -> pd.DataFrame:
    """
    Add one <metric>_deviation column per flight dynamics factor.

    Deviation is actual minus target; heading uses the wrapped difference.
    Rows with a NaN target, or an infinite value, get a NaN deviation. The
    input is not modified.
    """
    _require_columns(df, REQUIRED_COLUMNS)
    _require_ids(df)
    out = df.copy()
    for metric in METRICS:
        actual = out[ACTUAL_COLUMN[metric]]
        target = out[TARGET_COLUMN[metric]]
        if metric == "heading":
            deviation = angular_diff(actual, target)
        else:
            deviation = actual - target
        out[f"{metric}_deviation"] = pd.Series(deviation, index=out.index).replace(
            [np.inf, -np.inf], np.nan
        )
    return out


def flight_features(df: pd.DataFrame) -> pd.DataFrame:
    """
    One row per (pilot_id, flight_id) with, for each metric:

        <metric>_mad        mean absolute deviation (size of the deviation)
        <metric>_std        standard deviation of signed deviation, ddof=0 (steadiness)
        <metric>_bias       mean signed deviation (not meant for clustering by default)
        <metric>_rows_used  samples with a valid deviation

    NaN deviations are skipped. Raises if a flight has no valid sample for a
    metric, so no flight silently ends up with all-NaN features.
    """
    deviation_columns = [f"{m}_deviation" for m in METRICS]
    _require_columns(df, ID_COLUMNS + deviation_columns)
    _require_ids(df)
    if df.empty:
        raise ValueError("Cannot compute features from an empty table")

    rows = []
    for (pilot_id, flight_id), group in df.groupby(ID_COLUMNS, sort=True):
        row = {"pilot_id": pilot_id, "flight_id": flight_id}
        for metric in METRICS:
            deviations = group[f"{metric}_deviation"].dropna()
            if deviations.empty:
                raise ValueError(
                    f"Flight {flight_id!r} of pilot {pilot_id!r} has no valid "
                    f"target for {metric}; cannot compute its deviation features"
                )
            row[f"{metric}_mad"] = deviations.abs().mean()
            row[f"{metric}_std"] = deviations.std(ddof=0)
            row[f"{metric}_bias"] = deviations.mean()
            row[f"{metric}_rows_used"] = len(deviations)
        rows.append(row)
    return pd.DataFrame(rows)


def pilot_features(flights: pd.DataFrame) -> pd.DataFrame:
    """
    One row per pilot: the mean of their flights' features, so every flight
    counts equally regardless of length, plus a `flights` count.
    """
    feature_columns = [f"{m}_{suffix}" for m in METRICS for suffix in STATISTICS]
    _require_columns(flights, ID_COLUMNS + feature_columns)
    grouped = flights.groupby("pilot_id", sort=True)
    pilots = grouped[feature_columns].mean()
    pilots["flights"] = grouped.size()
    return pilots.reset_index()


def compute_features(df: pd.DataFrame):
    """Telemetry with targets -> (per-flight features, per-pilot features)."""
    flights = flight_features(compute_deviations(df))
    return flights, pilot_features(flights)
