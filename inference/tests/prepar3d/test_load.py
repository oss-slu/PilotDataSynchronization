"""
Every fixture here is made-up telemetry. No client data is committed.

The eight fixtures hold the same six samples in the eight header layouts the
client's 19 exports use, so the core assertion is that all eight parse to
identical numbers. Positional parsing passes on one layout and silently
returns flap position or nose gear travel on the others.
"""

from pathlib import Path

import numpy as np
import pandas as pd
import pytest

from pilot_inference.prepar3d.load import (
    COLUMN_ALIASES,
    elapsed_seconds,
    load_flight,
    load_folder,
    parse_flight_ids,
    sample_rate_hz,
    split_header,
)

FIXTURES = Path(__file__).parent / "fixtures"
LAYOUTS = sorted(path.name for path in FIXTURES.glob("P001_L*.tsv"))

# What every layout must produce, whatever order its columns are in.
EXPECTED = {
    "altitude": [1100.0, 1500.0, 3100.0, 3100.0, 3100.0, 1200.0],
    "heading": [190.0, 190.0, 190.0, 40.0, 220.0, 0.0],
    "airspeed": [0.0, 90.0, 110.0, 110.0, 112.0, 70.0],
    "vertical_speed": [0.0, 700.0, 0.0, 0.0, -50.0, -600.0],
    "bank": [0.0, 2.0, 1.0, 3.0, 2.0, 1.0],
    "pitch": [0.0, 5.0, 0.0, 0.0, -1.0, -3.0],
}


def write_tsv(path: Path, lines) -> Path:
    """A TSV fixture built in a test, CRLF and latin-1 like the real exports."""
    path.write_bytes(("\r\n".join("\t".join(row) for row in lines) + "\r\n").encode("latin-1"))
    return path


def test_there_is_one_fixture_per_header_layout():
    assert len(LAYOUTS) == 8


@pytest.mark.parametrize("name", LAYOUTS)
def test_every_layout_parses_to_the_same_values(name):
    df = load_flight(FIXTURES / name)
    for column, values in EXPECTED.items():
        assert df[column].tolist() == values, f"{column} wrong in {name}"


@pytest.mark.parametrize("name", LAYOUTS)
def test_every_layout_keeps_all_six_samples(name):
    assert len(load_flight(FIXTURES / name)) == 6


def test_vertical_speed_is_not_in_the_same_column_in_every_layout():
    """The reason parsing is by name. If this ever fails the fixtures got dull."""

    def vertical_speed_index(name):
        header = (FIXTURES / name).read_text(encoding="latin-1").splitlines()[0]
        names = [split_header(cell)[0] for cell in header.split("\t")]
        return names.index("Vertical speed")

    assert len({vertical_speed_index(name) for name in LAYOUTS}) > 1


def test_a_blank_header_cell_does_not_shift_the_columns():
    df = load_flight(FIXTURES / "P001_L5_blank_header.tsv")
    assert df["altitude"].tolist() == EXPECTED["altitude"]


def test_unnamed_trailing_values_are_ignored():
    two = load_flight(FIXTURES / "P001_L6_two_trailing.tsv")
    four = load_flight(FIXTURES / "P001_L7_four_trailing.tsv")
    assert two["pitch"].tolist() == four["pitch"].tolist() == EXPECTED["pitch"]


def test_ids_come_from_the_filename():
    assert parse_flight_ids(Path("P004_T1.tsv")) == ("P004", "P004-T1")
    assert parse_flight_ids(Path("/a/b/P007_T2.tsv")) == ("P007", "P007-T2")


def test_a_filename_without_a_pilot_is_rejected():
    with pytest.raises(ValueError, match="pilot_id"):
        parse_flight_ids(Path("flight3.tsv"))


def test_ids_are_on_every_row():
    df = load_flight(FIXTURES / "P001_L1_plain.tsv")
    assert df["pilot_id"].unique().tolist() == ["P001"]
    assert df["flight_id"].unique().tolist() == ["P001-L1_plain"]
    assert df["source_file"].iloc[0] == "P001_L1_plain.tsv"


def test_split_header_separates_the_unit():
    assert split_header("Altitude MSL,ft") == ("Altitude MSL", "ft")
    assert split_header("Heading (true),deg") == ("Heading (true)", "deg")
    assert split_header("Timestamp") == ("Timestamp", "")


def test_a_missing_column_names_the_file_and_what_it_has(tmp_path):
    path = write_tsv(
        tmp_path / "P002_T9.tsv",
        [["Timestamp", "Altitude MSL,ft"], ["12:00:00", "1100"]],
    )
    with pytest.raises(ValueError) as error:
        load_flight(path)
    assert "P002_T9.tsv" in str(error.value)
    assert "heading" in str(error.value)
    assert "Altitude MSL" in str(error.value)


def test_an_altitude_in_metres_is_an_error_not_a_silent_conversion(tmp_path):
    header = [
        "Timestamp",
        "Altitude MSL,m",
        "Heading (true),deg",
        "Indicated Airspeed,kts",
        "Vertical speed,fpm",
        "Bank,deg",
        "Pitch,deg",
    ]
    path = write_tsv(
        tmp_path / "P002_T9.tsv", [header, ["12:00:00", "335", "190", "90", "0", "0", "0"]]
    )
    with pytest.raises(ValueError, match="needs ft"):
        load_flight(path)


