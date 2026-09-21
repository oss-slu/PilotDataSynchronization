# Flight Event ML Pipeline

This directory contains the complete machine learning pipeline for flight telemetry: collecting
raw data, generating rule-based labels, preparing datasets, and training/testing a Random Forest
classifier that predicts flight events (`TAXI`, `TAKEOFF`, `CRUISE`, `APPROACH`, `LANDING`, turns,
speed/altitude extremes, high-g events, and normal flight).

## Prerequisites

**Supported Python version: 3.11 or newer** (matches `.python-version`; the committed models were
pickled with a scikit-learn version that only installs on 3.11+, and earlier versions can't load
them)

Using [uv](https://docs.astral.sh/uv/) (recommended):

```
cd inference
uv sync
```

`uv sync` reads `inference/pyproject.toml` / `inference/uv.lock` and creates `inference/.venv`
automatically. Prefix commands with `uv run` (e.g., from the project root, `uv run --project
inference python inference/label_generator.py`), or activate `inference/.venv` and run `python`
directly.

Or with a plain virtual environment, from the project root:

```
python3 -m venv .venv

# Windows
.venv\Scripts\activate

# macOS/Linux
source .venv/bin/activate

python -m pip install -r inference/requirements.txt
```

`python3` is omitted deliberately: a Windows venv only contains `python.exe` (no `python3.exe`), so
`python3 -m pip` would fall through to a system Python instead of the activated venv. `python -m
pip` resolves to the venv's interpreter on all platforms once activated.

No other manual package installation is required.

## Pipeline overview

The scripts are meant to be run in this order. Every command below is written to be run **from
the project root** (`PilotDataSynchronization/`), with the virtual environment activated.

| Step | Script                                | Purpose                                                                                                                                  | Input                                          | Output                                                                                            |
| ---- | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| 1    | `inference/Data/data_logger.py`       | Collects live telemetry from the relay over TCP and appends it to a raw CSV                                                              | Telemetry socket stream                        | `inference/Data/raw_flight_data.csv`                                                              |
| 1b   | `inference/generate_balanced_data.py` | (Optional, no hardware needed) Generates synthetic, class-balanced flight data for testing the pipeline                                  | none                                           | `inference/Data/synthetic_flight_data.csv`                                                        |
| 2    | `inference/label_generator.py`        | Applies rule-based thresholds (altitude, speed, vertical speed, roll, g-force, heading change) to assign one of 13 event labels per row  | `inference/Data/raw_flight_data.csv`           | `inference/Data/labeled_flight_data.csv`                                                          |
| 3    | `inference/validate_labels.py`        | Sanity-checks the labeled dataset: required columns, valid label set, data ranges, spot-checks, and label distribution                   | `inference/Data/labeled_flight_data.csv`       | Console report only (exit code 0/1)                                                               |
| 4    | `inference/prepare_data.py`           | (Optional) Cleans, median-fills, standardizes, and splits the labeled data into train/test CSVs — independent of the training step below | `inference/Data/labeled_flight_data.csv`       | `inference/dataset/*.csv`, `label_mapping.json`, `scaler_params.json`                             |
| 5    | `inference/train_model.py`            | Trains a Random Forest classifier (80/20 stratified split) and reports accuracy/precision/recall                                         | `inference/Data/labeled_flight_data.csv`       | `inference/Models/bestModel.pkl`, `inference/Models/finalModel.pkl`, `inference/dataset/test.csv` |
| 6    | `inference/test_model.py`             | Loads a trained model, runs inference, and (if ground-truth labels are present) evaluates it                                             | Trained model + held-out test data (see below) | `inference/predictions_output.csv`, `inference/evaluation_metrics.json`                           |

### Step 4 and step 5/6 are independent

`train_model.py` reads directly from `inference/Data/labeled_flight_data.csv` — it does **not**
consume `prepare_data.py`'s output in `inference/dataset/`. Run `prepare_data.py` only if you need
a separately scaled/split copy of the data (e.g. for a different model or notebook); it is not
required to train or test the shipped Random Forest model.

`train_model.py` does, however, write its own held-out 20% split to `inference/dataset/test.csv`,
which `test_model.py` reads first (see "Model details" below) — this is a different file from
`prepare_data.py`'s `dataset/test_processed.csv` and does not depend on step 4 having run.

## Commands (run from the project root)

```
# 1. Collect data from the relay (leave running while flying/simulating)
python inference/Data/data_logger.py --port 5001

#   ...or, without hardware, generate synthetic balanced data instead, then copy/rename
#   inference/Data/synthetic_flight_data.csv to inference/Data/raw_flight_data.csv:
python inference/generate_balanced_data.py

# 2. Generate labels
#    label_generator.py reads inference/Data/raw_flight_data.csv by default.
python inference/label_generator.py

# 3. Validate the labeled dataset
python inference/validate_labels.py

# 4. (Optional) Prepare a scaled train/test split in inference/dataset/
python inference/prepare_data.py

# 5. Train the model
python inference/train_model.py

# 6. Test the model and generate predictions
python inference/test_model.py
```

Each script can also be run from inside `inference/` (e.g. `cd inference && python
label_generator.py`); the scripts resolve their own input/output paths relative to the
`inference/` directory, not the current working directory.

## Event labels

The system labels data with 13 event types:

**Flight phases**

- `TAXI` — Ground operations (altitude < 50ft, speed < 30 knots)
- `TAKEOFF` — Transition from ground to air (low altitude, climbing, 50-100 knots)
- `CRUISE` — Steady flight at altitude (altitude > 3000ft, stable vertical speed)
- `APPROACH` — Descending for landing (500-3000ft, descending)
- `LANDING` — Final approach and touchdown (altitude < 500ft, descending)

**Maneuver events**

- `TURN_LEFT` — Left turn (roll < -5° or heading change < -3°/s)
- `TURN_RIGHT` — Right turn (roll > 5° or heading change > 3°/s)

**Speed events**

- `HIGH_SPEED` — Velocity > 200 knots
- `LOW_SPEED` — Velocity < 60 knots (while airborne)

**Altitude events**

- `HIGH_ALTITUDE` — Altitude > 10,000 feet
- `LOW_ALTITUDE` — Altitude < 1,000 feet (while airborne)

**Special events**

- `HIGH_G_FORCE` — G-force > 1.5g
- `NORMAL_FLIGHT` — Default steady flight (none of the above conditions)

See `FlightEventLabeler.label_row()` in `label_generator.py` for the exact priority order and
thresholds.

## Model details

`train_model.py` trains a `RandomForestClassifier` on 8 input features (`altitude`, `heading`,
`vertical_speed`, `velocity`, `roll`, `pitch`, `yaw`, `g_force`) against the `event_label` target,
with an 80/20 stratified train/test split. When run, it: loads the labeled dataset, displays the
class distribution, splits into train/test sets, saves the held-out test split to
`inference/dataset/test.csv`, trains the model, displays feature importance, evaluates on the test
set, prints accuracy/precision/recall, and saves the trained model.

Model parameters:

- `n_estimators`: 100 trees
- `max_depth`: None (unlimited depth)
- `min_samples_split`: 2
- `min_samples_leaf`: 1
- `random_state`: 42 (for reproducibility, both for the model and the train/test split)

`bestModel.pkl` and `finalModel.pkl` are identical copies of the same trained model, loadable with
`joblib.load()`.

`test_model.py` looks for a model in this order: `inference/Models/bestModel.pkl`, then
`inference/Models/finalModel.pkl`. It looks for test data in this order:
`inference/dataset/test.csv` (the held-out split `train_model.py` saves — the model never trained
on these rows), then `inference/Data/labeled_flight_data.csv` (fallback; this is the full dataset
the model _did_ train on, so metrics from this fallback measure memorization, not generalization),
then `inference/labeled_flight_data.csv`. Run `train_model.py` before `test_model.py` so the
held-out split exists and the first candidate is used.

## Troubleshooting

- **`FileNotFoundError` for `raw_flight_data.csv` when running `label_generator.py` after step
  1b** — `generate_balanced_data.py` writes synthetic data to `inference/Data/synthetic_flight_data.csv`
  rather than overwriting the real collected `raw_flight_data.csv`. Copy or rename it to
  `raw_flight_data.csv` before running `label_generator.py`.
- **`validate_labels.py` reports "Negative velocity values found" or missing label categories** —
  this is expected with small or synthetic datasets that don't exercise every flight phase (e.g.
  `TAKEOFF`, `APPROACH`, `LANDING`, `LOW_ALTITUDE` require specific altitude/speed/vertical-speed
  combinations that a short synthetic run may not produce). It's a warning, not a hard failure;
  `validate_labels.py` still exits 0 as long as required columns and label formatting are valid.
