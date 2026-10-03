import numpy as np
import pandas as pd
import pytest

from features import (
    METRICS,
    STATISTICS,
    compute_deviations,
    compute_features,
    flight_features,
    pilot_features,
)


def make_flight(pilot_id="p1", flight_id="f1", n=4, **overrides):
    """A flight sitting exactly on its targets, with selected columns overridden."""
    columns = {
        "pilot_id": pilot_id,
        "flight_id": flight_id,
        "altitude": 5000.0,
        "heading": 90.0,
        "vertical_speed": 0.0,
        "velocity": 120.0,
        "target_altitude": 5000.0,
        "target_heading": 90.0,
        "target_vertical_speed": 0.0,
        "target_airspeed": 120.0,
    }
    columns.update(overrides)
    return pd.DataFrame({k: [v] * n if np.isscalar(v) else v for k, v in columns.items()})


def test_metrics_use_glossary_names():
    assert METRICS == ("altitude", "heading", "vertical_speed", "airspeed")


def test_deviation_is_actual_minus_target():
    df = make_flight(
        altitude=[4900.0, 5100.0],
        vertical_speed=[-50.0, 200.0],
        velocity=[110.0, 125.0],
        n=2,
    )
    out = compute_deviations(df)
    assert out["altitude_deviation"].tolist() == [-100.0, 100.0]
    assert out["vertical_speed_deviation"].tolist() == [-50.0, 200.0]
    assert out["airspeed_deviation"].tolist() == [-10.0, 5.0]


def test_heading_deviation_wraps_at_360():
    df = make_flight(
        heading=[5.0, 355.0, 270.0],
        target_heading=[355.0, 5.0, 90.0],
        n=3,
    )
    out = compute_deviations(df)
    np.testing.assert_allclose(out["heading_deviation"], [10.0, -10.0, -180.0])


def test_compute_deviations_does_not_mutate_input():
    df = make_flight()
    before = df.copy()
    compute_deviations(df)
    pd.testing.assert_frame_equal(df, before)


def test_missing_column_is_named_in_error():
    df = make_flight().drop(columns=["target_altitude", "pilot_id"])
    with pytest.raises(ValueError, match="pilot_id.*target_altitude|target_altitude.*pilot_id"):
        compute_deviations(df)


def test_flight_features_mad_std_and_bias():
    df = make_flight(altitude=[4900.0, 5100.0, 5200.0, 5000.0])
    flights = flight_features(compute_deviations(df)).set_index(["pilot_id", "flight_id"])
    row = flights.loc[("p1", "f1")]
    deviations = np.array([-100.0, 100.0, 200.0, 0.0])
    assert row["altitude_mad"] == pytest.approx(np.abs(deviations).mean())
    assert row["altitude_std"] == pytest.approx(deviations.std(ddof=0))
    assert row["altitude_bias"] == pytest.approx(deviations.mean())
    assert row["altitude_rows_used"] == 4
    assert row["heading_mad"] == 0.0


def test_flight_features_skips_nan_targets_and_counts_rows_used():
    df = make_flight(
        altitude=[5100.0, 5300.0, 9999.0, 9999.0],
        target_altitude=[5000.0, 5000.0, np.nan, np.nan],
    )
    flights = flight_features(compute_deviations(df))
    row = flights.iloc[0]
    assert row["altitude_rows_used"] == 2
    assert row["altitude_mad"] == pytest.approx(200.0)
    assert row["heading_rows_used"] == 4


def test_flight_with_no_valid_rows_for_a_metric_is_skipped_with_a_warning():
    df = pd.concat(
        [
            make_flight(flight_id="good"),
            make_flight(flight_id="no_targets", target_altitude=np.nan),
        ],
        ignore_index=True,
    )
    with pytest.warns(UserWarning, match="no_targets.*altitude"):
        flights = flight_features(compute_deviations(df)).set_index("flight_id")
    bad = flights.loc["no_targets"]
    assert bad["altitude_rows_used"] == 0
    assert np.isnan(bad["altitude_mad"])
    assert np.isnan(bad["altitude_std"])
    assert np.isnan(bad["altitude_bias"])
    assert bad["heading_rows_used"] == 4
    assert bad["heading_mad"] == 0.0
    assert flights.loc["good", "altitude_rows_used"] == 4


