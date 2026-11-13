"""
Test the per-Flight clustering model and write every number and figure (#272).

Clustering has no labels, so there is no accuracy to report. What can be
tested is whether the groups generalize to a pilot the model never saw,
whether they survive small changes to the data and settings, and whether they
describe anything about how people fly. Each function below answers one of
those; main() runs them all and writes tables, figures and metrics.json to
inference/results/.

Run it from inference/, after uv sync:
    uv run python -m pilot_inference.performance.evaluate

With no --flights it builds a synthetic cohort, where the right answer is
known. That is the day-1 mode asked for in #272, and the only mode with tests,
because the client's Prepar3D flights are not in the repository yet (#228,
#270).
"""

import argparse
import json
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import pandas as pd
from scipy.optimize import linear_sum_assignment
from sklearn.cluster import KMeans
from sklearn.metrics import adjusted_rand_score
from sklearn.preprocessing import StandardScaler

from pilot_inference.paths import RESULTS_DIR
from pilot_inference.performance.clustering import (
    FEATURE_COLUMNS,
    FEATURE_PARTS,
    LOG_FLOOR,
)
from pilot_inference.performance.features import METRICS

ID_COLUMNS = ["pilot_id", "flight_id"]

# Enough that a p-value of 0.001 is distinguishable from 0.
DEFAULT_PERMUTATIONS = 1000

# #272 asks for seeds 0 to 11 of the synthetic generator.
SYNTHETIC_SEEDS = tuple(range(12))


# --- The model under test --------------------------------------------------
#
# TEMPORARY. #271 owns the per-Flight model and is still open, so this is the
# smallest thing that lets the tests below be written and checked against
# synthetic pilots today, as #272 asks. It is deliberately not in
# clustering.py: that file is #271's to change, and keeping this here means
# the two cannot conflict.
#
# It reuses FEATURE_COLUMNS and LOG_FLOOR so the treatment matches ADR 0002 --
# cluster on the log of std and |bias|, standardized across the cohort. The
# one addition is that the floors are a parameter, which is what makes the
# "log floors halved / doubled" sensitivity run possible at all.
#
# When #271 lands, replace fit_flights() with its model. Everything below
# depends only on the FlightGroups interface: .assignments, .centres, .assign.


@dataclass
class FlightGroups:
    """A fitted per-Flight model, kept so other Flights can be placed in it."""

    scaler: StandardScaler
    kmeans: KMeans
    floors: dict
    assignments: pd.DataFrame
    centres: pd.DataFrame

    def assign(self, flights: pd.DataFrame) -> pd.DataFrame:
        """
        The nearest group for each Flight, as pilot_id, flight_id and group.

        Flights are scaled with the fitted cohort's scaling, not their own, so
        a group means the same thing for them as for the cohort, and they do
        not move the groups.
        """
        features = feature_matrix(flights)
        scaled = self.scaler.transform(_logged(features, self.floors))
        return pd.DataFrame(
            {
                "pilot_id": features.index.get_level_values("pilot_id"),
                "flight_id": features.index.get_level_values("flight_id"),
                "group": self.kmeans.predict(scaled),
            }
        )


def feature_matrix(flights: pd.DataFrame) -> pd.DataFrame:
    """
    The clustering inputs, one row per Flight, indexed by pilot and flight id.

    Raises if a feature is missing or NaN, naming the Flight: dropping or
    imputing would quietly reshape a cohort this small.
    """
    missing = [c for c in ID_COLUMNS + FEATURE_COLUMNS if c not in flights.columns]
    if missing:
        raise ValueError(
            f"Missing column(s): {', '.join(missing)}. "
            "Regenerate the per-Flight features with compute_features.py."
        )
    features = flights.set_index(ID_COLUMNS)[FEATURE_COLUMNS]

    gaps = []
    for (pilot_id, flight_id), row in features.iterrows():
        bad = [m for m in METRICS if row[[f"{m}_std", f"{m}_abs_bias"]].isna().any()]
        if bad:
            gaps.append(f"{pilot_id}/{flight_id} ({', '.join(bad)})")
    if gaps:
        raise ValueError(f"NaN deviation features for Flight(s): {'; '.join(gaps)}")
    return features


