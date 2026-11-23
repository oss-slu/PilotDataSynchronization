"""
Build one deviation-feature row per Flight from the client's Prepar3D sample.

Runs load -> clean -> legs over a folder of exports, writes the feature table
a clustering model trains on, and writes a per-Flight report and an
actual-against-target plot for each Flight so the Legs can be eyeballed
before anything is trained on them.

Usage (from inference/, after uv sync):
    uv run python -m pilot_inference.prepar3d.build_features

The client's exports are not in the repo. Copy them into
inference/data/client/raw/ from the team's Drive folder first. That folder and
inference/results/ are gitignored: the data is under an IRB protocol and must
never be committed.

Outputs:
    inference/data/client/clean/<flight_id>.csv      cleaned samples + Targets
    inference/data/client/features/flight_features.csv
    inference/results/prepar3d_flight_report.csv     trim, gaps, spikes, Legs
    inference/results/prepar3d_plots/<flight_id>.png
"""

from __future__ import annotations

import argparse
import warnings
from pathlib import Path

import numpy as np
import pandas as pd

from pilot_inference.paths import DATA_DIR, INFERENCE_DIR
from pilot_inference.performance.features import compute_deviations, flight_features
from pilot_inference.prepar3d.clean import (
    DEFAULT_AIRSPEED_MIN_KTS,
    DEFAULT_ALTITUDE_MARGIN_FT,
    DEFAULT_MAX_GAP_SECONDS,
    DEFAULT_SPIKE_FPM,
    clean_flight,
)
from pilot_inference.angles import angular_diff
from pilot_inference.prepar3d.legs import (
    DEFAULT_ALTITUDE_TOLERANCE_FT,
    DEFAULT_BANK_LIMIT_DEG,
    DEFAULT_HEADING_TOLERANCE_DEG,
    DEFAULT_SETTLE_SECONDS,
    DEFAULT_VERTICAL_SPEED_LIMIT_FPM,
    attach_targets,
    leg_summary,
    load_scenario,
    nearest_heading,
)
from pilot_inference.prepar3d.load import load_folder

CLIENT_DIR = DATA_DIR / "client"
RESULTS_DIR = INFERENCE_DIR / "results"
SCENARIO = INFERENCE_DIR / "briefs" / "prepar3d_scenario.csv"

# The four the model trains on, per docs/adr/0002.
CORE_FEATURES = ("altitude_std", "altitude_abs_bias", "heading_std", "heading_abs_bias")
# Plus three, kept separate so Issue 2 can train on either set.
EXTENDED_FEATURES = ("vertical_speed_std", "vertical_speed_abs_bias", "airspeed_std")


def airspeed_spread(samples: pd.DataFrame) -> pd.DataFrame:
    """
    Per-Flight spread of indicated airspeed inside the holds.

    The scenario sets no airspeed Target, so there is no deviation and no
    bias. Spread still says something: holding a heading and an altitude
    while the airspeed wanders is not the same as holding all three. Computed
    over the scored samples only, with ddof=0 to match features.py's std.
    """
    held = samples[samples["scored"]] if "scored" in samples.columns else samples
    if held.empty:
        return pd.DataFrame(columns=["pilot_id", "flight_id", "airspeed_std"])
    grouped = held.groupby(["pilot_id", "flight_id"], sort=True)["airspeed"]
    return grouped.std(ddof=0).reset_index(name="airspeed_std")