- **`FileNotFoundError: No model files found` in `test_model.py`** — run `train_model.py` first to
  produce `inference/Models/bestModel.pkl`.
- **`inference/.venv` not picked up / `uv run` uses the wrong Python** — make sure you're running
  `uv` commands from inside `inference/` (where `pyproject.toml` lives), or pass `--project
inference` from the repository root.

## Files

- `Data/data_logger.py` — TCP listener that parses relay telemetry packets and appends rows to a raw CSV
- `generate_balanced_data.py` — Generates synthetic, class-balanced flight data for testing the pipeline without hardware
- `label_generator.py` — Rule-based labeling: assigns `event_label` to each row of raw telemetry
- `validate_labels.py` — Validates a labeled dataset's structure, label set, and value ranges
- `prepare_data.py` — Cleans, standardizes, and splits labeled data into train/test CSVs (optional, not used by `train_model.py`/`test_model.py`)
- `train_model.py` — Trains and saves a Random Forest classifier
- `test_model.py` — Loads a trained model, runs inference, evaluates, and saves predictions
- `requirements.txt` / `pyproject.toml` / `uv.lock` — Python dependencies
- `Data/raw_flight_data.csv` — Raw telemetry input (not tracked for new data; a sample file is present in this repo); written by step 1 or renamed from step 1b's output
- `Data/synthetic_flight_data.csv` — Output of `generate_balanced_data.py` (step 1b); rename to `raw_flight_data.csv` to feed it into step 2
- `Data/labeled_flight_data.csv` — Labeled dataset output of step 2 / input to steps 3-6
- `dataset/test.csv` — Held-out test split saved by `train_model.py`; read by `test_model.py`
- `dataset/` (other files) — Output of `prepare_data.py` (optional)
- `Models/` — Trained model files (`bestModel.pkl`, `finalModel.pkl`)
- `predictions_output.csv`, `evaluation_metrics.json` — Output of `test_model.py`

## Verification

Each script's documented input/output paths were checked to chain together correctly (step 1 or
1b's renamed output → step 2 → step 3 → steps 4-6). The committed `Data/labeled_flight_data.csv`
and the trained models under `Models/` come from a real logged flight, not from
`generate_balanced_data.py`'s synthetic output — the committed dataset only contains the 9 event
classes that flight produced, not all 13 the synthetic generator can create.
