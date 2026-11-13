"""
Tests for the clustering evaluation (#272).

Synthetic pilots only. The client's flights are not in the repository, and a
test that needed them could not run anywhere.

Cohorts are kept small and permutation counts low: these tests check that each
measure behaves, not that any particular number comes out, because the numbers
are the thing under study.
"""

import json

import numpy as np
import pandas as pd
import pytest

from pilot_inference.performance import evaluate


@pytest.fixture(scope="module")
def cohort():
    """A small synthetic cohort, built once: generating it dominates runtime."""
    flights, truth = evaluate.synthetic_cohort(seed=0, n_pilots=9, flight_seconds=20)
    return flights, truth


def labelling(groups):
    return pd.DataFrame(
        {
            "pilot_id": ["A", "A", "B", "B", "C", "C"],
            "flight_id": [f"f{i}" for i in range(6)],
            "group": groups,
        }
    )


# --- comparing two labellings ---------------------------------------------


def test_renumbering_a_labelling_is_not_a_change():
    """
    Two fits can agree completely and still number their groups differently.
    Without this, every sensitivity row would report spurious movement.
    """
    base = labelling([0, 0, 1, 1, 2, 2])
    renamed = base.assign(group=base["group"].map({0: 2, 1: 0, 2: 1}))

    assert evaluate._switched(base, renamed, 3) == (0, 6)


def test_a_real_move_is_counted():
    base = labelling([0, 0, 1, 1, 2, 2])
    moved = base.copy()
    moved.loc[0, "group"] = 1

    assert evaluate._switched(base, moved, 3) == (1, 6)


def test_match_labels_picks_the_best_overall_pairing():
    reference = np.array([0, 0, 0, 1, 1, 2])
    other = np.array([2, 2, 2, 0, 0, 1])

    assert evaluate.match_labels(reference, other, 3) == {2: 0, 0: 1, 1: 2}


# --- features --------------------------------------------------------------


def test_missing_feature_columns_name_the_fix(cohort):
    flights, _ = cohort
    with pytest.raises(ValueError, match="compute_features"):
        evaluate.feature_matrix(flights.drop(columns=["altitude_std"]))


def test_a_nan_feature_names_the_flight(cohort):
    flights, _ = cohort
    broken = flights.copy()
    broken.loc[broken.index[0], "heading_abs_bias"] = np.nan

    with pytest.raises(ValueError, match="NaN deviation features"):
        evaluate.feature_matrix(broken)


def test_one_row_per_flight(cohort):
    flights, _ = cohort
    features = evaluate.feature_matrix(flights)

    assert len(features) == len(flights)
    assert features.index.names == ["pilot_id", "flight_id"]


# --- 1. held-out pilot -----------------------------------------------------


def test_every_pilot_is_held_out_in_turn(cohort):
    flights, _ = cohort
    per_pilot, overall = evaluate.held_out_pilot(flights, k=3)

    assert len(per_pilot) == flights["pilot_id"].nunique()
    assert set(per_pilot["pilot_id"]) == set(flights["pilot_id"])
    assert 0.0 <= overall <= 1.0
    # The held-out pilot's own Flights must not be in the model that places
    # them, so agreement is a real out-of-sample number and can be short of 1.
    assert per_pilot["flights"].sum() == len(flights)


# --- 2. consistency and its null -------------------------------------------


def test_consistency_is_one_when_each_pilot_sits_in_one_group():
    assert evaluate.consistency(labelling([0, 0, 1, 1, 2, 2])) == 1.0


def test_consistency_falls_when_a_pilots_flights_split():
    assert evaluate.consistency(labelling([0, 1, 1, 1, 2, 2])) < 1.0


def test_single_flight_pilots_are_left_out_of_consistency():
    """They score 1.0 by construction and would inflate the average."""
    one_each = pd.DataFrame(
        {"pilot_id": ["A", "B", "C"], "flight_id": ["f1", "f2", "f3"], "group": [0, 1, 2]}
    )

    assert np.isnan(evaluate.consistency(one_each))


def test_the_permutation_test_reports_a_usable_p_value(cohort):
    flights, _ = cohort
    groups = evaluate.fit_flights(flights, k=3)

    result = evaluate.consistency_permutation_test(groups.assignments, n_permutations=100)

    # Never 0: a finite number of shuffles cannot show that.
    assert 0 < result["p_value"] <= 1
    assert result["observed"] >= result["null_mean"]
    assert result["permutations"] == 100


def test_shuffling_destroys_the_pilot_signal(cohort):
    """
    The null has to be a real null. If a shuffled labelling scored as well as
    the real one, the test could never distinguish groups from noise.
    """
    flights, _ = cohort
    groups = evaluate.fit_flights(flights, k=3)

    result = evaluate.consistency_permutation_test(groups.assignments, n_permutations=200)

    assert result["null_mean"] < result["observed"]


