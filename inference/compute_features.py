"""
Compute deviation features from telemetry that carries targets.

Reads a CSV with pilot_id, flight_id, the four actual values and the four
target_* columns (see docs/telemetry_schema.md section 4a), and writes a
per-flight and a per-pilot feature CSV.

Usage (from the project root):
    python inference/compute_features.py --input <telemetry.csv>
"""

import argparse
from pathlib import Path

import pandas as pd

from features import compute_features

DATA_DIR = Path(__file__).parent / "Data"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--input", type=Path, required=True, help="Telemetry CSV with targets")
    parser.add_argument(
        "--flight-output",
        type=Path,
        default=DATA_DIR / "flight_features.csv",
        help="Per-flight features CSV",
    )
    parser.add_argument(
        "--pilot-output",
        type=Path,
        default=DATA_DIR / "pilot_features.csv",
        help="Per-pilot features CSV",
    )
    args = parser.parse_args()

    flights, pilots = compute_features(pd.read_csv(args.input))
    flights.to_csv(args.flight_output, index=False)
    pilots.to_csv(args.pilot_output, index=False)
    print(f"Wrote {len(flights)} flights to {args.flight_output}")
    print(f"Wrote {len(pilots)} pilots to {args.pilot_output}")


if __name__ == "__main__":
    main()
