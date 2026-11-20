"""
Tests for the ADR 0002 feature-set comparison (#230).

Short flights and few seeds throughout: these check that each measurement
behaves, not that a particular number comes out, since the numbers are the
thing being studied.
"""

import numpy as np
import pytest

from pilot_inference.performance import compare_feature_sets as compare
from pilot_inference.performance.features import METRICS
from pilot_inference.performance.generate_synthetic_pilots import (
    generate_pilots,
    independent_pilots,
)


# --- the generator's new behaviour ----------------------------------------


def test_shared_scales_are_the_default():
    """
    Existing cohorts and the ADR's published numbers must not move because of
    this change.
    """
    _, truth = generate_pilots(n_pilots=6, seed=0, flight_seconds=5)

    assert (truth["bias_over_wobble"] == 1.0).all()


def test_independent_pilots_get_a_second_draw():
    _, truth = generate_pilots(n_pilots=6, seed=0, flight_seconds=5, independent_share=1.0)

    assert (truth["bias_over_wobble"] != 1.0).all()


def test_the_share_is_spread_across_tiers_not_taken_from_the_front():
    """
    Pilots are laid out tier by tier, so a prefix would put every independent
    pilot in tier 1 and leave the other tiers untested.
    """
    _, truth = generate_pilots(n_pilots=9, seed=0, flight_seconds=5, independent_share=1 / 3)

    independent = truth[truth["bias_over_wobble"] != 1.0]
    assert len(independent) == 3
    assert independent["tier"].nunique() == 3


def test_independent_pilots_picks_an_even_spread():
    assert independent_pilots(12, 0.0) == set()
    assert independent_pilots(12, 1.0) == set(range(12))
    assert len(independent_pilots(12, 0.5)) == 6
    assert independent_pilots(12, 0.5) == {0, 2, 4, 6, 8, 10}


def test_a_share_outside_zero_to_one_is_refused():
    with pytest.raises(ValueError, match="independent_share"):
        generate_pilots(n_pilots=6, seed=0, flight_seconds=5, independent_share=1.5)


def test_both_kinds_of_pilot_appear():
    """
    The point of #230. Independent draws must actually produce pilots whose
    steadiness and wobble disagree, not merely accept the parameter.
    """
    _, truth = generate_pilots(n_pilots=18, seed=0, flight_seconds=10, independent_share=1.0)
    ratios = truth["bias_over_wobble"]

    assert (ratios > 1.3).any(), "no steadily-off-target pilot"
    assert (ratios < 0.77).any(), "no on-target-but-oscillating pilot"


def test_truth_still_carries_tier_for_the_truth_flag():
    """truth_agreement() merges on pilot_id and reads tier; the extra column must not disturb it."""
    _, truth = generate_pilots(n_pilots=6, seed=0, flight_seconds=5, independent_share=1.0)

    assert {"pilot_id", "tier"} <= set(truth.columns)
    assert truth["pilot_id"].is_unique
    assert set(truth["tier"]) <= {1, 2, 3}


def test_the_telemetry_itself_still_looks_like_flights():
    telemetry, _ = generate_pilots(n_pilots=6, seed=0, flight_seconds=5, independent_share=1.0)

    assert telemetry["heading"].between(0, 360).all()
    assert not telemetry.isna().any().any()


# --- scoring ---------------------------------------------------------------


def test_every_feature_column_has_a_log_floor():
    for columns in compare.FEATURE_SETS.values():
        for column in columns:
            assert compare._floor_for(column) > 0


def test_an_unknown_column_has_no_floor():
    with pytest.raises(ValueError, match="No log floor"):
        compare._floor_for("roll_std")


def test_scoring_a_missing_column_says_which():
    telemetry, truth = generate_pilots(n_pilots=6, seed=0, flight_seconds=5)
    from pilot_inference.performance.features import compute_features

    _, pilots = compute_features(telemetry)

    with pytest.raises(ValueError, match="Missing column"):
        compare.score(pilots, truth, ["altitude_nonsense"], k=2, seed=0)


def test_score_against_recovers_a_labelling_it_was_given():
    """
    A sanity check on the scorer itself: labels that the features separate
    cleanly must score near 1, or every other number here is meaningless.
    """
    import pandas as pd

    rows = []
    for i in range(12):
        far = i < 6
        row = {"pilot_id": f"P{i:02d}"}
        for metric in METRICS:
            row[f"{metric}_std"] = 100.0 if far else 1.0
            row[f"{metric}_abs_bias"] = 100.0 if far else 1.0
            row[f"{metric}_mad"] = 100.0 if far else 1.0
        rows.append(row)
    pilots = pd.DataFrame(rows)
    labels = np.array([1] * 6 + [0] * 6)

    for columns in compare.FEATURE_SETS.values():
        assert compare.score_against(pilots, labels, columns, k=2, seed=0) == 1.0