def _logged(features: pd.DataFrame, floors: dict) -> pd.DataFrame:
    """Mirrors clustering._logged, with the floors passed in rather than fixed."""
    series = pd.Series(
        {f"{metric}_{part}": floors[metric] for metric in METRICS for part in FEATURE_PARTS}
    )
    return np.log(features + series[features.columns])


def _group_centres(features: pd.DataFrame, labels: np.ndarray) -> pd.DataFrame:
    """
    The raw-unit mean of each group's Flights, not the KMeans centre, which
    lives in log space (ADR 0002) and cannot be read in feet or degrees.
    """
    grouped = features.groupby(labels)
    centres = grouped.mean().reset_index(names="group")
    centres.insert(1, "flights", grouped.size().to_numpy())
    return centres


def fit_flights(
    flights: pd.DataFrame, k: int, seed: int = 0, floors: dict | None = None
) -> FlightGroups:
    """Cluster Flights into k performance groups. See the note above on #271."""
    floors = dict(LOG_FLOOR if floors is None else floors)
    features = feature_matrix(flights)
    if k < 2 or k > len(features):
        raise ValueError(f"k must be 2 to {len(features)} for {len(features)} Flights, got {k}")

    logged = _logged(features, floors)
    scaler = StandardScaler().fit(logged)
    kmeans = KMeans(n_clusters=k, n_init=10, random_state=seed).fit(scaler.transform(logged))
    labels = kmeans.labels_
    assignments = pd.DataFrame(
        {
            "pilot_id": features.index.get_level_values("pilot_id"),
            "flight_id": features.index.get_level_values("flight_id"),
            "group": labels,
        }
    )
    return FlightGroups(scaler, kmeans, floors, assignments, _group_centres(features, labels))


# --- Comparing two labellings ----------------------------------------------


def match_labels(reference: np.ndarray, other: np.ndarray, k: int) -> dict:
    """
    How `other`'s group numbers line up with `reference`'s.

    Two fits of the same data can agree completely and still number their
    groups differently, so comparing them directly would report disagreement
    that is not there. The pairing that overlaps most is chosen outright
    rather than greedily, which matters when two groups are close.
    """
    contingency = np.zeros((k, k), dtype=int)
    for ref, oth in zip(reference, other):
        contingency[int(oth), int(ref)] += 1
    rows, columns = linear_sum_assignment(-contingency)
    return {int(row): int(column) for row, column in zip(rows, columns)}


# --- 1. Held-out pilot: the closest thing to a test set --------------------


def held_out_pilot(flights: pd.DataFrame, k: int, seed: int = 0) -> tuple:
    """
    For each pilot: fit on everyone else, then place that pilot's Flights and
    see whether they land where the full model puts them.

    Whole pilots are held out rather than a random share of Flights. A random
    split would put the same pilot on both sides, and Flights by one pilot
    resemble each other, so the score would mostly measure that.

    The held-out fit numbers its groups independently, so its numbering is
    matched to the full model's on the pilots both of them saw, and only then
    are the held-out Flights compared.

    Returns (per_pilot, overall_agreement).
    """
    full = fit_flights(flights, k, seed)
    full_group = dict(
        zip(zip(full.assignments["pilot_id"], full.assignments["flight_id"]),
            full.assignments["group"])
    )

    rows = []
    for pilot in sorted(flights["pilot_id"].unique()):
        others = flights[flights["pilot_id"] != pilot]
        held = flights[flights["pilot_id"] == pilot]
        if others["pilot_id"].nunique() < 2 or len(others) < k:
            # Not enough left to fit; reported rather than silently skipped.
            rows.append({"pilot_id": pilot, "flights": len(held), "agreed": np.nan,
                         "agreement": np.nan, "note": "too few pilots left to fit"})
            continue

        model = fit_flights(others, k, seed)
        trained = model.assignments
        mapping = match_labels(
            np.array([full_group[(p, f)] for p, f in zip(trained["pilot_id"], trained["flight_id"])]),
            trained["group"].to_numpy(),
            k,
        )
        placed = model.assign(held)
        agreed = sum(
            mapping.get(int(group), -1) == full_group[(p, f)]
            for p, f, group in zip(placed["pilot_id"], placed["flight_id"], placed["group"])
        )
        rows.append({"pilot_id": pilot, "flights": len(held), "agreed": agreed,
                     "agreement": agreed / len(held), "note": ""})

    per_pilot = pd.DataFrame(rows)
    scored = per_pilot.dropna(subset=["agreed"])
    overall = float(scored["agreed"].sum() / scored["flights"].sum()) if len(scored) else float("nan")
    return per_pilot, overall


