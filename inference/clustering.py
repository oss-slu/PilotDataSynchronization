"""
Performance groups from per-pilot deviation features.

Clusters pilots on the log of std and |bias| for each flight dynamics factor
(docs/adr/0002). Pilot metadata is not an input (docs/adr/0001). Pure
DataFrame-in functions; cluster_pilots.py is the CLI wrapper.
"""

import numpy as np
import pandas as pd
from sklearn.cluster import KMeans
from sklearn.metrics import adjusted_rand_score, silhouette_score
from sklearn.preprocessing import StandardScaler

from features import METRICS

FEATURE_PARTS = ("std", "abs_bias")
FEATURE_COLUMNS = [f"{metric}_{part}" for metric in METRICS for part in FEATURE_PARTS]

# Added before taking the log, in raw units, so near-zero differences (0.01 vs
# 0.1 ft of bias) don't count as much as real ones (10 vs 100 ft).
LOG_FLOOR = {"altitude": 1.0, "heading": 0.1, "vertical_speed": 2.0, "airspeed": 0.1}

# Share of pilots each stability refit keeps.
SUBSAMPLE_FRACTION = 0.8


def clustering_features(pilots: pd.DataFrame) -> pd.DataFrame:
    """
    The clustering inputs, one row per pilot indexed by pilot_id, in raw units.

    Raises if any pilot has a NaN feature, naming each pilot and metric.
    Dropping or imputing would quietly reshape a cohort of 10 to 20 pilots.
    """
    features = pd.DataFrame(index=pilots["pilot_id"])
    for metric in METRICS:
        features[f"{metric}_std"] = pilots[f"{metric}_std"].to_numpy()
        features[f"{metric}_abs_bias"] = pilots[f"{metric}_bias"].abs().to_numpy()

    gaps = []
    for pilot_id, row in features.iterrows():
        metrics = [m for m in METRICS if row[[f"{m}_std", f"{m}_abs_bias"]].isna().any()]
        if metrics:
            gaps.append(f"{pilot_id} ({', '.join(metrics)})")
    if gaps:
        raise ValueError(f"NaN deviation features for pilot(s): {'; '.join(gaps)}")
    return features[FEATURE_COLUMNS]


def _scaled(features: pd.DataFrame) -> np.ndarray:
    """
    Log, then standardize over the cohort being clustered (never per pilot).

    Deviation sizes are positive and pilots differ by multiples, so on a
    linear scale the worst pilots stretch the axis and the rest bunch up.
    """
    floors = pd.Series(
        {f"{metric}_{part}": LOG_FLOOR[metric] for metric in METRICS for part in FEATURE_PARTS}
    )
    return StandardScaler().fit_transform(np.log(features + floors[features.columns]))


def _kmeans_labels(scaled: np.ndarray, k: int, seed: int) -> np.ndarray:
    return KMeans(n_clusters=k, n_init=10, random_state=seed).fit_predict(scaled)


def candidate_ks(n_pilots: int) -> range:
    """k from 2 to min(6, n // 3), so every group can average 3 or more pilots."""
    if n_pilots < 6:
        raise ValueError(f"Need at least 6 pilots to sweep k, got {n_pilots}")
    return range(2, min(6, n_pilots // 3) + 1)


def sweep(
    pilots: pd.DataFrame,
    seed: int = 0,
    n_subsamples: int = 50,
) -> pd.DataFrame:
    """
    Report how well each candidate k fits, without choosing one.

    One row per k with:
        silhouette  cohesion vs separation of the full-cohort fit, in [-1, 1]
        stability   mean adjusted Rand index between the full-cohort fit and
                    refits on random subsamples (SUBSAMPLE_FRACTION of the pilots),
                    compared on the pilots each subsample kept; 1 is identical

    Stability is measured on subsamples rather than KMeans seeds, because with
    n_init=10 a small cohort gets nearly the same partition from any seed.
    """
    features = clustering_features(pilots)
    scaled = _scaled(features)
    n = len(features)
    size = round(SUBSAMPLE_FRACTION * n)
    rng = np.random.default_rng(seed)
    rows = []
    for k in candidate_ks(n):
        labels = _kmeans_labels(scaled, k, seed)
        scores = []
        for _ in range(n_subsamples):
            kept = np.sort(rng.choice(n, size=size, replace=False))
            # Rescale on the subsample, as a fresh run on those pilots would.
            refit = _kmeans_labels(_scaled(features.iloc[kept]), k, seed)
            scores.append(adjusted_rand_score(labels[kept], refit))
        rows.append(
            {
                "k": k,
                "silhouette": silhouette_score(scaled, labels),
                "stability": float(np.mean(scores)),
            }
        )
    return pd.DataFrame(rows)


def fit(pilots: pd.DataFrame, k: int, seed: int = 0):
    """
    Cluster pilots into k performance groups.

    Returns (assignments, centres): assignments has pilot_id and group;
    centres has one row per group with its pilot count and the mean of each
    feature in raw units (ft, deg, ft/min, kts), so a group can be read
    without undoing the scaling.
    """
    features = clustering_features(pilots)
    labels = _kmeans_labels(_scaled(features), k, seed)
    assignments = pd.DataFrame({"pilot_id": features.index, "group": labels})

    # The raw-unit mean of each group's members, not the KMeans centre, which
    # lives in log-scaled space.
    grouped = features.groupby(labels)
    centres = grouped.mean()
    centres.insert(0, "pilots", grouped.size())
    centres = centres.rename_axis("group").reset_index()
    return assignments, centres


def truth_agreement(assignments: pd.DataFrame, truth: pd.DataFrame) -> float:
    """
    Adjusted Rand index between performance groups and known tiers.

    For synthetic pilots only: truth has pilot_id and tier. 1 means the groups
    match the tiers exactly, however they are numbered; about 0 is chance.
    """
    merged = assignments.merge(truth, on="pilot_id", how="left")
    missing = merged.loc[merged["tier"].isna(), "pilot_id"].tolist()
    if missing:
        raise ValueError(f"Pilot(s) missing from truth: {', '.join(map(str, missing))}")
    return float(adjusted_rand_score(merged["tier"], merged["group"]))
