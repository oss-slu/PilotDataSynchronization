"""
Read one Prepar3D export into a sample table.

The 19 files in the client's sample come in 8 different column layouts. Some
insert `Flaps position percent`, `Nose gear` or `Engine 2 N2` partway through
a row, some have blank header cells, and all have 2 to 4 unnamed trailing
values, so vertical speed sits in column 15, 16 or 17 depending on the file.
Everything here is therefore looked up by header name. Nothing is read by
column position, because positional parsing does not fail on these files, it
silently returns a different quantity.

Header cells carry the unit after a comma (`Altitude MSL,ft`), so the name is
the part before the first comma and the unit is the part after it. Declared
units are checked: a file that says metres is an error, not a silent 3.28x.

Files are decoded as latin-1, which cannot fail, and `\\r` is stripped.
"""

from __future__ import annotations

import csv
import re
import warnings
from pathlib import Path

import pandas as pd

# Our column name -> the header names it may appear under, most specific
# first. Matching is on the name with case and runs of whitespace normalised,
# and it is exact: "heading" never matches "heading (magnetic)".
COLUMN_ALIASES = {
    "timestamp": ("timestamp", "time"),
    "altitude": ("altitude msl", "altitude"),
    "heading": ("heading (true)", "heading true", "true heading"),
    "airspeed": ("indicated airspeed", "airspeed indicated", "indicated air speed"),
    "vertical_speed": ("vertical speed", "vertical velocity"),
    "bank": ("bank", "roll"),
    "pitch": ("pitch",),
}

# Units we accept per column, normalised the same way. A header with no unit
# is warned about and trusted; a header with a unit we do not recognise is an
# error, because the number would be wrong rather than missing.
EXPECTED_UNITS = {
    "altitude": ("ft", "feet"),
    "heading": ("deg", "degree", "degrees"),
    "airspeed": ("kts", "kt", "knots", "knot"),
    "vertical_speed": ("fpm", "ft/min", "feet/minute", "ft per min"),
    "bank": ("deg", "degree", "degrees"),
    "pitch": ("deg", "degree", "degrees"),
}

VALUE_COLUMNS = ("altitude", "heading", "airspeed", "vertical_speed", "bank", "pitch")

# P004_T1.tsv -> pilot P004, flight P004-T1.
FILENAME = re.compile(r"^(?P<pilot>P\d+)_(?P<run>.+)$")

# A backwards jump larger than this in a time-of-day column is midnight, not a
# recording error. Flights run 77 to 163 minutes, so nothing legitimate in one
# file can move the clock back by an hour.
ROLLOVER_THRESHOLD_SECONDS = 3600.0

# Timestamp formats tried in order before falling back to dateutil. Naming
# them keeps one odd row from changing how the whole column is read, which is
# what pandas warns about when it has to guess per element.
TIMESTAMP_FORMATS = (
    "%H:%M:%S",
    "%H:%M:%S.%f",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d %H:%M:%S.%f",
    "%m/%d/%Y %H:%M:%S",
    "%d/%m/%Y %H:%M:%S",
)


def _normalise(text: str) -> str:
    return " ".join(text.strip().lower().split())


def parse_flight_ids(path: Path):
    """
    `P004_T1.tsv` -> `("P004", "P004-T1")`.

    The flight id keeps the pilot in it so flight ids are unique across the
    whole sample, which is what features.py groups on.
    """
    match = FILENAME.match(Path(path).stem)
    if not match:
        raise ValueError(
            f"{Path(path).name!r} is not <pilot>_<run>.tsv, so its pilot_id and "
            f"flight_id cannot be read from the filename"
        )
    pilot_id = match.group("pilot")
    return pilot_id, f"{pilot_id}-{match.group('run')}"


def split_header(cell: str):
    """`'Altitude MSL,ft'` -> `('Altitude MSL', 'ft')`. No comma means no unit."""
    name, _, unit = cell.partition(",")
    return name.strip(), unit.strip()


def _locate_columns(header, source: str):
    """
    Map each of our columns to its index in this file's header.

    Returns the index map and the declared unit map. Raises if a column we
    need is absent, naming the file and the headers it does have, because that
    means a ninth layout has turned up and the caller has to look at it.
    """
    parsed = [split_header(cell) for cell in header]
    by_name = {}
    for index, (name, unit) in enumerate(parsed):
        key = _normalise(name)
        if key and key not in by_name:
            by_name[key] = (index, unit)

    indices, units, missing = {}, {}, []
    for column, aliases in COLUMN_ALIASES.items():
        for alias in aliases:
            if alias in by_name:
                indices[column], units[column] = by_name[alias]
                break
        else:
            missing.append(column)

    if missing:
        present = ", ".join(name for name, _ in parsed if name) or "(none)"
        raise ValueError(
            f"{source}: no column found for {', '.join(missing)}. "
            f"Headers present: {present}"
        )
    return indices, units


def _check_units(units, source: str) -> None:
    for column, expected in EXPECTED_UNITS.items():
        declared = _normalise(units.get(column, ""))
        if not declared:
            warnings.warn(
                f"{source}: the {column} header declares no unit, so {expected[0]} "
                f"is assumed",
                UserWarning,
                stacklevel=3,
            )
        elif declared not in expected:
            raise ValueError(
                f"{source}: {column} is in {declared!r}, but this pipeline needs "
                f"{' or '.join(expected)}. Convert it before loading."
            )


