"""
The feature table itself, and one run of the CLI over invented exports.

The point of these is that the deviation maths is features.py's, not a second
copy of it: the wrapped heading difference and the std/abs_bias definitions
have to be the same ones the synthetic pipeline and the live sidecar use.
"""

import argparse

import numpy as np
import pandas as pd
import pytest

from pilot_inference.prepar3d.build_features import (
    CORE_FEATURES,
    EXTENDED_FEATURES,
    airspeed_spread,
    build,
    plot_flight,
    run,
)
from pilot_inference.prepar3d.legs import Scenario, attach_targets

SCENARIO = Scenario(headings=(190.0, 40.0, 220.0, 0.0), level_altitude=3100.0)

HEADER = [
    "Timestamp",
    "Altitude MSL,ft",
    "Heading (true),deg",
    "Indicated Airspeed,kts",
    "Vertical speed,fpm",
    "Bank,deg",
    "Pitch,deg",
]


def held_flight(n=100, flight_id="P001-T1", heading=(189.0, 193.0), altitude=(3095.0, 3115.0)):
    """
    An airborne hold with a deterministic wobble, so the statistics are exact.

    Heading alternates to give deviations of -1 and +3 against 190: mean 1,
    population std 2. Altitude alternates to give -5 and +15 against 3,100:
    mean 5, std 10.
    """
    rows = {
        "pilot_id": ["P001"] * n,
        "flight_id": [flight_id] * n,
        "t_seconds": [float(i) for i in range(n)],
        "altitude": [altitude[i % 2] for i in range(n)],
        "heading": [heading[i % 2] for i in range(n)],
        "airspeed": [108.0 if i % 2 else 112.0 for i in range(n)],
        "vertical_speed": [-30.0 if i % 2 else 70.0 for i in range(n)],
        "bank": [0.0] * n,
        "pitch": [0.0] * n,
        "airborne": [True] * n,
    }
    return attach_targets(pd.DataFrame(rows), SCENARIO, settle_seconds=0.0)


def test_the_feature_table_holds_exactly_the_columns_the_issue_specifies():
    features = build(held_flight())
    assert list(features.columns) == ["pilot_id", "flight_id", *CORE_FEATURES, *EXTENDED_FEATURES]


def test_the_core_four_are_the_four_from_adr_0002():
    assert CORE_FEATURES == (
        "altitude_std",
        "altitude_abs_bias",
        "heading_std",
        "heading_abs_bias",
    )


def test_the_extended_set_adds_three():
    assert EXTENDED_FEATURES == ("vertical_speed_std", "vertical_speed_abs_bias", "airspeed_std")


def test_one_row_per_flight():
    samples = pd.concat([held_flight(flight_id="P001-T1"), held_flight(flight_id="P001-T2")])
    features = build(samples)
    assert features["flight_id"].tolist() == ["P001-T1", "P001-T2"]


def test_the_statistics_are_the_ones_features_py_computes():
    features = build(held_flight()).iloc[0]
    assert features["heading_std"] == pytest.approx(2.0)
    assert features["heading_abs_bias"] == pytest.approx(1.0)
    assert features["altitude_std"] == pytest.approx(10.0)
    assert features["altitude_abs_bias"] == pytest.approx(5.0)
    assert features["vertical_speed_std"] == pytest.approx(50.0)
    assert features["vertical_speed_abs_bias"] == pytest.approx(20.0)
    assert features["airspeed_std"] == pytest.approx(2.0)


def test_heading_deviation_wraps_so_a_hold_on_north_is_not_358_degrees_off():
    """A pilot holding 358 against a target of 0 is 2 degrees off, not 358."""
    features = build(held_flight(heading=(357.0, 1.0))).iloc[0]
    assert features["heading_abs_bias"] == pytest.approx(1.0)
    assert features["heading_std"] == pytest.approx(2.0)


def test_the_core_four_are_not_nan_for_a_flight_with_both_hold_types():
    features = build(held_flight())
    assert not features[list(CORE_FEATURES)].isna().any().any()


def test_airspeed_has_a_spread_but_no_bias_because_there_is_no_target():
    features = build(held_flight())
    assert "airspeed_std" in features.columns
    assert "airspeed_abs_bias" not in features.columns
    assert "airspeed_bias" not in features.columns


def test_airspeed_spread_uses_only_the_scored_samples():
    samples = held_flight(n=10)
    # Put a wild airspeed on an unscored sample; it must not reach the feature.
    samples.loc[0, "scored"] = False
    samples.loc[0, "airspeed"] = 400.0
    spread = airspeed_spread(samples)
    assert spread["airspeed_std"].iloc[0] == pytest.approx(2.0, abs=0.3)


def test_airspeed_spread_of_a_flight_with_no_scored_sample_is_empty():
    samples = held_flight(n=4)
    samples["scored"] = False
    assert airspeed_spread(samples).empty


def test_building_from_a_flight_with_no_targets_leaves_the_features_nan():
    samples = held_flight(n=10)
    for column in ("target_heading", "target_altitude", "target_vertical_speed"):
        samples[column] = np.nan
    samples["scored"] = False
    features = build(samples)
    assert features[list(CORE_FEATURES)].isna().all().all()


def write_export(path, samples):
    """An invented Prepar3D export, CRLF and latin-1 like the real ones."""
    lines = ["\t".join(HEADER)]
    for second, altitude, heading, airspeed, vertical, bank, pitch in samples:
        lines.append(
            "\t".join(
                [
                    f"12:{second // 60:02d}:{second % 60:02d}",
                    str(altitude),
                    str(heading),
                    str(airspeed),
                    str(vertical),
                    str(bank),
                    str(pitch),
                ]
            )
        )
    path.write_bytes(("\r\n".join(lines) + "\r\n").encode("latin-1"))


