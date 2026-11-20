"""
Score the chosen clustering features against `mad` only (#230).

ADR 0002 rejects `mad` on a structural argument: it cannot separate a pilot
who sits steadily off target from one who is on target but oscillates. That
argument was never measured, because the generator drew a pilot's bias and
wobble from one scale, so the two always moved together and neither case
existed.

--independent-share draws them separately, which produces both. This runs the
same comparison on such a cohort and reports what it shows.

Deliberately narrow: two feature sets, on the log scale the ADR chose. The
whole Evidence table, every feature set and both scales, is #231.

Usage (from inference/, after uv sync):
    uv run python -m pilot_inference.performance.compare_feature_sets
    uv run python -m pilot_inference.performance.compare_feature_sets --independent-share 0
"""

import argparse

import numpy as np
import pandas as pd
from sklearn.cluster import KMeans
from sklearn.metrics import adjusted_rand_score
from sklearn.preprocessing import StandardScaler

from pilot_inference.performance.clustering import LOG_FLOOR
from pilot_inference.performance.features import METRICS, compute_features
from pilot_inference.performance.generate_synthetic_pilots import generate_pilots

# The two the ADR could not separate. Names match its Evidence table.
FEATURE_SETS = {
    "std + per-flight abs_bias": [
        f"{metric}_{part}" for metric in METRICS for part in ("std", "abs_bias")
    ],
    "mad only": [f"{metric}_mad" for metric in METRICS],
}

DEFAULT_SEEDS = tuple(range(12))
# Half the cohort, so both kinds of pilot are present without making the
# cohort unlike the one the rest of the evidence was measured on.
DEFAULT_INDEPENDENT_SHARE = 0.5


def _floor_for(column: str) -> float:
    """The log floor of the metric a feature column belongs to."""
    for metric in METRICS:
        if column.startswith(f"{metric}_"):
            return LOG_FLOOR[metric]
    raise ValueError(f"No log floor for {column}")


def score(pilots: pd.DataFrame, truth: pd.DataFrame, columns: list, k: int, seed: int) -> float:
    """
    Adjusted Rand index of k groups on these columns against the hidden tier.

    Same treatment as clustering.py: add the metric's floor, take the log,
    standardize across the cohort, then KMeans.
    """
    missing = [c for c in columns if c not in pilots.columns]
    if missing:
        raise ValueError(f"Missing column(s): {', '.join(missing)}")

    features = pilots[columns]
    floors = pd.Series({column: _floor_for(column) for column in columns})
    logged = np.log(features + floors[columns])
    scaled = StandardScaler().fit_transform(logged)
    labels = KMeans(n_clusters=k, n_init=10, random_state=seed).fit_predict(scaled)

    tiers = pilots[["pilot_id"]].merge(truth, on="pilot_id", how="left")["tier"]
    return float(adjusted_rand_score(tiers, labels))


def compare(
    seeds=DEFAULT_SEEDS,
    k: int = 3,
    n_pilots: int = 15,
    flight_seconds: int = 120,
    independent_share: float = DEFAULT_INDEPENDENT_SHARE,
) -> pd.DataFrame:
    """One row per seed, one column per feature set."""
    rows = []
    for seed in seeds:
        telemetry, truth = generate_pilots(
            n_pilots=n_pilots,
            seed=seed,
            flight_seconds=flight_seconds,
            independent_share=independent_share,
        )
        _, pilots = compute_features(telemetry)
        row = {"seed": seed}
        for name, columns in FEATURE_SETS.items():
            row[name] = score(pilots, truth, columns, k, seed=0)
        rows.append(row)
    return pd.DataFrame(rows)


