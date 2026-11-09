import pandas as pd
import pytest

from pilot_inference.performance.cluster_pilots import main
from pilot_inference.performance.features import compute_features
from pilot_inference.performance.generate_synthetic_pilots import generate_pilots


@pytest.fixture
def cohort(tmp_path):
    """A synthetic pilot_features.csv and its true tiers, in an otherwise empty directory."""
    telemetry, truth = generate_pilots(n_pilots=9, seed=0, flight_seconds=20)
    _, pilots = compute_features(telemetry)
    pilots.to_csv(tmp_path / "pilot_features.csv", index=False)
    truth.to_csv(tmp_path / "tiers.csv", index=False)
    return tmp_path


def outputs(directory):
    return {"assignments": directory / "groups.csv", "centres": directory / "centres.csv"}


def run(directory, *extra):
    paths = outputs(directory)
    main(
        [
            "--input", str(directory / "pilot_features.csv"),
            "--assignments-output", str(paths["assignments"]),
            "--centres-output", str(paths["centres"]),
            "--subsamples", "5",
            *extra,
        ]
    )
    return paths


def test_without_k_prints_the_sweep_and_writes_nothing(cohort, capsys):
    paths = run(cohort)

    report = capsys.readouterr().out
    assert "silhouette" in report and "stability" in report
    assert not paths["assignments"].exists()
    assert not paths["centres"].exists()


def test_with_k_writes_groups_and_centres_and_reports_truth(cohort, capsys):
    paths = run(cohort, "--k", "3", "--truth", str(cohort / "tiers.csv"))

    groups = pd.read_csv(paths["assignments"])
    assert list(groups.columns) == ["pilot_id", "group"]
    assert len(groups) == 9
    centres = pd.read_csv(paths["centres"])
    assert len(centres) == 3
    assert centres["pilots"].sum() == 9
    assert "Agreement with true tiers" in capsys.readouterr().out


# #232: bad arguments ended in a raw traceback, and a --truth that did not match
# the cohort was only checked after both CSVs had been written, so a failed run
# left half its outputs behind.


def test_a_bad_k_is_a_usage_error_and_writes_nothing(cohort, capsys):
    """9 pilots allow k of 2 or 3, so --k 1 is out of range."""
    paths = outputs(cohort)

    with pytest.raises(SystemExit) as exit_info:
        run(cohort, "--k", "1")

    assert exit_info.value.code == 2
    assert "k must be" in capsys.readouterr().err
    assert not paths["assignments"].exists()
    assert not paths["centres"].exists()


def test_a_truth_file_with_a_repeated_pilot_writes_nothing(cohort, capsys):
    """The run fails, so it must not leave the group and centre CSVs behind."""
    truth = pd.read_csv(cohort / "tiers.csv")
    repeated = pd.concat([truth, truth.head(1)], ignore_index=True)
    repeated.to_csv(cohort / "repeated.csv", index=False)
    paths = outputs(cohort)

    with pytest.raises(SystemExit) as exit_info:
        run(cohort, "--k", "3", "--truth", str(cohort / "repeated.csv"))

    assert exit_info.value.code == 2
    assert "repeated in truth" in capsys.readouterr().err
    assert not paths["assignments"].exists()
    assert not paths["centres"].exists()


def test_a_truth_file_missing_a_pilot_writes_nothing(cohort, capsys):
    partial = pd.read_csv(cohort / "tiers.csv").head(5)
    partial.to_csv(cohort / "partial.csv", index=False)
    paths = outputs(cohort)

    with pytest.raises(SystemExit) as exit_info:
        run(cohort, "--k", "3", "--truth", str(cohort / "partial.csv"))

    assert exit_info.value.code == 2
    assert "missing from truth" in capsys.readouterr().err
    assert not paths["assignments"].exists()
    assert not paths["centres"].exists()


def test_a_truth_file_that_does_not_exist_is_a_usage_error(cohort, capsys):
    paths = outputs(cohort)

    with pytest.raises(SystemExit) as exit_info:
        run(cohort, "--k", "3", "--truth", str(cohort / "absent.csv"))

    assert exit_info.value.code == 2
    assert not paths["assignments"].exists()
    assert not paths["centres"].exists()