def build(samples: pd.DataFrame) -> pd.DataFrame:
    """
    Flights with Targets attached -> one feature row per Flight.

    Reuses features.py for every deviation, so the wrapped heading difference
    and the std/abs_bias definitions are the same ones the synthetic pipeline
    and the live sidecar use.
    """
    deviations = compute_deviations(samples)
    with warnings.catch_warnings():
        # features.py warns per Flight that airspeed has no valid Target. That
        # is by design here, and airspeed_std is added separately below.
        warnings.filterwarnings("ignore", message=".*no valid target for airspeed.*")
        per_flight = flight_features(deviations)

    # features.py emits airspeed_mad/std/bias/abs_bias from the (absent)
    # airspeed Target, so they are all NaN. Dropping them before the merge
    # keeps the real airspeed_std from colliding with the empty one and
    # arriving as airspeed_std_x and airspeed_std_y.
    empty = [column for column in per_flight.columns if column.startswith("airspeed")]
    per_flight = per_flight.drop(columns=empty).merge(
        airspeed_spread(samples), on=["pilot_id", "flight_id"], how="left"
    )
    columns = ["pilot_id", "flight_id", *CORE_FEATURES, *EXTENDED_FEATURES]
    for column in columns:
        if column not in per_flight.columns:
            per_flight[column] = np.nan
    return per_flight[columns]


def unwrapped_heading(samples: pd.DataFrame, scenario) -> np.ndarray:
    """
    Heading drawn around the nearest scenario heading instead of 0 to 360.

    A pilot holding 0 crosses 360 constantly, and a raw trace then draws a
    vertical stroke across the whole axis on every crossing, which hides the
    very thing the plot exists to show. Anchoring each sample to its nearest
    scenario heading puts that pilot at a continuous -3 to +3 against a Target
    line at 0. It is per sample, not cumulative, so a Flight that turns right
    around the compass does not drift off the axis.
    """
    reference = nearest_heading(samples["heading"], scenario.headings)
    offset = angular_diff(samples["heading"].to_numpy(dtype=float), reference)
    return np.where(np.isnan(reference), samples["heading"].to_numpy(dtype=float), reference + offset)


def plot_flight(samples: pd.DataFrame, path: Path, scenario) -> bool:
    """
    One actual-against-target figure for a Flight. False if plotting is off.

    Altitude over the whole Flight with the level Target drawn across it, and
    heading with its Target stepping between the scenario's headings. Scored
    samples are marked, so a Leg that should not have been detected, or one
    that was missed, is visible without reading any numbers.
    """
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        return False

    minutes = samples["t_seconds"] / 60.0
    scored = samples["scored"].to_numpy(dtype=bool)
    figure, (top, bottom) = plt.subplots(2, 1, figsize=(11, 6), sharex=True)

    top.plot(minutes, samples["altitude"], linewidth=0.8, color="#333333", label="altitude")
    top.axhline(
        scenario.level_altitude,
        linestyle="--",
        linewidth=0.8,
        color="#1f77b4",
        label="level target",
    )
    scored_altitude = samples["target_altitude"].notna().to_numpy(dtype=bool)
    top.scatter(
        minutes[scored_altitude],
        samples["altitude"][scored_altitude],
        s=3,
        color="#1f77b4",
        label="scored for altitude",
    )
    top.set_ylabel("altitude, ft")
    top.legend(loc="upper right", fontsize="small")
    top.set_title(f"{samples['flight_id'].iloc[0]}  ({int(scored.sum())} scored samples)")

    bottom.plot(
        minutes,
        unwrapped_heading(samples, scenario),
        linewidth=0.8,
        color="#333333",
        label="heading",
    )
    bottom.plot(
        minutes,
        samples["target_heading"],
        linewidth=1.4,
        color="#d62728",
        label="heading target",
    )
    bottom.set_ylabel("heading, deg true")
    bottom.set_xlabel("minutes from the start of the recording")
    bottom.legend(loc="upper right", fontsize="small")

    path.parent.mkdir(parents=True, exist_ok=True)
    figure.tight_layout()
    figure.savefig(path, dpi=110)
    plt.close(figure)
    return True


