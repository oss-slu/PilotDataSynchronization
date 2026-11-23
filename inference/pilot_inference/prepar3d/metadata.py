"""
Normalise the client's pilot demographics into one row per pilot.

Metadata is deliberately not a clustering feature (docs/adr/0001). It exists
so the groups can be interpreted after the fact: if the tightest group turns
out to be the ATPs, that is a finding, and if it does not, that is also a
finding. Keeping it out of the fit is what makes either one mean anything.

The client's table is free text. Ratings arrive as a list, or as the phrase
"All Six Ratings", and a pilot with nothing recorded arrives as "No data".
Both are handled here so the same convention cannot be read two ways in two
places.

Input and output are both client data and belong under inference/data/client/,
which is gitignored. Only the parsing lives in the repo.
"""

from __future__ import annotations

import re
from pathlib import Path

import numpy as np
import pandas as pd

RATINGS = ("instrument", "commercial", "multi_engine", "cfi", "cfii", "atp")

# Normalised rating text -> our column. Normalising lowercases, and turns
# runs of punctuation and whitespace into single spaces, so "Multi-Engine",
# "multi engine" and "MULTI  ENGINE" are one key.
RATING_ALIASES = {
    "instrument": "instrument",
    "instrument rating": "instrument",
    "ir": "instrument",
    "commercial": "commercial",
    "commercial pilot": "commercial",
    "cpl": "commercial",
    "multi engine": "multi_engine",
    "multiengine": "multi_engine",
    "me": "multi_engine",
    "mel": "multi_engine",
    "amel": "multi_engine",
    "cfi": "cfi",
    "flight instructor": "cfi",
    "certified flight instructor": "cfi",
    "cfii": "cfii",
    "cfi i": "cfii",
    "instrument instructor": "cfii",
    "atp": "atp",
    "air transport pilot": "atp",
}

ALL_SIX = "all six ratings"

# Anything that means the client recorded nothing for this pilot.
NO_DATA = {"", "no data", "none", "n a", "na", "nil", "-"}

# The client's four categories. A pilot with no background recorded gets a
# blank, which is what the issue asks for rather than a fifth category.
BACKGROUNDS = (
    "Collegiate",
    "Part 91 or 135 Commercial",
    "Air Transport",
    "Part 61 or 141 Flight Instructor",
)

COLUMNS = ("pilot_id", "flight_hours", "background", *RATINGS)


def _normalise(text) -> str:
    if text is None or (isinstance(text, float) and np.isnan(text)):
        return ""
    return " ".join(re.sub(r"[^0-9a-z]+", " ", str(text).lower()).split())


def is_no_data(text) -> bool:
    """True for a blank, "No data", or any of the other ways of writing it."""
    return _normalise(text) in NO_DATA


def parse_ratings(text) -> dict:
    """
    Free-text ratings -> a 0/1 flag per rating.

    "All Six Ratings" sets all six. "No data" and a blank set none. Otherwise
    the text is split on commas, semicolons, slashes, plus signs and the word
    "and", and each part must be a rating this module knows.

    An unrecognised part raises, naming it. Dropping it silently would make a
    pilot look less qualified than they are, and the groups are interpreted
    against exactly this table.
    """
    normalised = _normalise(text)
    if normalised == ALL_SIX:
        return {rating: 1 for rating in RATINGS}
    if normalised in NO_DATA:
        return {rating: 0 for rating in RATINGS}

    flags = {rating: 0 for rating in RATINGS}
    parts = [part for part in re.split(r"[,;/+&]| and ", str(text)) if _normalise(part)]
    unknown = []
    for part in parts:
        key = _normalise(part)
        if key in RATING_ALIASES:
            flags[RATING_ALIASES[key]] = 1
        else:
            unknown.append(part.strip())
    if unknown:
        raise ValueError(
            f"Unrecognised rating(s) {', '.join(repr(u) for u in unknown)} in {text!r}. "
            f"Add an alias to RATING_ALIASES rather than letting it be dropped."
        )
    return flags


def parse_background(text) -> str:
    """
    Free-text background -> one of the client's four categories, or a blank.

    Matched on the normalised text, so punctuation and case in the client's
    table do not matter. An unrecognised non-blank value raises.
    """
    if is_no_data(text):
        return ""
    key = _normalise(text)
    for background in BACKGROUNDS:
        if key == _normalise(background):
            return background
    raise ValueError(
        f"Unrecognised background {text!r}. Expected one of "
        f"{', '.join(BACKGROUNDS)}, or a blank for no data."
    )


def parse_flight_hours(text) -> float:
    """
    Flight hours as a number. NaN when the client recorded none.

    Tolerates the ways hours get written in a spreadsheet: "1,200", "1200 hrs",
    "~450". A value with no digits at all raises, because a typo there would
    otherwise become a silent NaN in the one metadata column that is numeric.
    """
    if is_no_data(text):
        return float("nan")
    digits = re.sub(r"[^0-9.]", "", str(text).replace(",", ""))
    if not digits or digits == ".":
        raise ValueError(f"Cannot read flight hours from {text!r}")
    return float(digits)


def metadata_row(pilot_id, flight_hours, background, ratings) -> dict:
    """One normalised row. The pilot id is upper-cased, as P004 not p004."""
    row = {
        "pilot_id": str(pilot_id).strip().upper(),
        "flight_hours": parse_flight_hours(flight_hours),
        "background": parse_background(background),
    }
    row.update(parse_ratings(ratings))
    return row


def build_metadata(records) -> pd.DataFrame:
    """
    Transcribed rows -> the normalised table, one row per pilot.

    Each record is a mapping with pilot_id, flight_hours, background and
    ratings, exactly as written in the client's document. A pilot id appearing
    twice raises, because the second row would otherwise quietly win.
    """
    rows = [
        metadata_row(
            record.get("pilot_id"),
            record.get("flight_hours"),
            record.get("background"),
            record.get("ratings"),
        )
        for record in records
    ]
    table = pd.DataFrame(rows, columns=list(COLUMNS))
    duplicated = table["pilot_id"][table["pilot_id"].duplicated()].tolist()
    if duplicated:
        raise ValueError(f"Pilot id(s) listed more than once: {', '.join(duplicated)}")
    return table.sort_values("pilot_id").reset_index(drop=True)


def normalise_file(source: Path, destination: Path) -> pd.DataFrame:
    """
    Transcribe-then-normalise: read a CSV of the client's text, write the table.

    The source is a faithful transcription of the client's document with the
    columns pilot_id, flight_hours, background, ratings. Keeping transcription
    and parsing apart means a question about a value is answered by looking at
    one file, not by re-reading a PDF.
    """
    raw = pd.read_csv(source, dtype=str, keep_default_na=False)
    missing = {"pilot_id", "flight_hours", "background", "ratings"} - set(raw.columns)
    if missing:
        raise ValueError(
            f"{Path(source).name}: missing column(s) {', '.join(sorted(missing))}"
        )
    table = build_metadata(raw.to_dict("records"))
    Path(destination).parent.mkdir(parents=True, exist_ok=True)
    table.to_csv(destination, index=False)
    return table