def style_separation(
    seeds=DEFAULT_SEEDS,
    n_pilots: int = 15,
    flight_seconds: int = 120,
    independent_share: float = 1.0,
) -> pd.DataFrame:
    """
    The question tier recovery cannot answer.

    ADR 0002 rejects `mad` because it cannot tell a pilot who sits steadily
    off target from one who is on target but oscillates. Those two pilots have
    the same tier -- tier is total deviation size, which is what `mad`
    measures well -- so scoring against tier can never test the claim, however
    the cohort is built.

    This asks it directly. Within one tier, split the pilots by the
    generator's own bias_over_wobble ratio, then fit two groups on each
    feature set and score against that split. High means the features carry
    the distinction; near 0 means they do not.
    """
    rows = []
    for seed in seeds:
        telemetry, truth = generate_pilots(
            n_pilots=n_pilots,
            seed=seed,
            flight_seconds=flight_seconds,
            independent_share=independent_share,
        )
        _, pilots = compute_features(telemetry)
        joined = pilots.merge(truth, on="pilot_id", how="left")

        for tier, cohort in joined.groupby("tier"):
            if len(cohort) < 4:
                continue
            # Steady versus wobbly within this tier, by the generator's number.
            steady = (cohort["bias_over_wobble"] > cohort["bias_over_wobble"].median()).astype(int)
            if steady.nunique() < 2:
                continue
            row = {"seed": seed, "tier": int(tier), "pilots": len(cohort)}
            for name, columns in FEATURE_SETS.items():
                row[name] = score_against(cohort, steady.to_numpy(), columns, k=2, seed=0)
            rows.append(row)
    return pd.DataFrame(rows, columns=["seed", "tier", "pilots", *FEATURE_SETS])


def score_against(pilots: pd.DataFrame, labels: np.ndarray, columns: list, k: int, seed: int):
    """Adjusted Rand index of k groups on these columns against any labelling."""
    features = pilots[columns]
    floors = pd.Series({column: _floor_for(column) for column in columns})
    logged = np.log(features + floors[columns])
    scaled = StandardScaler().fit_transform(logged)
    found = KMeans(n_clusters=k, n_init=10, random_state=seed).fit_predict(scaled)
    return float(adjusted_rand_score(labels, found))


def summarize(per_seed: pd.DataFrame) -> pd.DataFrame:
    """Mean and range per feature set, which is what the ADR table reports."""
    rows = []
    for name in FEATURE_SETS:
        scores = per_seed[name]
        rows.append(
            {
                "features": name,
                "mean": scores.mean(),
                "min": scores.min(),
                "max": scores.max(),
            }
        )
    return pd.DataFrame(rows)


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--k", type=int, default=3, help="Number of groups")
    parser.add_argument("--pilots", type=int, default=15, help="Pilots per cohort")
    parser.add_argument("--flight-seconds", type=int, default=120)
    parser.add_argument("--seeds", type=int, default=len(DEFAULT_SEEDS), help="How many seeds")
    parser.add_argument(
        "--independent-share",
        type=float,
        default=DEFAULT_INDEPENDENT_SHARE,
        help="Fraction of pilots whose bias and wobble scales are drawn separately",
    )
    args = parser.parse_args(argv)

    try:
        per_seed = compare(
            DEFAULT_SEEDS[: args.seeds],
            args.k,
            args.pilots,
            args.flight_seconds,
            args.independent_share,
        )
    except ValueError as e:
        parser.error(str(e))

    summary = summarize(per_seed)
    print(
        f"{args.pilots} pilots, k={args.k}, {len(per_seed)} seeds, "
        f"independent share {args.independent_share}\n"
    )
    print(summary.to_string(index=False, float_format="%.3f"))
    print()
    print(per_seed.to_string(index=False, float_format="%.3f"))

    gap = summary.loc[0, "mean"] - summary.loc[1, "mean"]
    spread = per_seed[list(FEATURE_SETS)].max().max() - per_seed[list(FEATURE_SETS)].min().min()
    print(
        f"\nGap between the two: {gap:+.3f}. Per-seed scores span {spread:.2f}, "
        "so read the gap against that."
    )

    # Tier recovery cannot test the ADR's actual claim: the two pilots it is
    # about share a tier. This can.
    styles = style_separation(
        DEFAULT_SEEDS[: args.seeds], args.pilots, args.flight_seconds, independent_share=1.0
    )
    print("\nSteady versus oscillating, within one tier (all pilots independent):\n")
    if styles.empty:
        # Three tiers, and a tier needs four pilots before splitting it in two
        # means anything.
        print(
            f"  Not enough pilots per tier. {args.pilots} pilots gives about "
            f"{args.pilots // 3} per tier; this needs at least 4, and about 10 "
            "before the difference is visible at all."
        )
        return
    print(styles[list(FEATURE_SETS)].agg(["mean", "min", "max"]).T.to_string(float_format="%.3f"))
    print(f"\nOver {len(styles)} tier-cohorts from {args.seeds} seeds.")


if __name__ == "__main__":
    main()