def test_pilot_features_average_only_the_flights_that_have_the_metric():
    df = pd.concat(
        [
            make_flight(flight_id="ok1", altitude=5100.0),
            make_flight(flight_id="ok2", altitude=5300.0),
            make_flight(flight_id="no_targets", target_altitude=np.nan),
        ],
        ignore_index=True,
    )
    with pytest.warns(UserWarning, match="no_targets"):
        flights = flight_features(compute_deviations(df))
    pilots = pilot_features(flights).set_index("pilot_id")
    assert pilots.loc["p1", "altitude_mad"] == pytest.approx((100.0 + 300.0) / 2)
    assert pilots.loc["p1", "flights"] == 3


def test_pilot_with_no_valid_flight_for_a_metric_is_nan_with_a_warning():
    df = pd.concat(
        [
            make_flight(pilot_id="a", flight_id="fa"),
            make_flight(pilot_id="b", flight_id="fb", target_altitude=np.nan),
        ],
        ignore_index=True,
    )
    with pytest.warns(UserWarning):
        flights = flight_features(compute_deviations(df))
    with pytest.warns(UserWarning, match="Pilot 'b'.*altitude"):
        pilots = pilot_features(flights).set_index("pilot_id")
    assert np.isnan(pilots.loc["b", "altitude_mad"])
    assert pilots.loc["a", "altitude_mad"] == 0.0


def test_pilot_features_weight_every_flight_equally():
    long_flight = make_flight(flight_id="long", n=100, altitude=5100.0)
    short_flight = make_flight(flight_id="short", n=2, altitude=5300.0)
    worst_flight = make_flight(flight_id="worst", n=5, altitude=5900.0)
    df = pd.concat([long_flight, short_flight, worst_flight], ignore_index=True)
    pilots = pilot_features(flight_features(compute_deviations(df))).set_index("pilot_id")
    # A mean of flight means (not pooled samples, not a median).
    assert pilots.loc["p1", "altitude_mad"] == pytest.approx((100.0 + 300.0 + 900.0) / 3)
    assert pilots.loc["p1", "flights"] == 3


def test_pilot_features_keep_pilots_separate_and_drop_rows_used():
    df = pd.concat(
        [
            make_flight(pilot_id="a", altitude=5100.0),
            make_flight(pilot_id="b", altitude=5500.0),
        ],
        ignore_index=True,
    )
    pilots = pilot_features(flight_features(compute_deviations(df))).set_index("pilot_id")
    assert pilots.loc["a", "altitude_mad"] == pytest.approx(100.0)
    assert pilots.loc["b", "altitude_mad"] == pytest.approx(500.0)
    assert not any(c.endswith("_rows_used") for c in pilots.columns)


def test_compute_features_returns_flight_and_pilot_tables():
    df = pd.concat(
        [make_flight(pilot_id="a", flight_id="f1"), make_flight(pilot_id="a", flight_id="f2")],
        ignore_index=True,
    )
    flights, pilots = compute_features(df)
    assert len(flights) == 2
    assert len(pilots) == 1
    for metric in METRICS:
        for suffix in STATISTICS:
            assert f"{metric}_{suffix}" in pilots.columns


def test_empty_input_raises():
    with pytest.raises(ValueError, match="empty"):
        compute_features(make_flight().iloc[0:0])


@pytest.mark.parametrize("id_column", ["pilot_id", "flight_id"])
def test_null_id_raises_instead_of_dropping_rows(id_column):
    df = make_flight(n=3)
    df[id_column] = df[id_column].astype(object)
    df.loc[1, id_column] = None
    with pytest.raises(ValueError, match=id_column):
        compute_features(df)


def test_infinite_values_are_skipped_like_nan_and_not_counted():
    df = make_flight(altitude=[5100.0, np.inf, 4900.0, 5000.0])
    row = flight_features(compute_deviations(df)).iloc[0]
    assert row["altitude_rows_used"] == 3
    assert np.isfinite(row["altitude_mad"])
    assert row["altitude_mad"] == pytest.approx((100.0 + 100.0 + 0.0) / 3)


def test_one_row_flight_has_zero_std():
    df = make_flight(n=1, altitude=5100.0)
    row = flight_features(compute_deviations(df)).iloc[0]
    assert row["altitude_mad"] == pytest.approx(100.0)
    assert row["altitude_std"] == 0.0
    assert row["altitude_rows_used"] == 1
