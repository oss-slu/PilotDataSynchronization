import numpy as np
import pandas as pd
import pytest

from pilot_inference.performance.clustering import fit, fit_reference, sweep, truth_agreement
from pilot_inference.performance.features import METRICS, STATISTICS, compute_features
from pilot_inference.performance.generate_synthetic_pilots import generate_pilots


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
            ("high1", {"altitude_abs_bias": 60.0, "altitude_std": 5.0}),
            ("low1", {"altitude_abs_bias": 60.0, "altitude_std": 5.0}),
            ("high2", {"altitude_abs_bias": 60.0, "altitude_std": 5.0}),
            ("low2", {"altitude_abs_bias": 60.0, "altitude_std": 5.0}),
            ("wobbly1", {"altitude_std": 60.0}),
            ("wobbly2", {"altitude_std": 60.0}),
            ("wobbly3", {"altitude_std": 60.0}),
            ("wobbly4", {"altitude_std": 60.0}),
        ]
    )


def test_pilots_equally_far_off_target_share_a_group():
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
            (f"{size}{i}", {"altitude_std": std, "altitude_abs_bias": bias})
            for size, std in (("tight", 8.0), ("loose", 12.0))
            for i, bias in enumerate((0.01, 0.1, 0.01, 0.1))
        ]
        + [
            (f"off{i}", {"altitude_std": 60.0, "altitude_abs_bias": bias})
            for i, bias in enumerate((50.0, 50.0, 50.0, 50.0))
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


@pytest.mark.parametrize(
    "k",
    [
        pytest.param(1, id="one-group-for-everyone"),
        pytest.param(6, id="just-over-a-third-of-the-pilots"),
        pytest.param(15, id="one-pilot-per-group"),
    ],
)
def test_fit_rejects_k_the_sweep_would_not_offer(k):
    # 15 pilots allow k of 2 to 5, so that groups average 3 or more pilots.
    rng = np.random.default_rng(0)
    pilots = make_pilots(
        [(f"p{i}", {"altitude_std": rng.uniform(1, 100)}) for i in range(15)]
    )
    with pytest.raises(ValueError, match=r"2 to 5"):
        fit(pilots, k=k)


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


def test_truth_agreement_fails_naming_pilots_repeated_in_truth():
    # A repeat would turn 3 pilots into 4 rows and quietly shift the score.
    assignments = pd.DataFrame({"pilot_id": ["a", "b", "c"], "group": [0, 1, 1]})
    truth = pd.DataFrame({"pilot_id": ["a", "b", "c", "b"], "tier": [1, 2, 2, 3]})
    with pytest.raises(ValueError, match="repeated in truth: b"):
        truth_agreement(assignments, truth)


def test_synthetic_pilots_are_grouped_by_their_hidden_tier():
    # Averaged over the 12 cohorts of docs/adr/0002, since 15 pilots make any
    # single cohort noisy. The tiers overlap, so this is well short of 1. The
    # bar sits above a linear scale and the per-pilot |mean bias| (both about
    # 0.5 here) and below the chosen features; it does not separate the mad
    # options, which score about the same.
    scores = []
    for seed in range(12):
        telemetry, truth = generate_pilots(seed=seed)
        _, pilots = compute_features(telemetry)
        assignments, _ = fit(pilots, k=3)
        scores.append(truth_agreement(assignments, truth))
    assert np.mean(scores) >= 0.7


def test_nan_feature_fails_naming_pilot_and_metric():
    pilots = make_pilots(
        [
            ("p1", {}),
            ("p2", {"heading_std": float("nan"), "heading_abs_bias": float("nan")}),
            ("p3", {}),
            ("p4", {"airspeed_abs_bias": float("nan")}),
        ]
    )
    with pytest.raises(ValueError) as error:
        fit(pilots, k=2)
    message = str(error.value)
    assert "p2" in message and "heading" in message
    assert "p4" in message and "airspeed" in message
    assert "p1" not in message and "p3" not in message


def _telemetry_with_altitude_offsets(offsets_by_pilot, seed=0):
    """Two-flight telemetry; each pilot's altitude sits at target + that flight's offset."""
    rng = np.random.default_rng(seed)
    frames = []
    for pilot_id, offsets in offsets_by_pilot.items():
        for flight, offset in enumerate(offsets):
            n = 60
            frames.append(
                pd.DataFrame(
                    {
                        "pilot_id": pilot_id,
                        "flight_id": f"{pilot_id}-{flight}",
                        "altitude": 5000.0 + offset + rng.normal(0, 5.0, n),
                        "heading": 90.0,
                        "vertical_speed": 0.0,
                        "velocity": 120.0,
                        "target_altitude": 5000.0,
                        "target_heading": 90.0,
                        "target_vertical_speed": 0.0,
                        "target_airspeed": 120.0,
                    }
                )
            )
    return pd.concat(frames, ignore_index=True)


def test_pilot_off_target_in_opposite_directions_on_different_flights_is_not_on_target():
    # Averaging signed bias across flights first would make the alternating
    # pilots look exactly like the on-target ones.
    offsets = {f"alt{i}": (60.0, -60.0) for i in range(4)}
    offsets.update({f"ontarget{i}": (0.0, 0.0) for i in range(4)})
    _, pilots = compute_features(_telemetry_with_altitude_offsets(offsets))

    assignments, _ = fit(pilots, k=2)

    alternating = {group_of(assignments, f"alt{i}") for i in range(4)}
    on_target = {group_of(assignments, f"ontarget{i}") for i in range(4)}
    assert len(alternating) == 1
    assert len(on_target) == 1
    assert alternating != on_target


def synthetic_cohort(seed=0):
    telemetry, truth = generate_pilots(n_pilots=15, seed=seed, flight_seconds=30)
    _, pilots = compute_features(telemetry)
    return pilots


def test_reference_groups_are_what_fit_returns():
    pilots = synthetic_cohort()
    assignments, centres = fit(pilots, k=3)
    reference = fit_reference(pilots, k=3)
    pd.testing.assert_frame_equal(reference.assignments, assignments)
    pd.testing.assert_frame_equal(reference.centres, centres)


def test_assigning_the_cohort_reproduces_its_groups():
    pilots = synthetic_cohort()
    reference = fit_reference(pilots, k=3)
    pd.testing.assert_frame_equal(reference.assign(pilots), reference.assignments)


def test_a_new_pilot_flying_like_a_member_joins_that_members_group():
    pilots = synthetic_cohort()
    reference = fit_reference(pilots, k=3)
    twin = pilots[pilots["pilot_id"] == "P15"].assign(pilot_id="live")
    assert reference.assign(twin)["group"].iloc[0] == group_of(reference.assignments, "P15")


def test_assigning_does_not_move_the_groups():
    pilots = synthetic_cohort()
    reference = fit_reference(pilots, k=3)
    before = reference.kmeans.cluster_centers_.copy()
    reference.assign(synthetic_cohort(seed=1))
    np.testing.assert_array_equal(reference.kmeans.cluster_centers_, before)


def test_assigning_a_pilot_with_nan_features_is_refused():
    pilots = synthetic_cohort()
    reference = fit_reference(pilots, k=3)
    gap = pilots.head(1).assign(pilot_id="live", altitude_std=np.nan)
    with pytest.raises(ValueError, match="live"):
        reference.assign(gap)