# --- 2. Pilot consistency, against a shuffled null -------------------------


def consistency(assignments: pd.DataFrame) -> float:
    """
    How much of a pilot's flying lands in one group, averaged over pilots.

    1.0 means every pilot's Flights share a group. Pilots with one Flight are
    left out: they score 1.0 by construction and would inflate the result.
    """
    scores = []
    for _, flights_of in assignments.groupby("pilot_id"):
        if len(flights_of) < 2:
            continue
        scores.append(flights_of["group"].value_counts().iloc[0] / len(flights_of))
    return float(np.mean(scores)) if scores else float("nan")


def consistency_permutation_test(
    assignments: pd.DataFrame, n_permutations: int = DEFAULT_PERMUTATIONS, seed: int = 0
) -> dict:
    """
    Consistency against chance: shuffle the group labels across Flights and
    see how often that does as well.

    If groups track pilots no better than a shuffle, they are probably noise.
    The p-value counts the observed value itself, so it can never be 0, which
    would overstate what a finite number of shuffles can show.

    This measures consistency, not skill. A pilot can genuinely have a good
    day and a bad one, and that would show up here as inconsistency.
    """
    observed = consistency(assignments)
    rng = np.random.default_rng(seed)
    shuffled = assignments.copy()
    groups = assignments["group"].to_numpy()

    at_least_as_high = 0
    null = []
    for _ in range(n_permutations):
        shuffled["group"] = rng.permutation(groups)
        value = consistency(shuffled)
        null.append(value)
        if value >= observed:
            at_least_as_high += 1

    return {
        "observed": observed,
        "null_mean": float(np.mean(null)),
        "null_max": float(np.max(null)),
        "permutations": n_permutations,
        "p_value": (at_least_as_high + 1) / (n_permutations + 1),
    }


# --- 3. Synthetic recovery: the one place the truth is known ---------------


def synthetic_cohort(seed: int, n_pilots: int = 15, flight_seconds: int = 120) -> tuple:
    """(flights, truth) for one seed of the synthetic generator."""
    from pilot_inference.performance.features import compute_features
    from pilot_inference.performance.generate_synthetic_pilots import generate_pilots

    telemetry, truth = generate_pilots(
        n_pilots=n_pilots, seed=seed, flight_seconds=flight_seconds
    )
    flights, _ = compute_features(telemetry)
    return flights, truth


def synthetic_recovery(
    k: int, seeds=SYNTHETIC_SEEDS, n_pilots: int = 15, flight_seconds: int = 120
) -> tuple:
    """
    Run the pipeline on synthetic pilots and score it against the hidden tiers.

    The tiers are per pilot and the groups are per Flight, so each Flight is
    scored against its own pilot's tier. The adjusted Rand index is 0 for
    chance and 1 for a perfect match however the groups are numbered.

    Tiers overlap by design in the generator, so 1.0 is not expected and would
    suggest the cohort is easier than real pilots.
    """
    rows = []
    for seed in seeds:
        flights, truth = synthetic_cohort(seed, n_pilots, flight_seconds)
        groups = fit_flights(flights, k, seed=0)
        scored = groups.assignments.merge(truth, on="pilot_id", how="left")
        rows.append(
            {
                "seed": seed,
                "pilots": flights["pilot_id"].nunique(),
                "flights": len(flights),
                "adjusted_rand": adjusted_rand_score(scored["tier"], scored["group"]),
            }
        )
    per_seed = pd.DataFrame(rows)
    summary = {
        "mean": float(per_seed["adjusted_rand"].mean()),
        "min": float(per_seed["adjusted_rand"].min()),
        "max": float(per_seed["adjusted_rand"].max()),
        "seeds": len(per_seed),
    }
    return per_seed, summary


# --- 4. Sensitivity: how much do the groups move? --------------------------


def _switched(baseline: pd.DataFrame, variant: pd.DataFrame, k: int) -> tuple:
    """How many Flights present in both landed in a different group."""
    merged = baseline.merge(variant, on=ID_COLUMNS, suffixes=("_base", "_variant"))
    if merged.empty:
        return 0, 0
    mapping = match_labels(
        merged["group_base"].to_numpy(), merged["group_variant"].to_numpy(), k
    )
    moved = sum(
        mapping.get(int(v), -1) != b
        for b, v in zip(merged["group_base"], merged["group_variant"])
    )
    return int(moved), len(merged)