def run(args) -> int:
    scenario = load_scenario(args.scenario)
    flights = load_folder(args.raw_dir)

    prepared, reports = [], []
    for raw in flights:
        trimmed, report = clean_flight(
            raw,
            airspeed_min=args.airspeed_min,
            altitude_margin=args.altitude_margin,
            max_gap_seconds=args.gap_seconds,
            spike_fpm=args.spike_fpm,
        )
        if trimmed.empty:
            print(f"  {report.flight_id}: {report.note}, no features")
            reports.append(report.as_row())
            continue

        with_targets = attach_targets(
            trimmed,
            scenario,
            heading_tolerance=args.heading_tolerance,
            bank_limit=args.bank_limit,
            altitude_tolerance=args.altitude_tolerance,
            vertical_speed_limit=args.vertical_speed_limit,
            settle_seconds=args.settle_seconds,
            max_gap_seconds=args.gap_seconds,
        )
        prepared.append(with_targets)

        row = report.as_row()
        row.update(leg_summary(with_targets))
        reports.append(row)

        if args.clean_dir:
            args.clean_dir.mkdir(parents=True, exist_ok=True)
            with_targets.to_csv(args.clean_dir / f"{report.flight_id}.csv", index=False)
        if args.plots_dir:
            plot_flight(with_targets, args.plots_dir / f"{report.flight_id}.png", scenario)

    report_table = pd.DataFrame(reports)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report_table.to_csv(args.report, index=False)
    print(report_table.to_string(index=False))
    print(f"\nWrote the per-Flight report to {args.report}")

    if not prepared:
        print("No Flight produced a scorable sample, so no feature table was written")
        return 1

    features = build(pd.concat(prepared, ignore_index=True))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    features.to_csv(args.out, index=False)
    print(f"Wrote {len(features)} Flight(s) to {args.out}")

    incomplete = features[list(CORE_FEATURES)].isna().any(axis=1)
    if incomplete.any():
        print(
            "\nThese Flights have a NaN in the core four features, so a Leg was not "
            "detected in them. Widen the tolerances or look at their plots:"
        )
        print(features.loc[incomplete, ["pilot_id", "flight_id", *CORE_FEATURES]].to_string(index=False))
        return 1
    return 0


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--raw-dir", type=Path, default=CLIENT_DIR / "raw", help="Folder of Prepar3D TSV exports"
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=CLIENT_DIR / "features" / "flight_features.csv",
        help="Per-Flight feature table to write",
    )
    parser.add_argument(
        "--clean-dir",
        type=Path,
        default=CLIENT_DIR / "clean",
        help="Folder for the cleaned per-Flight CSVs",
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=RESULTS_DIR / "prepar3d_flight_report.csv",
        help="Per-Flight trim, gap, spike and Leg report",
    )
    parser.add_argument(
        "--plots-dir",
        type=Path,
        default=RESULTS_DIR / "prepar3d_plots",
        help="Folder for the actual-against-target plots",
    )
    parser.add_argument("--no-plots", action="store_true", help="Skip the plots")
    parser.add_argument("--scenario", type=Path, default=SCENARIO, help="Scenario brief CSV")
    parser.add_argument("--heading-tolerance", type=float, default=DEFAULT_HEADING_TOLERANCE_DEG)
    parser.add_argument("--bank-limit", type=float, default=DEFAULT_BANK_LIMIT_DEG)
    parser.add_argument("--altitude-tolerance", type=float, default=DEFAULT_ALTITUDE_TOLERANCE_FT)
    parser.add_argument(
        "--vertical-speed-limit", type=float, default=DEFAULT_VERTICAL_SPEED_LIMIT_FPM
    )
    parser.add_argument("--settle-seconds", type=float, default=DEFAULT_SETTLE_SECONDS)
    parser.add_argument("--gap-seconds", type=float, default=DEFAULT_MAX_GAP_SECONDS)
    parser.add_argument("--spike-fpm", type=float, default=DEFAULT_SPIKE_FPM)
    parser.add_argument("--airspeed-min", type=float, default=DEFAULT_AIRSPEED_MIN_KTS)
    parser.add_argument("--altitude-margin", type=float, default=DEFAULT_ALTITUDE_MARGIN_FT)
    args = parser.parse_args()
    if args.no_plots:
        args.plots_dir = None

    raise SystemExit(run(args))


if __name__ == "__main__":
    main()
