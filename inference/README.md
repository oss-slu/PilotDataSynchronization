# Flight Event ML Pipeline

This directory contains the complete machine learning pipeline for flight telemetry: collecting
raw data, generating rule-based labels, preparing datasets, and training/testing a Random Forest
classifier that predicts flight events (`TAXI`, `TAKEOFF`, `CRUISE`, `APPROACH`, `LANDING`, turns,
speed/altitude extremes, high-g events, and normal flight).

## Prerequisites

**Supported Python version: 3.9 - 3.12 (developed/tested with Python 3.11)**

Using [uv](https://docs.astral.sh/uv/) (recommended):

```
cd inference
uv sync
```

`uv sync` reads `inference/pyproject.toml` / `inference/uv.lock` and creates `inference/.venv`
automatically. Prefix commands with `uv run` (e.g. `uv run python inference/label_generator.py`),
or activate `inference/.venv` and run `python` directly.

Or with a plain virtual environment, from the project root:

```
python3 -m venv .venv

# Windows
.venv\Scripts\activate

# macOS/Linux
source .venv/bin/activate

python3 -m pip install -r inference/requirements.txt
```

No other manual package installation is required.

## Pipeline overview

The scripts are meant to be run in this order. Every command below is written to be run **from
the project root** (`PilotDataSynchronization/`), with the virtual environment activated.

| Step | Script | Purpose | Input | Output |
|---|---|---|---|---|
| 1 | `inference/Data/data_logger.py` | Collects live telemetry from the relay over TCP and appends it to a raw CSV | Telemetry socket stream | `inference/Data/raw_flight_data.csv` |
| 1b | `inference/generate_balanced_data.py` | (Optional, no hardware needed) Generates synthetic, class-balanced flight data for testing the pipeline | none | `inference/Data/raw_flight_data.csv` |
| 2 | `inference/label_generator.py` | Applies rule-based thresholds (altitude, speed, vertical speed, roll, g-force, heading change) to assign one of 13 event labels per row | `inference/Data/raw_flight_data_updated.csv` | `inference/Data/labeled_flight_data.csv` |
| 3 | `inference/validate_labels.py` | Sanity-checks the labeled dataset: required columns, valid label set, data ranges, spot-checks, and label distribution | `inference/Data/labeled_flight_data.csv` | Console report only (exit code 0/1) |
| 4 | `inference/prepare_data.py` | (Optional) Cleans, median-fills, standardizes, and splits the labeled data into train/test CSVs — independent of the training step below | `inference/Data/labeled_flight_data.csv` | `inference/dataset/*.csv`, `label_mapping.json`, `scaler_params.json` |
| 5 | `inference/train_model.py` | Trains a Random Forest classifier (80/20 stratified split) and reports accuracy/precision/recall | `inference/Data/labeled_flight_data.csv` | `inference/Models/bestModel.pkl`, `inference/Models/finalModel.pkl` |
| 6 | `inference/test_model.py` | Loads a trained model, runs inference, and (if ground-truth labels are present) evaluates it | Trained model + labeled/test data (see below) | `inference/predictions_output.csv`, `inference/evaluation_metrics.json` |

### Step 4 and step 5/6 are independent

`train_model.py` and `test_model.py` read directly from `inference/Data/labeled_flight_data.csv` —
they do **not** consume `prepare_data.py`'s output in `inference/dataset/`. Run `prepare_data.py`
only if you need a separately scaled/split copy of the data (e.g. for a different model or
notebook); it is not required to train or test the shipped Random Forest model.

## Commands (run from the project root)

```
# 1. Collect data from the relay (leave running while flying/simulating)
python inference/Data/data_logger.py --port 5001

#   ...or, without hardware, generate synthetic balanced data instead:
python inference/generate_balanced_data.py

# 2. Generate labels
#    label_generator.py reads inference/Data/raw_flight_data_updated.csv by default.
#    If you produced inference/Data/raw_flight_data.csv in step 1 instead, copy/rename
#    it to raw_flight_data_updated.csv first (see Troubleshooting).
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
class distribution, splits into train/test sets, trains the model, displays feature importance,
evaluates on the test set, prints accuracy/precision/recall, and saves the trained model.

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
`inference/dataset/test.csv` (not produced by any script here — `prepare_data.py` writes
`test_processed.csv`, so this candidate is normally skipped), then
`inference/Data/labeled_flight_data.csv`, then `inference/labeled_flight_data.csv`.

## Troubleshooting

- **`FileNotFoundError` for `raw_flight_data_updated.csv` when running `label_generator.py`** —
  the script hardcodes `inference/Data/raw_flight_data_updated.csv` as its input, which is not the
  same filename that `data_logger.py` (`raw_flight_data.csv`) or `generate_balanced_data.py`
  (also `raw_flight_data.csv`) write. Copy or rename your generated file to
  `raw_flight_data_updated.csv` before running `label_generator.py`, or edit the `input_file`
  path in `label_generator.py`'s `main()`.
- **`UnicodeEncodeError: 'charmap' codec can't encode character '✓'` on Windows** — several
  scripts print a ✓ character, and the default Windows console codepage (cp1252) can't encode it.
  Set `PYTHONUTF8=1` before running (e.g. `set PYTHONUTF8=1` in cmd.exe,
  `$env:PYTHONUTF8=1` in PowerShell, `export PYTHONUTF8=1` in bash), or run `chcp 65001` first.
- **`ValueError: Missing required columns` in `label_generator.py`, `validate_labels.py`, or
  `prepare_data.py`** — the input CSV is missing one of `altitude`, `velocity`, `vertical_speed`,
  `heading`, `roll`, `g_force` (and `pitch`/`yaw`/`event_label` for later steps). Check the header
  row of your CSV against `CSV_FIELDS` in `Data/data_logger.py`.
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
- `Data/raw_flight_data.csv`, `Data/raw_flight_data_updated.csv` — Raw telemetry input (not tracked for new data; sample files present in this repo)
- `Data/labeled_flight_data.csv` — Labeled dataset output of step 2 / input to steps 3-6
- `dataset/` — Output of `prepare_data.py` (optional)
- `Models/` — Trained model files (`bestModel.pkl`, `finalModel.pkl`)
- `predictions_output.csv`, `evaluation_metrics.json` — Output of `test_model.py`

## Verification

This workflow was verified end-to-end from the project root in a fresh virtual environment
(`python -m venv`, `pip install -r inference/requirements.txt`), running steps 1b through 6 in
order and confirming each script's documented input/output files were produced correctly.
