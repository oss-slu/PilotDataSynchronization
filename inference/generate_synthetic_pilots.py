"""
Generate synthetic pilots for testing performance groups (#216).

Each synthetic pilot belongs to one of 3 hidden skill tiers (1 holds targets
best, 3 worst) and flies 2 to 4 flights at 20 Hz, each with its own constant
targets. Per metric, a flight's deviation is a steady bias with a random sign
plus time-correlated AR(1) wobble. The tier sets the size of both, scaled per
pilot and metric so a pilot can be better at altitude than at heading.

Writes telemetry in post-#196 units (ft, kts) that compute_features.py
accepts, and the true tiers to a separate file that clustering may compare
against. generate_balanced_data.py is separate and serves the labeler.

Usage (from the project root):
    python inference/generate_synthetic_pilots.py
"""

import argparse
from pathlib import Path

import numpy as np
import pandas as pd

from features import ACTUAL_COLUMN, METRICS

DATA_DIR = Path(__file__).parent / "Data"
HZ = 20
TIERS = (1, 2, 3)
TIER_SCALE = {1: 1.0, 2: 2.5, 3: 6.0}
# Per-pilot, per-metric spread around the tier scale.
METRIC_VARIATION = (0.7, 1.4)
# Per-flight spread of the bias size.
BIAS_VARIATION = (0.7, 1.3)
# AR(1) coefficient per sample; at 20 Hz a wobble lasts a few seconds.
WOBBLE_PERSISTENCE = 0.98

# Tier 1 sizes in raw units: altitude ft, heading deg, vertical speed ft/min, airspeed kts.
BASE_BIAS = {"altitude": 20.0, "heading": 1.5, "vertical_speed": 40.0, "airspeed": 2.0}
BASE_WOBBLE = {"altitude": 15.0, "heading": 1.0, "vertical_speed": 60.0, "airspeed": 1.5}


def _targets(rng) -> dict:
    return {
        "altitude": float(rng.integers(6, 19) * 500),
        "heading": float(rng.integers(0, 360)),
        "vertical_speed": float(rng.choice([-500, 0, 500])),
        "airspeed": float(rng.integers(18, 29) * 5),
    }


def _ar1(rng, n: int, sd: float) -> np.ndarray:
    """Stationary AR(1) series with standard deviation sd."""
    innovations = rng.normal(0.0, sd * np.sqrt(1 - WOBBLE_PERSISTENCE**2), n)
    series = np.empty(n)
    series[0] = rng.normal(0.0, sd)
    for t in range(1, n):
        series[t] = WOBBLE_PERSISTENCE * series[t - 1] + innovations[t]
    return series


def _flight(rng, pilot_id: str, flight_id: str, deviation_scale: dict, n: int) -> pd.DataFrame:
    targets = _targets(rng)
    columns = {"pilot_id": pilot_id, "flight_id": flight_id}
    for metric in METRICS:
        bias = BASE_BIAS[metric] * deviation_scale[metric] * rng.uniform(*BIAS_VARIATION)
        bias *= rng.choice([-1.0, 1.0])
        deviation = bias + _ar1(rng, n, BASE_WOBBLE[metric] * deviation_scale[metric])
        actual = targets[metric] + deviation
        if metric == "heading":
            actual = np.mod(actual, 360.0)
        columns[ACTUAL_COLUMN[metric]] = actual
        columns[f"target_{metric}"] = targets[metric]
    return pd.DataFrame(columns)


def generate_pilots(n_pilots: int = 15, seed: int = 0, flight_seconds: int = 120):
    """
    Returns (telemetry, truth).

    telemetry has pilot_id, flight_id, the four actual values and their
    target_* columns. truth has pilot_id and tier, with tiers split as evenly
    as n_pilots allows.
    """
    rng = np.random.default_rng(seed)
    n_samples = flight_seconds * HZ
    flights, truth = [], []
    for i in range(n_pilots):
        pilot_id = f"P{i + 1:02d}"
        tier = TIERS[i * len(TIERS) // n_pilots]
        truth.append({"pilot_id": pilot_id, "tier": tier})
        # Multiplies deviation size, so larger is worse.
        deviation_scale = {m: TIER_SCALE[tier] * rng.uniform(*METRIC_VARIATION) for m in METRICS}
        for f in range(rng.integers(2, 5)):
            flights.append(_flight(rng, pilot_id, f"{pilot_id}-F{f + 1}", deviation_scale, n_samples))
    return pd.concat(flights, ignore_index=True), pd.DataFrame(truth)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--pilots", type=int, default=15, help="Number of pilots")
    parser.add_argument("--seed", type=int, default=0, help="Random seed")
    parser.add_argument("--flight-seconds", type=int, default=120, help="Length of each flight")
    parser.add_argument(
        "--output",
        type=Path,
        default=DATA_DIR / "synthetic_pilot_telemetry.csv",
        help="Telemetry CSV",
    )
    parser.add_argument(
        "--truth-output",
        type=Path,
        default=DATA_DIR / "synthetic_pilot_tiers.csv",
        help="True tier per pilot",
    )
    args = parser.parse_args()

    telemetry, truth = generate_pilots(args.pilots, args.seed, args.flight_seconds)
    telemetry.to_csv(args.output, index=False)
    truth.to_csv(args.truth_output, index=False)
    print(f"Wrote {len(telemetry)} samples from {len(truth)} pilots to {args.output}")
    print(f"Wrote true tiers to {args.truth_output}")


if __name__ == "__main__":
    main()