# --- 3. synthetic recovery -------------------------------------------------


def test_recovery_scores_every_seed():
    per_seed, summary = evaluate.synthetic_recovery(
        k=3, seeds=(0, 1), n_pilots=9, flight_seconds=20
    )

    assert list(per_seed["seed"]) == [0, 1]
    assert summary["seeds"] == 2
    assert summary["min"] <= summary["mean"] <= summary["max"]
    # Adjusted Rand is 0 at chance and 1 for a perfect match. Tiers overlap by
    # design, so this pins the range rather than a value.
    assert all(-1.0 <= value <= 1.0 for value in per_seed["adjusted_rand"])


def test_recovery_beats_chance_on_synthetic_tiers():
    """If the pipeline cannot recover tiers it built itself, nothing else matters."""
    _, summary = evaluate.synthetic_recovery(k=3, seeds=(0,), n_pilots=9, flight_seconds=20)

    assert summary["mean"] > 0.3


# --- 4. sensitivity --------------------------------------------------------


def test_sensitivity_reports_each_variant(cohort):
    flights, _ = cohort
    moves = evaluate.sensitivity(flights, k=3)

    assert len(moves) == 4
    assert {"variant", "flights_compared", "switched", "share_switched"} <= set(moves.columns)


def test_a_large_enough_floor_change_does_move_flights(cohort):
    """
    Guards the sensitivity rows that report 0. A comparison that always
    returned 0 would look identical, so one variant must be able to move.
    """
    flights, _ = cohort
    baseline = evaluate.fit_flights(flights, k=3).assignments
    extreme = {metric: floor * 500 for metric, floor in evaluate.LOG_FLOOR.items()}

    moved, compared = evaluate._switched(
        baseline, evaluate.fit_flights(flights, k=3, floors=extreme).assignments, 3
    )

    assert compared == len(flights)
    assert moved > 0


# --- 5. interpretation -----------------------------------------------------


def test_each_group_gets_a_description(cohort):
    flights, _ = cohort
    groups = evaluate.fit_flights(flights, k=3)

    descriptions = evaluate.describe_groups(groups.centres)

    assert len(descriptions) == 3
    assert all(text for text in descriptions["description"])


def test_centres_are_in_raw_units_not_log_space(cohort):
    """
    ADR 0002: the centre is the mean of the group's Flights in feet and
    degrees, so a group can be read without undoing the scaling.
    """
    flights, _ = cohort
    groups = evaluate.fit_flights(flights, k=3)

    assert groups.centres["flights"].sum() == len(flights)
    for column in ("altitude_std", "airspeed_std"):
        assert (groups.centres[column] > 0).all()
        assert groups.centres[column].max() <= flights[column].max()


# --- the one command -------------------------------------------------------


def test_one_command_writes_every_table_and_metrics(tmp_path):
    evaluate.main(
        [
            "--synthetic-pilots", "9",
            "--flight-seconds", "20",
            "--permutations", "25",
            "--synthetic-seeds", "1",
            "--results-dir", str(tmp_path),
            "--no-figures",
        ]
    )

    for name in evaluate.TABLES.values():
        assert (tmp_path / name).exists(), name

    metrics = json.loads((tmp_path / "metrics.json").read_text())
    assert metrics["pilots"] == 9
    assert metrics["k"] == 3
    assert 0.0 <= metrics["held_out_pilot"]["overall_agreement"] <= 1.0
    assert 0 < metrics["pilot_consistency"]["p_value"] <= 1
    # The gaps are recorded with the results, so a reader of metrics.json
    # cannot mistake what was not measured for a measurement of zero.
    assert set(metrics["not_built"]) == set(evaluate.NOT_BUILT)


def test_figures_are_written_when_asked(tmp_path):
    evaluate.main(
        [
            "--synthetic-pilots", "9",
            "--flight-seconds", "20",
            "--permutations", "10",
            "--synthetic-seeds", "1",
            "--results-dir", str(tmp_path),
        ]
    )

    metrics = json.loads((tmp_path / "metrics.json").read_text())
    assert set(metrics["figures"]) == {"pca", "dendrogram"}
    for name in metrics["figures"].values():
        assert (tmp_path / name).stat().st_size > 0


def test_a_bad_k_is_a_usage_error(tmp_path):
    with pytest.raises(SystemExit) as exit_info:
        evaluate.main(
            [
                "--synthetic-pilots", "9",
                "--flight-seconds", "20",
                "--k", "1",
                "--results-dir", str(tmp_path),
                "--no-figures",
            ]
        )

    assert exit_info.value.code == 2
    assert not (tmp_path / "metrics.json").exists()