# --- the two comparisons ---------------------------------------------------


def test_compare_scores_every_seed_and_feature_set():
    per_seed = compare.compare(seeds=(0, 1), n_pilots=9, flight_seconds=10)

    assert list(per_seed["seed"]) == [0, 1]
    assert set(compare.FEATURE_SETS) <= set(per_seed.columns)
    for name in compare.FEATURE_SETS:
        assert all(-1.0 <= value <= 1.0 for value in per_seed[name])


def test_summarize_reports_mean_and_range():
    per_seed = compare.compare(seeds=(0, 1), n_pilots=9, flight_seconds=10)

    summary = compare.summarize(per_seed)

    assert list(summary["features"]) == list(compare.FEATURE_SETS)
    for _, row in summary.iterrows():
        assert row["min"] <= row["mean"] <= row["max"]


def test_style_separation_works_within_a_tier_not_across_tiers():
    """
    The whole point: the two pilots ADR 0002 is about share a tier, so the
    comparison has to be made inside one.
    """
    styles = compare.style_separation(
        seeds=(0, 1), n_pilots=18, flight_seconds=10, independent_share=1.0
    )

    assert not styles.empty
    assert set(styles["tier"]) <= {1, 2, 3}
    # Each row is one tier of one seed, never a mix.
    assert len(styles) == len(styles.drop_duplicates(subset=["seed", "tier"]))
    for name in compare.FEATURE_SETS:
        assert all(-1.0 <= value <= 1.0 for value in styles[name])


def test_style_separation_skips_tiers_too_small_to_split():
    """Three pilots in a tier cannot be split into two groups meaningfully."""
    styles = compare.style_separation(
        seeds=(0,), n_pilots=6, flight_seconds=10, independent_share=1.0
    )

    assert styles.empty or (styles["pilots"] >= 4).all()


def test_the_command_runs_and_prints_both_comparisons(capsys):
    compare.main(["--seeds", "1", "--pilots", "18", "--flight-seconds", "10"])

    printed = capsys.readouterr().out
    assert "std + per-flight abs_bias" in printed
    assert "mad only" in printed
    assert "Steady versus oscillating" in printed
    assert "tier-cohorts" in printed


def test_a_cohort_too_small_to_split_says_so_rather_than_failing(capsys):
    """9 pilots is 3 per tier. The run should explain that, not raise."""
    compare.main(["--seeds", "1", "--pilots", "9", "--flight-seconds", "10"])

    printed = capsys.readouterr().out
    assert "Not enough pilots per tier" in printed


def test_a_bad_share_is_a_usage_error(capsys):
    with pytest.raises(SystemExit) as exit_info:
        compare.main(["--seeds", "1", "--pilots", "9", "--independent-share", "2"])

    assert exit_info.value.code == 2
    assert "independent_share" in capsys.readouterr().err


# --- the generator's own command ------------------------------------------


def test_the_generator_command_runs_and_writes_both_files(tmp_path, monkeypatch):
    """
    Nothing exercised generate_synthetic_pilots' main() before, so a flag that
    main() read but never declared passed every test and failed on first use.
    """
    from pilot_inference.performance import generate_synthetic_pilots as generator

    telemetry = tmp_path / "telemetry.csv"
    truth = tmp_path / "truth.csv"
    monkeypatch.setattr(
        "sys.argv",
        [
            "generate_synthetic_pilots",
            "--pilots", "6",
            "--flight-seconds", "2",
            "--independent-share", "0.5",
            "--output", str(telemetry),
            "--truth-output", str(truth),
        ],
    )

    generator.main()

    assert telemetry.stat().st_size > 0
    assert truth.stat().st_size > 0


def test_the_generator_command_refuses_a_bad_share(tmp_path, monkeypatch):
    from pilot_inference.performance import generate_synthetic_pilots as generator

    monkeypatch.setattr(
        "sys.argv",
        [
            "generate_synthetic_pilots",
            "--pilots", "6",
            "--flight-seconds", "2",
            "--independent-share", "3",
            "--output", str(tmp_path / "t.csv"),
            "--truth-output", str(tmp_path / "tt.csv"),
        ],
    )

    with pytest.raises(SystemExit) as exit_info:
        generator.main()

    assert exit_info.value.code == 2