def _to_float(text: str) -> float:
    try:
        return float(text.strip())
    except (TypeError, ValueError):
        return float("nan")


def _parse_datetimes(raw: pd.Series) -> pd.Series:
    """
    Parse a timestamp column, preferring a single known format for all of it.

    The first format that parses every value wins. Only a column that matches
    none of them falls back to per-element parsing, which is both slow and
    free to read two rows two different ways.
    """
    for layout in TIMESTAMP_FORMATS:
        parsed = pd.to_datetime(raw, format=layout, errors="coerce")
        if parsed.notna().all():
            return parsed
    with warnings.catch_warnings():
        warnings.filterwarnings("ignore", message=".*Could not infer format.*")
        return pd.to_datetime(raw, errors="coerce")


def elapsed_seconds(timestamps) -> pd.Series:
    """
    Seconds since this Flight's first sample.

    Accepts a numeric column (already seconds, in any epoch) or a parseable
    date or time-of-day column. A time-of-day column that crosses midnight
    goes backwards, so each backwards jump of more than an hour adds a day to
    every sample after it. Values that will not parse become NaN.
    """
    raw = pd.Series(list(timestamps))
    numeric = pd.to_numeric(raw, errors="coerce")
    if numeric.notna().all() and len(numeric):
        seconds = numeric.astype(float)
    else:
        parsed = _parse_datetimes(raw)
        if parsed.isna().all():
            return pd.Series([float("nan")] * len(raw), dtype=float)
        seconds = (parsed - parsed.dropna().iloc[0]).dt.total_seconds()

    steps = seconds.diff()
    rollovers = (steps < -ROLLOVER_THRESHOLD_SECONDS).fillna(False)
    seconds = seconds + rollovers.cumsum() * 86400.0
    first = seconds.dropna()
    return (seconds - first.iloc[0]).astype(float) if len(first) else seconds.astype(float)


def read_rows(path: Path):
    """
    The raw cells of one export: the header names and the data rows.

    Rows are returned as read, ragged tails included, so the caller can count
    what it dropped rather than having it disappear here.
    """
    path = Path(path)
    text = path.read_text(encoding="latin-1").replace("\r", "")
    rows = list(csv.reader(text.splitlines(), delimiter="\t"))
    if not rows:
        raise ValueError(f"{path.name}: the file is empty")
    return rows[0], rows[1:]


def load_flight(path: Path) -> pd.DataFrame:
    """
    One Prepar3D TSV -> a sample table.

    Columns: pilot_id, flight_id, source_file, timestamp (as written),
    t_seconds, and the six values in ft, deg, kts and fpm. One row per sample,
    in file order. Rows too short to hold every column we need are dropped
    with a warning that counts them; everything else is kept, including rows
    whose individual values will not parse, which become NaN.
    """
    path = Path(path)
    pilot_id, flight_id = parse_flight_ids(path)
    header, data = read_rows(path)
    indices, units = _locate_columns(header, path.name)
    _check_units(units, path.name)

    needed = max(indices.values())
    usable = [row for row in data if len(row) > needed]
    short = len(data) - len(usable)
    if short:
        warnings.warn(
            f"{path.name}: dropped {short} row(s) with fewer than {needed + 1} cells",
            UserWarning,
            stacklevel=2,
        )
    if not usable:
        raise ValueError(f"{path.name}: no row holds all of {', '.join(indices)}")

    table = {
        "pilot_id": pilot_id,
        "flight_id": flight_id,
        "source_file": path.name,
        "timestamp": [row[indices["timestamp"]].strip() for row in usable],
    }
    for column in VALUE_COLUMNS:
        table[column] = [_to_float(row[indices[column]]) for row in usable]

    df = pd.DataFrame(table)
    df.insert(4, "t_seconds", elapsed_seconds(df["timestamp"]).to_numpy())
    return df


def load_folder(folder: Path, pattern: str = "*.tsv"):
    """
    Every export in a folder, newest layout or oldest, one table per Flight.

    Returns a list of DataFrames ordered by flight_id. A file that cannot be
    loaded at all is not allowed to take the others down: its error is
    re-raised as a warning and it is left out, so a run over the sample
    reports which files need looking at instead of stopping on the first.
    """
    folder = Path(folder)
    if not folder.is_dir():
        raise FileNotFoundError(
            f"{folder} does not exist. The client's exports are not in the repo; "
            f"copy them from the team's Drive folder. They must not be committed."
        )

    flights = []
    for path in sorted(folder.glob(pattern)):
        try:
            flights.append(load_flight(path))
        except ValueError as error:
            warnings.warn(f"Skipping {path.name}: {error}", UserWarning, stacklevel=2)
    if not flights:
        raise ValueError(f"{folder} holds no readable {pattern} file")
    return sorted(flights, key=lambda df: df["flight_id"].iloc[0])


def sample_rate_hz(df: pd.DataFrame) -> float:
    """
    Median sampling rate of one Flight, in Hz. NaN if time never advances.

    The median, not the mean, so the handful of multi-second gaps in the
    sample do not drag it down.
    """
    steps = pd.Series(df["t_seconds"]).diff().dropna()
    steps = steps[steps > 0]
    return float(1.0 / steps.median()) if len(steps) else float("nan")