def a_whole_flight():
    """Ten seconds parked, a climb, two held Legs, then back on the ground."""
    samples = []
    second = 0
    for _ in range(10):
        samples.append((second, 1100, 190, 0, 0, 0, 0))
        second += 1
    for altitude in range(1200, 3100, 100):
        samples.append((second, altitude, 190, 90, 700, 1, 6))
        second += 1
    for index in range(60):
        samples.append((second, 3095 if index % 2 else 3105, 190, 110, 0, 0, 0))
        second += 1
    for index in range(60):
        samples.append((second, 3095 if index % 2 else 3105, 40, 110, 0, 0, 0))
        second += 1
    for _ in range(10):
        samples.append((second, 1100, 40, 0, 0, 0, 0))
        second += 1
    return samples


def cli_args(tmp_path, raw_dir, **overrides):
    from pilot_inference.prepar3d.build_features import SCENARIO as BRIEF
    from pilot_inference.prepar3d.clean import (
        DEFAULT_AIRSPEED_MIN_KTS,
        DEFAULT_ALTITUDE_MARGIN_FT,
        DEFAULT_MAX_GAP_SECONDS,
        DEFAULT_SPIKE_FPM,
    )
    from pilot_inference.prepar3d.legs import (
        DEFAULT_ALTITUDE_TOLERANCE_FT,
        DEFAULT_BANK_LIMIT_DEG,
        DEFAULT_HEADING_TOLERANCE_DEG,
        DEFAULT_VERTICAL_SPEED_LIMIT_FPM,
    )

    args = argparse.Namespace(
        raw_dir=raw_dir,
        out=tmp_path / "features" / "flight_features.csv",
        clean_dir=tmp_path / "clean",
        report=tmp_path / "results" / "report.csv",
        plots_dir=None,
        scenario=BRIEF,
        heading_tolerance=DEFAULT_HEADING_TOLERANCE_DEG,
        bank_limit=DEFAULT_BANK_LIMIT_DEG,
        altitude_tolerance=DEFAULT_ALTITUDE_TOLERANCE_FT,
        vertical_speed_limit=DEFAULT_VERTICAL_SPEED_LIMIT_FPM,
        settle_seconds=5.0,
        gap_seconds=DEFAULT_MAX_GAP_SECONDS,
        spike_fpm=DEFAULT_SPIKE_FPM,
        airspeed_min=DEFAULT_AIRSPEED_MIN_KTS,
        altitude_margin=DEFAULT_ALTITUDE_MARGIN_FT,
    )
    for key, value in overrides.items():
        setattr(args, key, value)
    return args


def test_a_run_over_two_exports_writes_every_output(tmp_path):
    raw = tmp_path / "raw"
    raw.mkdir()
    write_export(raw / "P001_T1.tsv", a_whole_flight())
    write_export(raw / "P002_T1.tsv", a_whole_flight())

    args = cli_args(tmp_path, raw)
    assert run(args) == 0

    features = pd.read_csv(args.out)
    assert len(features) == 2
    assert features["pilot_id"].tolist() == ["P001", "P002"]
    assert not features[list(CORE_FEATURES)].isna().any().any()

    report = pd.read_csv(args.report)
    assert len(report) == 2
    assert report["airborne_minutes"].gt(0).all()
    assert (args.clean_dir / "P001-T1.csv").exists()


def test_a_run_writes_one_plot_per_flight(tmp_path):
    raw = tmp_path / "raw"
    raw.mkdir()
    write_export(raw / "P001_T1.tsv", a_whole_flight())
    write_export(raw / "P002_T1.tsv", a_whole_flight())

    plots = tmp_path / "plots"
    args = cli_args(tmp_path, raw, plots_dir=plots)
    assert run(args) == 0

    written = sorted(path.name for path in plots.glob("*.png"))
    assert written == ["P001-T1.png", "P002-T1.png"]
    assert all((plots / name).stat().st_size > 5000 for name in written)


def test_plotting_a_flight_returns_true_and_writes_a_figure(tmp_path):
    path = tmp_path / "deep" / "P001-T1.png"
    assert plot_flight(held_flight(), path, SCENARIO) is True
    assert path.exists()


def test_a_run_reports_a_flight_that_produced_no_leg(tmp_path):
    raw = tmp_path / "raw"
    raw.mkdir()
    # Parked the whole time: trimming leaves nothing, so there are no features.
    write_export(raw / "P003_T1.tsv", [(i, 1100, 190, 0, 0, 0, 0) for i in range(20)])
    write_export(raw / "P001_T1.tsv", a_whole_flight())

    args = cli_args(tmp_path, raw)
    assert run(args) == 0
    features = pd.read_csv(args.out)
    assert features["pilot_id"].tolist() == ["P001"]

    report = pd.read_csv(args.report)
    assert "never airborne" in report.loc[report["flight_id"] == "P003-T1", "note"].iloc[0]


def test_a_run_whose_tolerances_detect_nothing_exits_nonzero(tmp_path):
    raw = tmp_path / "raw"
    raw.mkdir()
    write_export(raw / "P001_T1.tsv", a_whole_flight())
    # No heading is ever within half a degree of a scenario heading plus wobble.
    args = cli_args(tmp_path, raw, heading_tolerance=0.0, altitude_tolerance=0.0)
    assert run(args) == 1
