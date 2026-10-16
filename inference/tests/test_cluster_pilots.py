import pandas as pd
import pytest

from cluster_pilots import main
from features import compute_features
from generate_synthetic_pilots import generate_pilots


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
