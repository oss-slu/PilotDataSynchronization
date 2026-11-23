"""
Normalising the client's pilot demographics. Every value here is invented.

The two conventions that have to be read the same way everywhere are the
phrase "All Six Ratings" and the value "No data". Both appear in the client's
table, and a pilot's ratings are what the groups get interpreted against in
the third issue of this sequence.
"""

import numpy as np
import pytest

from pilot_inference.prepar3d.metadata import (
    COLUMNS,
    RATINGS,
    build_metadata,
    is_no_data,
    normalise_file,
    parse_background,
    parse_flight_hours,
    parse_ratings,
)


def test_all_six_ratings_sets_every_rating():
    assert parse_ratings("All Six Ratings") == {rating: 1 for rating in RATINGS}


def test_all_six_ratings_is_not_case_or_spacing_sensitive():
    assert parse_ratings("all  six   ratings") == {rating: 1 for rating in RATINGS}


def test_no_data_sets_no_rating():
    assert parse_ratings("No data") == {rating: 0 for rating in RATINGS}


@pytest.mark.parametrize("text", ["", "   ", "N/A", "none", "-"])
def test_the_other_ways_of_writing_no_data(text):
    assert is_no_data(text)
    assert parse_ratings(text) == {rating: 0 for rating in RATINGS}


def test_a_list_of_ratings_sets_only_those():
    flags = parse_ratings("Instrument, Commercial, Multi-Engine")
    assert flags == {
        "instrument": 1,
        "commercial": 1,
        "multi_engine": 1,
        "cfi": 0,
        "cfii": 0,
        "atp": 0,
    }


def test_ratings_split_on_slashes_and_the_word_and():
    assert parse_ratings("CFI / CFII and ATP") == {
        "instrument": 0,
        "commercial": 0,
        "multi_engine": 0,
        "cfi": 1,
        "cfii": 1,
        "atp": 1,
    }


def test_cfi_and_cfii_are_not_confused():
    assert parse_ratings("CFII")["cfi"] == 0
    assert parse_ratings("CFII")["cfii"] == 1
    assert parse_ratings("CFI")["cfi"] == 1
    assert parse_ratings("CFI")["cfii"] == 0


def test_an_unrecognised_rating_raises_rather_than_being_dropped():
    """Dropping it silently would make a pilot look less qualified than they are."""
    with pytest.raises(ValueError, match="Unrecognised rating"):
        parse_ratings("Instrument, Seaplane")


@pytest.mark.parametrize(
    "text",
    ["Collegiate", "Part 91 or 135 Commercial", "Air Transport", "Part 61 or 141 Flight Instructor"],
)
def test_the_four_client_backgrounds_round_trip(text):
    assert parse_background(text) == text


def test_a_background_of_no_data_is_blank_not_a_fifth_category():
    assert parse_background("No data") == ""
    assert parse_background("") == ""


def test_an_unrecognised_background_raises():
    with pytest.raises(ValueError, match="Unrecognised background"):
        parse_background("Military")


@pytest.mark.parametrize(
    ("text", "expected"), [("1200", 1200.0), ("1,200", 1200.0), ("450 hrs", 450.0), ("~450", 450.0)]
)
def test_flight_hours_tolerate_how_a_spreadsheet_writes_them(text, expected):
    assert parse_flight_hours(text) == expected


def test_flight_hours_of_no_data_is_nan():
    assert np.isnan(parse_flight_hours("No data"))


def test_flight_hours_with_no_digits_raises():
    with pytest.raises(ValueError, match="Cannot read flight hours"):
        parse_flight_hours("lots")


def test_build_metadata_normalises_every_row():
    table = build_metadata(
        [
            {
                "pilot_id": "p001",
                "flight_hours": "1,200",
                "background": "Air Transport",
                "ratings": "All Six Ratings",
            },
            {
                "pilot_id": "P002",
                "flight_hours": "No data",
                "background": "No data",
                "ratings": "No data",
            },
        ]
    )
    assert list(table.columns) == list(COLUMNS)
    assert table["pilot_id"].tolist() == ["P001", "P002"]
    assert table.loc[0, "atp"] == 1
    assert table.loc[1, "atp"] == 0
    assert table.loc[1, "background"] == ""
    assert np.isnan(table.loc[1, "flight_hours"])


def test_build_metadata_sorts_by_pilot():
    table = build_metadata(
        [
            {"pilot_id": "P007", "flight_hours": "10", "background": "", "ratings": ""},
            {"pilot_id": "P001", "flight_hours": "20", "background": "", "ratings": ""},
        ]
    )
    assert table["pilot_id"].tolist() == ["P001", "P007"]


def test_a_pilot_listed_twice_raises():
    with pytest.raises(ValueError, match="more than once"):
        build_metadata(
            [
                {"pilot_id": "P001", "flight_hours": "10", "background": "", "ratings": ""},
                {"pilot_id": "P001", "flight_hours": "20", "background": "", "ratings": ""},
            ]
        )


def test_normalise_file_reads_a_transcription_and_writes_the_table(tmp_path):
    source = tmp_path / "transcribed.csv"
    source.write_text(
        "pilot_id,flight_hours,background,ratings\n"
        "P001,1200,Air Transport,All Six Ratings\n"
        "P002,No data,No data,No data\n"
        "P003,350,Collegiate,Instrument\n"
    )
    destination = tmp_path / "out" / "pilot_metadata.csv"
    table = normalise_file(source, destination)
    assert destination.exists()
    assert len(table) == 3
    assert table.loc[table["pilot_id"] == "P003", "instrument"].iloc[0] == 1
    assert table.loc[table["pilot_id"] == "P003", "atp"].iloc[0] == 0


def test_normalise_file_names_a_missing_column(tmp_path):
    source = tmp_path / "transcribed.csv"
    source.write_text("pilot_id,flight_hours\nP001,1200\n")
    with pytest.raises(ValueError, match="background"):
        normalise_file(source, tmp_path / "out.csv")


def test_the_six_ratings_are_the_ones_the_issue_lists():
    assert RATINGS == ("instrument", "commercial", "multi_engine", "cfi", "cfii", "atp")
