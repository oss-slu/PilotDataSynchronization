# Flight Event Prediction - Model Training

This document explains how to train the Random Forest machine learning model for flight event prediction.

## Overview

The `train_model.py` script trains a Random Forest classifier to predict flight events based on 8 flight parameters:

**Input Features:**
- `altitude` - Aircraft altitude (feet)
- `heading` - Aircraft heading (degrees)
- `vertical_speed` - Vertical velocity (feet/min)
- `velocity` - Airspeed (knots)
- `roll` - Roll angle (degrees)
- `pitch` - Pitch angle (degrees)
- `yaw` - Yaw angle (degrees)
- `g_force` - G-force experienced

**Output:**
- `event_label` - Predicted flight event (e.g., TAXI, TAKEOFF, CRUISE, LANDING)

## Prerequisites

### Supported Python Version

Python 3.9 - 3.12 (developed/tested with Python 3.11).

### Required Python Packages

Install all dependencies in one step from `inference/requirements.txt` (no separate manual installs needed):

```
python3 -m pip install -r inference/requirements.txt
```

### Required Data
Ensure you have labeled training data at:
inference/Data/labeled_flight_data.csv


This file should be generated using `label_generator.py` before training.

## How to Run
 py .\inference\train_model.py

### Basic Usage
From the `inference/` directory: python train_model.py

Or from the project root: python inference/train_model.py


### Expected Output
The script will:
1. Load the labeled dataset
2. Display class distribution
3. Split data into training (80%) and testing (20%) sets
4. Train a Random Forest model
5. Display feature importance
6. Evaluate the model on test data
7. Print evaluation metrics (accuracy, precision, recall)
8. Save trained models

## Output Files

The script generates two model files in `inference/models/`:

- **`bestModel.pkl`** - Trained Random Forest model
- **`finalModel.pkl`** - Copy of the trained model (same as bestModel)

Both files contain the same trained model and can be loaded using `joblib.load()`.

## Model Configuration

The Random Forest model uses the following default parameters:
- **n_estimators**: 100 trees
- **max_depth**: None (unlimited depth)
- **min_samples_split**: 2
- **min_samples_leaf**: 1
- **random_state**: 42 (for reproducibility)

