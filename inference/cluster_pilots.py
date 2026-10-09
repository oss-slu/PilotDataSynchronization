"""
Form performance groups from per-pilot deviation features.

Without --k, prints silhouette and subsample stability for each candidate k
and writes nothing; a person picks k from that report. With --k, fits once and
writes each pilot's group and each group's centre in raw units.

Usage (from the project root):
    python inference/cluster_pilots.py
    python inference/cluster_pilots.py --k 3 --truth inference/Data/synthetic_pilot_tiers.csv
"""

import argparse
from pathlib import Path

import pandas as pd

from clustering import fit, sweep, truth_agreement

DATA_DIR = Path(__file__).parent / "Data"


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--input",
        type=Path,
        default=DATA_DIR / "pilot_features.csv",
        help="Per-pilot features CSV from compute_features.py",
    )
    parser.add_argument("--k", type=int, help="Number of groups; omit to print the sweep")
    parser.add_argument("--seed", type=int, default=0, help="Random seed")
    parser.add_argument(
        "--subsamples", type=int, default=50, help="Refits per k for the stability score"
    )
    parser.add_argument(
        "--truth", type=Path, help="True tier per pilot, for synthetic pilots only"
    )
    parser.add_argument(
        "--assignments-output",
        type=Path,
        default=DATA_DIR / "pilot_clusters.csv",
        help="Group per pilot CSV",
    )
    parser.add_argument(
        "--centres-output",
        type=Path,
        default=DATA_DIR / "group_centres.csv",
        help="Group centres CSV, in raw units",
    )
    args = parser.parse_args(argv)
    if args.truth is not None and args.k is None:
        parser.error("--truth needs --k: agreement is reported for one fitted k")

    pilots = pd.read_csv(args.input)
    if args.k is None:
        report = sweep(pilots, seed=args.seed, n_subsamples=args.subsamples)
        print(report.to_string(index=False, float_format="%.3f"))
        print("\nPick k from this report and rerun with --k.")
        return

    assignments, centres = fit(pilots, args.k, seed=args.seed)
    assignments.to_csv(args.assignments_output, index=False)
    centres.to_csv(args.centres_output, index=False)
    print(f"Wrote {len(assignments)} pilots in {args.k} groups to {args.assignments_output}")
    print(f"Wrote group centres to {args.centres_output}")
    print(centres.to_string(index=False, float_format="%.2f"))
    if args.truth is not None:
        score = truth_agreement(assignments, pd.read_csv(args.truth))
        print(f"Agreement with true tiers (adjusted Rand index): {score:.3f}")


if __name__ == "__main__":
    main()
