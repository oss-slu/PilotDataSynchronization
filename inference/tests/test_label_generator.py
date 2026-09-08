import pandas as pd
import pytest

from inference.label_generator import FlightEventLabeler


@pytest.fixture
def labeler():
    return FlightEventLabeler()


def make_row(
    altitude=5000,
    velocity=120,
    vertical_speed=0,
    heading=90,
    roll=0,
    g_force=1.0,
):
    return pd.Series({
        "altitude": altitude,
        "velocity": velocity,
        "vertical_speed": vertical_speed,
        "heading": heading,
        "roll": roll,
        "g_force": g_force,
    })


# ---------------------------------------------------------------------------
# Flight event tests
# ---------------------------------------------------------------------------

def test_taxi(labeler):
    row = make_row(
        altitude=20,
        velocity=10,
    )

    assert labeler.label_row(row) == "TAXI"


def test_takeoff(labeler):
    row = make_row(
        altitude=500,
        velocity=70,
        vertical_speed=500,
    )

    assert labeler.label_row(row) == "TAKEOFF"


def test_cruise(labeler):
    row = make_row(
        altitude=10000,
        velocity=150,
        vertical_speed=100,
    )

    assert labeler.label_row(row) == "CRUISE"


def test_approach(labeler):
    row = make_row(
        altitude=2000,
        velocity=120,
        vertical_speed=-500,
    )

    assert labeler.label_row(row) == "APPROACH"


def test_landing(labeler):
    row = make_row(
        altitude=300,
        velocity=80,
        vertical_speed=-200,
    )

    assert labeler.label_row(row) == "LANDING"


def test_turn_left(labeler):
    row = make_row(
        altitude=5000,
        velocity=120,
        roll=-10,
        vertical_speed=400,
    )

    assert labeler.label_row(row) == "TURN_LEFT"


def test_turn_right(labeler):
    row = make_row(
        altitude=5000,
        velocity=120,
        roll=10,
        vertical_speed=400,
    )

    assert labeler.label_row(row) == "TURN_RIGHT"


def test_turn_right_from_heading_change(labeler):
    previous_row = make_row(
        heading=90,
        roll=0,
    )

    current_row = make_row(
        heading=100,
        altitude=2000,
        velocity=50,
        vertical_speed=400,
        roll=0,
    )

    assert labeler.label_row(
        current_row,
        previous_row,
    ) == "TURN_RIGHT"


def test_turn_left_from_heading_change(labeler):
    previous_row = make_row(
        heading=100,
        roll=0,
    )

    current_row = make_row(
        heading=90,
        altitude=2000,
        velocity=50,
        vertical_speed=400,
        roll=0,
    )

    assert labeler.label_row(
        current_row,
        previous_row,
    ) == "TURN_LEFT"


def test_high_speed(labeler):
    row = make_row(
        altitude=5000,
        velocity=250,
        vertical_speed=400,
    )

    assert labeler.label_row(row) == "HIGH_SPEED"


def test_low_speed(labeler):
    row = make_row(
        altitude=5000,
        velocity=40,
    )

    assert labeler.label_row(row) == "LOW_SPEED"


def test_high_altitude(labeler):
    row = make_row(
        altitude=15000,
        velocity=120,
        vertical_speed=400
    )

    assert labeler.label_row(row) == "HIGH_ALTITUDE"


def test_low_altitude(labeler):
    row = make_row(
        altitude=500,
        velocity=150,
    )

    assert labeler.label_row(row) == "LOW_ALTITUDE"


def test_high_g_force(labeler):
    row = make_row(
        altitude=5000,
        velocity=120,
        g_force=2.0,
        vertical_speed=400
    )

    assert labeler.label_row(row) == "HIGH_G_FORCE"


def test_normal_flight(labeler):
    row = make_row(
        altitude=2500,
        velocity=120,
        vertical_speed=400,
        roll=0,
        g_force=1.0,
    )

    assert labeler.label_row(row) == "NORMAL_FLIGHT"


# ---------------------------------------------------------------------------
# Heading wraparound tests
# ---------------------------------------------------------------------------

def test_heading_wraparound_359_to_1(labeler):
    previous_row = make_row(
        heading=359,
    )

    current_row = make_row(
        altitude=2000,
        velocity=120,
        vertical_speed=400,
        heading=1,
        roll=0,
    )

    assert labeler._calculate_heading_change(359, 1) == 2

    assert labeler.label_row(
        current_row,
        previous_row,
    ) == "NORMAL_FLIGHT"


def test_heading_wraparound_1_to_359(labeler):
    previous_row = make_row(
        heading=1,
    )

    current_row = make_row(
        altitude=2000,
        velocity=120,
        vertical_speed=400,
        heading=359,
        roll=0,
    )

    assert labeler._calculate_heading_change(1, 359) == -2

    assert labeler.label_row(
        current_row,
        previous_row,
    ) == "NORMAL_FLIGHT"


# ---------------------------------------------------------------------------
# Threshold boundary tests
# ---------------------------------------------------------------------------

def test_high_speed_boundary(labeler):
    # HIGH_SPEED requires velocity > 200.
    row = make_row(
        altitude=5000,
        velocity=200,
    )

    assert labeler.label_row(row) != "HIGH_SPEED"


def test_high_speed_above_boundary(labeler):
    row = make_row(
        altitude=5000,
        velocity=201,
        vertical_speed=400,
    )

    assert labeler.label_row(row) == "HIGH_SPEED"


def test_low_speed_boundary(labeler):
    # LOW_SPEED requires velocity < 60.
    row = make_row(
        altitude=5000,
        velocity=60,
    )

    assert labeler.label_row(row) != "LOW_SPEED"


def test_high_g_force_boundary(labeler):
    # HIGH_G_FORCE requires g_force > 1.5.
    row = make_row(
        altitude=5000,
        velocity=120,
        g_force=1.5,
    )

    assert labeler.label_row(row) != "HIGH_G_FORCE"


def test_high_altitude_boundary(labeler):
    # HIGH_ALTITUDE requires altitude > 10000.
    row = make_row(
        altitude=10000,
        velocity=120,
    )

    assert labeler.label_row(row) != "HIGH_ALTITUDE"


def test_low_altitude_boundary(labeler):
    # LOW_ALTITUDE requires altitude < 1000 and > 100.
    row = make_row(
        altitude=1000,
        velocity=150,
    )

    assert labeler.label_row(row) != "LOW_ALTITUDE"
