import numpy as np
import pandas as pd
import pytest

from clustering import fit, sweep, truth_agreement
from features import METRICS, STATISTICS, compute_features
from generate_synthetic_pilots import generate_pilots


def make_pilots(rows):
    """
    A per-pilot feature table shaped like features.pilot_features output.

    Each row is (pilot_id, {column: value}); unspecified features are 1.0 for
    std and 0.0 for mad and bias.
    """
    records = []
    for pilot_id, overrides in rows:
        record = {"pilot_id": pilot_id, "flights": 2}
        for metric in METRICS:
            for stat in STATISTICS:
                record[f"{metric}_{stat}"] = 1.0 if stat == "std" else 0.0
        record.update(overrides)
        records.append(record)
    return pd.DataFrame(records)


def group_of(assignments, pilot_id):
    return assignments.set_index("pilot_id").loc[pilot_id, "group"]


def steady_and_wobbly_pilots():
    """Four pilots holding steady 60 ft off target (two high, two low), four wobbling around it."""
    return make_pilots(
        [
            ("high1", {"altitude_bias": 60.0, "altitude_std": 5.0}),
            ("low1", {"altitude_bias": -60.0, "altitude_std": 5.0}),
            ("high2", {"altitude_bias": 60.0, "altitude_std": 5.0}),
            ("low2", {"altitude_bias": -60.0, "altitude_std": 5.0}),
            ("wobbly1", {"altitude_std": 60.0}),
            ("wobbly2", {"altitude_std": 60.0}),
            ("wobbly3", {"altitude_std": 60.0}),
            ("wobbly4", {"altitude_std": 60.0}),
        ]
    )


def test_pilots_off_target_in_opposite_directions_share_a_group():
    assignments, _ = fit(steady_and_wobbly_pilots(), k=2)

    steady = {group_of(assignments, p) for p in ("high1", "low1", "high2", "low2")}
    wobbly = {group_of(assignments, p) for p in ("wobbly1", "wobbly2", "wobbly3", "wobbly4")}
    assert len(steady) == 1
    assert len(wobbly) == 1
    assert steady != wobbly


def test_sub_foot_bias_differences_do_not_decide_groups():
    # Tight and loose pilots differ by 4 ft of wobble, and within each, by a
    # meaningless 0.09 ft of bias. Pilots 50 ft off target give the bias axis
    # a real spread; without a floor on the log, 0.01 vs 0.1 ft would
    # look as far apart as 5 vs 50 ft and split the groups the wrong way.
    pilots = make_pilots(
        [
            (f"{size}{i}", {"altitude_std": std, "altitude_bias": bias})
            for size, std in (("tight", 8.0), ("loose", 12.0))
            for i, bias in enumerate((0.01, 0.1, 0.01, 0.1))
        ]
        + [
            (f"off{i}", {"altitude_std": 60.0, "altitude_bias": bias})
            for i, bias in enumerate((50.0, -50.0, 50.0, -50.0))
        ]
    )
    assignments, _ = fit(pilots, k=3)

    tight = {group_of(assignments, f"tight{i}") for i in range(4)}
    loose = {group_of(assignments, f"loose{i}") for i in range(4)}
    assert len(tight) == 1
    assert len(loose) == 1
    assert tight != loose


def test_centres_are_in_raw_units():
    assignments, centres = fit(steady_and_wobbly_pilots(), k=2)
    centres = centres.set_index("group")

    steady = centres.loc[group_of(assignments, "high1")]
    assert steady["altitude_abs_bias"] == pytest.approx(60.0)
    assert steady["altitude_std"] == pytest.approx(5.0)
    assert steady["pilots"] == 4

    wobbly = centres.loc[group_of(assignments, "wobbly1")]
    assert wobbly["altitude_abs_bias"] == pytest.approx(0.0)
    assert wobbly["altitude_std"] == pytest.approx(60.0)
    assert wobbly["pilots"] == 4


@pytest.mark.parametrize(
    "n_pilots, expected_ks",
    [(20, [2, 3, 4, 5, 6]), (15, [2, 3, 4, 5]), (9, [2, 3]), (6, [2])],
)
def test_sweep_covers_k_from_2_to_min_of_6_and_a_third_of_pilots(n_pilots, expected_ks):
    rng = np.random.default_rng(0)
    pilots = make_pilots(
        [(f"p{i}", {"altitude_std": rng.uniform(1, 100)}) for i in range(n_pilots)]
    )
    assert sweep(pilots, n_subsamples=5)["k"].tolist() == expected_ks


def test_sweep_needs_at_least_6_pilots():
    pilots = make_pilots([(f"p{i}", {"altitude_std": float(i)}) for i in range(5)])
    with pytest.raises(ValueError, match="6"):
        sweep(pilots)


def test_well_separated_groups_score_as_stable_and_cohesive():
    report = sweep(steady_and_wobbly_pilots()).set_index("k")
    assert report.loc[2, "stability"] == pytest.approx(1.0)
    assert report.loc[2, "silhouette"] > 0.9


def test_truth_agreement_ignores_how_groups_are_numbered():
    assignments = pd.DataFrame({"pilot_id": ["a", "b", "c", "d"], "group": [0, 0, 1, 1]})
    truth = pd.DataFrame({"pilot_id": ["d", "c", "b", "a"], "tier": [1, 1, 3, 3]})
    assert truth_agreement(assignments, truth) == pytest.approx(1.0)


def test_truth_agreement_fails_naming_pilots_missing_from_truth():
    assignments = pd.DataFrame({"pilot_id": ["a", "b", "c"], "group": [0, 1, 1]})
    truth = pd.DataFrame({"pilot_id": ["a", "b"], "tier": [1, 2]})
    with pytest.raises(ValueError, match="c"):
        truth_agreement(assignments, truth)


def test_synthetic_pilots_are_grouped_by_their_hidden_tier():
    # Averaged over cohorts, since 15 pilots make any single cohort noisy.
    scores = []
    for seed in range(6):
        telemetry, truth = generate_pilots(seed=seed)
        _, pilots = compute_features(telemetry)
        assignments, _ = fit(pilots, k=3)
        scores.append(truth_agreement(assignments, truth))
    assert np.mean(scores) >= 0.8


def test_nan_feature_fails_naming_pilot_and_metric():
    pilots = make_pilots(
        [
            ("p1", {}),
            ("p2", {"heading_std": float("nan"), "heading_bias": float("nan")}),
            ("p3", {}),
            ("p4", {"airspeed_bias": float("nan")}),
        ]
    )
    with pytest.raises(ValueError) as error:
        fit(pilots, k=2)
    message = str(error.value)
    assert "p2" in message and "heading" in message
    assert "p4" in message and "airspeed" in message
    assert "p1" not in message and "p3" not in message