def sensitivity(flights: pd.DataFrame, k: int, seed: int = 0) -> pd.DataFrame:
    """
    Refit under small changes and report how many Flights change group.

    #272 also asks for leg heading tolerance and settle window variants. The
    offline feature pipeline has neither: targets come straight from the
    target_* columns, and the settle window lives in live.py for the sidecar.
    They arrive with #229 and are left out rather than faked.
    """
    baseline = fit_flights(flights, k, seed).assignments

    halved = {metric: floor / 2 for metric, floor in LOG_FLOOR.items()}
    doubled = {metric: floor * 2 for metric, floor in LOG_FLOOR.items()}

    counts = flights["pilot_id"].value_counts()
    fewest = counts.index[-1]
    # Keyed on the pilot too: flight_id is only unique within a pilot, so
    # dropping by flight_id alone could take another pilot's Flight with it.
    spikiest = set(
        map(
            tuple,
            flights.nlargest(max(1, len(flights) // 10), "vertical_speed_std")[
                ID_COLUMNS
            ].to_numpy(),
        )
    )
    keep_spike_free = [
        (pilot, flight) not in spikiest
        for pilot, flight in zip(flights["pilot_id"], flights["flight_id"])
    ]

    variants = {
        "log floors halved": (flights, {"floors": halved}),
        "log floors doubled": (flights, {"floors": doubled}),
        f"pilot {fewest} removed ({counts.iloc[-1]} flight(s))": (
            flights[flights["pilot_id"] != fewest],
            {},
        ),
        "largest vertical-speed-spike flights removed": (
            flights[keep_spike_free],
            {},
        ),
    }

    rows = []
    for label, (subset, kwargs) in variants.items():
        if len(subset) < k:
            rows.append({"variant": label, "flights_compared": 0, "switched": np.nan,
                         "share_switched": np.nan, "note": "too few Flights to refit"})
            continue
        variant = fit_flights(subset, k, seed, **kwargs).assignments
        moved, compared = _switched(baseline, variant, k)
        rows.append({
            "variant": label,
            "flights_compared": compared,
            "switched": moved,
            "share_switched": moved / compared if compared else np.nan,
            "note": "",
        })
    return pd.DataFrame(rows)


# --- 5. Interpretation: what do the groups mean? ---------------------------

UNITS = {"altitude": "ft", "heading": "deg", "vertical_speed": "fpm", "airspeed": "kt"}
PLAIN = {
    "altitude": "altitude",
    "heading": "heading",
    "vertical_speed": "vertical speed",
    "airspeed": "airspeed",
}


def describe_groups(centres: pd.DataFrame) -> pd.DataFrame:
    """
    One plain sentence per group, for a reader who will not read the table.

    Each group's std is compared with the middle group's for that metric, so
    the wording is relative to this cohort and says nothing about pilots in
    general.
    """
    rows = []
    for _, centre in centres.iterrows():
        tight, loose = [], []
        for metric in METRICS:
            column = f"{metric}_std"
            median = centres[column].median()
            if centre[column] <= median * 0.75:
                tight.append(PLAIN[metric])
            elif centre[column] >= median * 1.25:
                loose.append(PLAIN[metric])
        parts = []
        if tight:
            parts.append("holds " + ", ".join(tight) + " tightly")
        if loose:
            parts.append("drifts on " + ", ".join(loose))
        if not parts:
            parts.append("sits near the middle on every factor")
        rows.append({
            "group": int(centre["group"]),
            "flights": int(centre["flights"]),
            "description": "; ".join(parts),
        })
    return pd.DataFrame(rows)


def _figure_paths(results_dir: Path) -> dict:
    return {
        "pca": results_dir / "groups_pca.png",
        "dendrogram": results_dir / "groups_dendrogram.png",
    }


def write_figures(flights: pd.DataFrame, groups: FlightGroups, results_dir: Path) -> dict:
    """
    The PCA scatter and the Ward dendrogram, both on the scaled features the
    model actually clusters, not the raw ones.

    Aggregates only: a point per Flight with no identifying label, per the
    data-handling rule that results may be published but the data may not.
    """
    import matplotlib

    matplotlib.use("Agg")  # No display on CI or a headless machine.
    import matplotlib.pyplot as plt
    from scipy.cluster.hierarchy import dendrogram, linkage
    from sklearn.decomposition import PCA

    features = feature_matrix(flights)
    scaled = groups.scaler.transform(_logged(features, groups.floors))
    assignments = groups.assignments
    paths = _figure_paths(results_dir)

    # Marker shape carries the pilot, colour carries the group, so a group
    # made of one pilot is visible as such.
    coordinates = PCA(n_components=2, random_state=0).fit_transform(scaled)
    markers = ["o", "s", "^", "D", "v", "P", "X", "*", "<", ">"]
    figure, axes = plt.subplots(figsize=(7, 5.5))
    for index, pilot in enumerate(sorted(assignments["pilot_id"].unique())):
        rows = (assignments["pilot_id"] == pilot).to_numpy()
        axes.scatter(
            coordinates[rows, 0],
            coordinates[rows, 1],
            c=assignments.loc[rows, "group"],
            cmap="viridis",
            marker=markers[index % len(markers)],
            s=70,
            edgecolors="black",
            linewidths=0.4,
            vmin=0,
            vmax=max(assignments["group"].max(), 1),
        )
    # Without this the colours cannot be tied back to the group numbers the
    # centres and descriptions tables use.
    highest = max(assignments["group"].max(), 1)
    colours = plt.get_cmap("viridis")
    axes.legend(
        handles=[
            plt.Line2D(
                [], [], marker="o", linestyle="", markersize=8,
                markerfacecolor=colours(group / highest), markeredgecolor="black",
                label=f"group {group}",
            )
            for group in sorted(assignments["group"].unique())
        ],
        loc="best",
        frameon=True,
    )
    axes.set_xlabel("PC1")
    axes.set_ylabel("PC2")
    axes.set_title("Flights in the clustered feature space\ncolour = group, shape = pilot")
    figure.tight_layout()
    figure.savefig(paths["pca"], dpi=150)
    plt.close(figure)

    figure, axes = plt.subplots(figsize=(8, 5))
    dendrogram(linkage(scaled, method="ward"), ax=axes, no_labels=True, color_threshold=None)
    axes.set_title("Ward linkage over the same scaled features")
    axes.set_ylabel("merge distance")
    figure.tight_layout()
    figure.savefig(paths["dendrogram"], dpi=150)
    plt.close(figure)

    return {name: path.name for name, path in paths.items()}


# --- 6. Metadata check -----------------------------------------------------
#
# NOT BUILT. #272 asks for Spearman correlation and Kruskal-Wallis of
# flight_hours across groups, plus a table of groups against ATP yes/no.
# No pilot metadata exists anywhere in the repository: #214 is still open and
# marked needs-info, and synthetic pilots have no flight hours to stand in.
# Writing it against invented hours would produce numbers that look like
# results, so it is left out and recorded as a gap in metrics.json.
#
# When #214 lands this belongs here, and ADR 0001 governs it: metadata
# interprets the groups afterwards and must never be used to choose k or the
# model.


# --- The one command -------------------------------------------------------

# Named so the write-up can cite a file rather than a number typed by hand.
TABLES = {
    "held_out_pilot": "held_out_pilot.csv",
    "sensitivity": "sensitivity.csv",
    "synthetic_recovery": "synthetic_recovery.csv",
    "group_centres": "group_centres.csv",
    "group_descriptions": "group_descriptions.csv",
}

NOT_BUILT = {
    "metadata_check": (
        "No pilot metadata exists (#214 open, needs-info), so the flight_hours "
        "correlation, the Kruskal-Wallis test and the certification table from "
        "#272 section 6 are not reported."
    ),
    "leg_tolerance_sensitivity": (
        "The offline feature pipeline has no Legs and no settle window; targets "
        "come from the target_* columns directly. Arrives with #229."
    ),
    "real_data": (
        "The client's Prepar3D flights are not in the repository (#228, #270), "
        "so every number here is from synthetic pilots."
    ),
    "model": (
        "The per-Flight model is #271 and is still open. fit_flights() in this "
        "module is a stand-in using the ADR 0002 treatment so the tests could be "
        "written and checked; swap it for #271's model when it lands."
    ),
}


def main(argv=None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--flights", type=Path, help="Per-Flight features CSV; omit to use synthetic pilots"
    )
    parser.add_argument("--truth", type=Path, help="True tier per pilot, for synthetic data")
    parser.add_argument("--k", type=int, default=3, help="Number of groups")
    parser.add_argument("--seed", type=int, default=0, help="Random seed")
    parser.add_argument(
        "--permutations", type=int, default=DEFAULT_PERMUTATIONS, help="Shuffles for the null"
    )
    parser.add_argument("--synthetic-pilots", type=int, default=15)
    parser.add_argument("--flight-seconds", type=int, default=120)
    parser.add_argument(
        "--synthetic-seeds",
        type=int,
        default=len(SYNTHETIC_SEEDS),
        help="How many generator seeds the recovery test runs",
    )
    parser.add_argument("--results-dir", type=Path, default=RESULTS_DIR)
    parser.add_argument("--no-figures", action="store_true", help="Skip the PNGs")
    args = parser.parse_args(argv)

    try:
        if args.flights is not None:
            flights = pd.read_csv(args.flights)
            truth = pd.read_csv(args.truth) if args.truth is not None else None
            source = str(args.flights)
        else:
            flights, truth = synthetic_cohort(
                args.seed, args.synthetic_pilots, args.flight_seconds
            )
            source = f"synthetic: {args.synthetic_pilots} pilots, seed {args.seed}"

        groups = fit_flights(flights, args.k, args.seed)
        per_pilot, overall = held_out_pilot(flights, args.k, args.seed)
        permutation = consistency_permutation_test(
            groups.assignments, args.permutations, args.seed
        )
        moves = sensitivity(flights, args.k, args.seed)
        recovery, recovery_summary = synthetic_recovery(
            args.k,
            SYNTHETIC_SEEDS[: args.synthetic_seeds],
            args.synthetic_pilots,
            args.flight_seconds,
        )
        descriptions = describe_groups(groups.centres)
    except (OSError, ValueError) as e:
        parser.error(str(e))

    results_dir = args.results_dir
    results_dir.mkdir(parents=True, exist_ok=True)
    for frame, name in [
        (per_pilot, TABLES["held_out_pilot"]),
        (moves, TABLES["sensitivity"]),
        (recovery, TABLES["synthetic_recovery"]),
        (groups.centres, TABLES["group_centres"]),
        (descriptions, TABLES["group_descriptions"]),
    ]:
        frame.to_csv(results_dir / name, index=False)

    figures = {}
    if not args.no_figures:
        figures = write_figures(flights, groups, results_dir)

    metrics = {
        "source": source,
        "k": args.k,
        "seed": args.seed,
        "pilots": int(flights["pilot_id"].nunique()),
        "flights": int(len(flights)),
        "held_out_pilot": {"overall_agreement": overall},
        "pilot_consistency": permutation,
        "synthetic_recovery": recovery_summary,
        "sensitivity": {
            row["variant"]: None if pd.isna(row["share_switched"]) else row["share_switched"]
            for _, row in moves.iterrows()
        },
        "tables": TABLES,
        "figures": figures,
        "not_built": NOT_BUILT,
    }
    (results_dir / "metrics.json").write_text(json.dumps(metrics, indent=2) + "\n")

    print(f"Source: {source}")
    print(f"{metrics['pilots']} pilots, {metrics['flights']} Flights, k={args.k}\n")
    print(f"Held-out pilot agreement: {overall:.3f}")
    print(
        f"Pilot consistency: {permutation['observed']:.3f} "
        f"(shuffled mean {permutation['null_mean']:.3f}, p={permutation['p_value']:.4f})"
    )
    print(
        f"Synthetic recovery (adjusted Rand): mean {recovery_summary['mean']:.3f}, "
        f"range {recovery_summary['min']:.3f} to {recovery_summary['max']:.3f} "
        f"over {recovery_summary['seeds']} seeds\n"
    )
    print(moves.to_string(index=False, float_format="%.3f"))
    print()
    print(descriptions.to_string(index=False))
    print(f"\nWrote tables, figures and metrics.json to {results_dir}")
    print("Not built, see metrics.json: " + ", ".join(NOT_BUILT))


if __name__ == "__main__":
    main()
