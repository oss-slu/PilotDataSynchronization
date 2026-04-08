# Flight Event Labeling System

This directory contains the rule-based labeling system for flight telemetry data, used to prepare datasets for machine learning training.

## Overview

The labeling system assigns flight phase and event labels to raw telemetry data based on rule-based thresholds for parameters like altitude, velocity, vertical speed, heading, roll, and g-force.

## Files

- **`label_generator.py`** - Main labeling script with rule-based logic
- **`validate_labels.py`** - Validation script for labeled data quality checks
- **`generate_sample_data.py`** - (Optional) Sample flight data generator for testing
- **`requirements.txt`** - Python package dependencies
- **`Data/raw_flight_data.csv`** - Input: Raw telemetry data (not tracked in git)
- **`Data/labeled_flight_data.csv`** - Output: Labeled dataset ready for ML training

## Event Labels

The system labels data with the following 13 event types:

### Flight Phases
- **TAXI** - Ground operations (altitude < 50ft, speed < 30 knots)
- **TAKEOFF** - Transition from ground to air (low altitude, climbing, 50-100 knots)
- **CRUISE** - Steady flight at altitude (altitude > 3000ft, stable vertical speed)
- **APPROACH** - Descending for landing (500-3000ft, descending)
- **LANDING** - Final approach and touchdown (altitude < 500ft, descending)

### Maneuver Events
- **TURN_LEFT** - Left turn (roll < -5° or heading change < -3°/s)
- **TURN_RIGHT** - Right turn (roll > 5° or heading change > 3°/s)

### Speed Events
- **HIGH_SPEED** - Velocity > 200 knots
- **LOW_SPEED** - Velocity < 60 knots (while airborne)

### Altitude Events
- **HIGH_ALTITUDE** - Altitude > 10,000 feet
- **LOW_ALTITUDE** - Altitude < 1,000 feet (while airborne)

### Special Events
- **HIGH_G_FORCE** - G-force > 1.5g
- **NORMAL_FLIGHT** - Default steady flight (none of the above conditions)

## Usage

### Prerequisites

**Python 3.7 or higher required**

Install dependencies:
Using pip
py -m pip install -r requirements.txt
Or install packages directly
py -m pip install pandas numpy



1. Collect flight data (using data_logger.py)
cd Data py data_logger.py
2. Generate labels
cd .. py label_generator.py
3. Validate labels
py validate_labels.py
4. Use labeled data for ML training