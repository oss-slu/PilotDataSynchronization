import pandas as pd

from features import compute_features
from generate_synthetic_pilots import TIERS, generate_pilots


def test_telemetry_feeds_compute_features():
    telemetry, _ = generate_pilots(n_pilots=6, seed=1, flight_seconds=5)
    flights, pilots = compute_features(telemetry)

    assert len(pilots) == 6
    assert flights.groupby("pilot_id").size().between(2, 4).all()
    # 5 seconds at 20 Hz.
    assert (flights["altitude_rows_used"] == 100).all()


def test_values_are_in_feet_and_knots():
    telemetry, _ = generate_pilots(n_pilots=6, seed=1, flight_seconds=5)
    # Pre-#196 data was in metres and m/s; feet and knots put a light aircraft here.
    assert telemetry["target_altitude"].between(1000, 15000).all()
    assert telemetry["target_airspeed"].between(60, 200).all()


def test_truth_is_separate_with_three_equal_tiers():
    telemetry, truth = generate_pilots(n_pilots=15, seed=1, flight_seconds=5)

    assert "tier" not in telemetry.columns
    assert sorted(truth.columns) == ["pilot_id", "tier"]
    assert truth["pilot_id"].is_unique
    assert set(truth["pilot_id"]) == set(telemetry["pilot_id"])
    assert truth["tier"].value_counts().to_dict() == {1: 5, 2: 5, 3: 5}


def test_same_seed_gives_same_pilots():
    first = generate_pilots(n_pilots=6, seed=3, flight_seconds=5)
    second = generate_pilots(n_pilots=6, seed=3, flight_seconds=5)
    pd.testing.assert_frame_equal(first[0], second[0])
    pd.testing.assert_frame_equal(first[1], second[1])


def test_adjacent_tiers_overlap_on_some_std_feature():
    # With every tier cleanly apart on every feature, most clustering options
    # score perfectly and cannot be compared (docs/adr/0002). This only rules
    # that out; how hard the overlap makes grouping is measured in
    # test_clustering.py.
    for seed in range(6):
        telemetry, truth = generate_pilots(n_pilots=15, seed=seed)
        _, pilots = compute_features(telemetry)
        std_columns = [c for c in pilots.columns if c.endswith("_std")]
        spread = pilots.merge(truth, on="pilot_id").groupby("tier")[std_columns].agg(["min", "max"])

        overlapping = [
            column
            for column in std_columns
            for better, worse in zip(TIERS, TIERS[1:])
            if spread.loc[better, (column, "max")] > spread.loc[worse, (column, "min")]
        ]
        assert overlapping, f"seed {seed}: no adjacent tiers overlap on any std feature"