def test_a_header_with_no_unit_warns_and_is_trusted(tmp_path):
    header = [
        "Timestamp",
        "Altitude MSL",
        "Heading (true),deg",
        "Indicated Airspeed,kts",
        "Vertical speed,fpm",
        "Bank,deg",
        "Pitch,deg",
    ]
    path = write_tsv(
        tmp_path / "P002_T9.tsv", [header, ["12:00:00", "1100", "190", "90", "0", "0", "0"]]
    )
    with pytest.warns(UserWarning, match="declares no unit"):
        df = load_flight(path)
    assert df["altitude"].tolist() == [1100.0]


def test_a_short_row_is_dropped_and_counted(tmp_path):
    header = [
        "Timestamp",
        "Altitude MSL,ft",
        "Heading (true),deg",
        "Indicated Airspeed,kts",
        "Vertical speed,fpm",
        "Bank,deg",
        "Pitch,deg",
    ]
    path = write_tsv(
        tmp_path / "P002_T9.tsv",
        [
            header,
            ["12:00:00", "1100", "190", "90", "0", "0", "0"],
            ["12:00:01", "1200"],
            ["12:00:02", "1300", "190", "95", "100", "1", "2"],
        ],
    )
    with pytest.warns(UserWarning, match="dropped 1 row"):
        df = load_flight(path)
    assert df["altitude"].tolist() == [1100.0, 1300.0]


def test_an_unparseable_value_becomes_nan_rather_than_losing_the_row(tmp_path):
    header = [
        "Timestamp",
        "Altitude MSL,ft",
        "Heading (true),deg",
        "Indicated Airspeed,kts",
        "Vertical speed,fpm",
        "Bank,deg",
        "Pitch,deg",
    ]
    path = write_tsv(
        tmp_path / "P002_T9.tsv",
        [header, ["12:00:00", "----", "190", "90", "0", "0", "0"]],
    )
    df = load_flight(path)
    assert len(df) == 1
    assert np.isnan(df["altitude"].iloc[0])
    assert df["heading"].iloc[0] == 190.0


def test_elapsed_seconds_from_a_time_of_day():
    seconds = elapsed_seconds(["12:00:00", "12:00:01", "12:00:03"])
    assert seconds.tolist() == [0.0, 1.0, 3.0]


def test_elapsed_seconds_from_numbers_is_relative_to_the_first():
    assert elapsed_seconds([100.0, 100.9, 102.0]).tolist() == [0.0, pytest.approx(0.9), 2.0]


def test_elapsed_seconds_repairs_a_crossing_of_midnight():
    seconds = elapsed_seconds(["23:59:58", "23:59:59", "00:00:01"])
    assert seconds.tolist() == [0.0, 1.0, 3.0]


def test_elapsed_seconds_of_unparseable_timestamps_is_nan():
    assert elapsed_seconds(["x", "y"]).isna().all()


def test_sample_rate_uses_the_median_so_a_gap_does_not_skew_it():
    df = pd.DataFrame({"t_seconds": [0.0, 1.0, 2.0, 3.0, 60.0, 61.0]})
    assert sample_rate_hz(df) == pytest.approx(1.0)


def test_sample_rate_of_a_frozen_clock_is_nan():
    assert np.isnan(sample_rate_hz(pd.DataFrame({"t_seconds": [5.0, 5.0, 5.0]})))


def test_load_folder_returns_every_fixture_sorted_by_flight():
    flights = load_folder(FIXTURES)
    assert len(flights) == 8
    ids = [df["flight_id"].iloc[0] for df in flights]
    assert ids == sorted(ids)


def test_load_folder_skips_an_unreadable_file_and_keeps_the_rest(tmp_path):
    good = [
        "Timestamp",
        "Altitude MSL,ft",
        "Heading (true),deg",
        "Indicated Airspeed,kts",
        "Vertical speed,fpm",
        "Bank,deg",
        "Pitch,deg",
    ]
    write_tsv(tmp_path / "P001_T1.tsv", [good, ["12:00:00", "1100", "190", "90", "0", "0", "0"]])
    write_tsv(tmp_path / "P002_T1.tsv", [["Timestamp"], ["12:00:00"]])
    with pytest.warns(UserWarning, match="Skipping P002_T1.tsv"):
        flights = load_folder(tmp_path)
    assert [df["flight_id"].iloc[0] for df in flights] == ["P001-T1"]


def test_a_missing_raw_folder_says_where_the_data_comes_from(tmp_path):
    with pytest.raises(FileNotFoundError, match="Drive"):
        load_folder(tmp_path / "not_here")


def test_the_columns_we_keep_are_the_ones_the_issue_lists():
    assert set(COLUMN_ALIASES) == {
        "timestamp",
        "altitude",
        "heading",
        "airspeed",
        "vertical_speed",
        "bank",
        "pitch",
    }
