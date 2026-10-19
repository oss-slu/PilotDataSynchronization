# Inference

This directory holds the Python side of the project: collecting telemetry from the relay, and
forming **performance groups** of pilots from how far they deviate from their targets (see
`GLOSSARY.md`). The older flight event classification pipeline (a rule-based labeler and a Random
Forest) is kept in its own folder, **out of scope** for the client's request; see
[Out of scope: event classification](#out-of-scope-event-classification).

## Layout

```
inference/
  pyproject.toml, uv.lock
  data/                         input and output CSVs (not code)
    prepared/                   train/test splits written by event_classification
  models/                       trained models (not tracked; train_model.py writes them)
  pilot_inference/              the package
    angles.py                   wrapped angular difference, shared
    paths.py                    where data/ and models/ are
    capture/                    data_logger.py
    performance/                features, clustering, synthetic pilots and their CLIs
    event_classification/       OUT OF SCOPE: labeler, Random Forest, evaluation
  tests/                        mirrors pilot_inference/
```

## Prerequisites

**Supported Python version: 3.11 or newer** (matches `.python-version`; the Random Forest models
were pickled with a scikit-learn version that only installs on 3.11+, and earlier versions can't
load them)

Using [uv](https://docs.astral.sh/uv/) (recommended):

```
cd inference
uv sync
```

`uv sync` reads `pyproject.toml` / `uv.lock`, creates `inference/.venv` and installs the
`pilot_inference` package into it in editable mode, so `python -m pilot_inference...` works from
any directory. Prefix commands with `uv run` (from the project root, add `--project inference`),
or activate `inference/.venv` and run `python` directly.

Or with a plain virtual environment, from the project root:

```
python -m venv .venv

# Windows
.venv\Scripts\activate

# macOS/Linux
source .venv/bin/activate

python -m pip install -e inference
```

The plain-venv install does not include pytest; add it with `python -m pip install pytest`.

Use `python`, not `python3`: a Windows venv only contains `python.exe` (no `python3.exe`), so
`python3 -m pip` would fall through to a system Python instead of the activated venv.

## Commands

Run every command from `inference/` with `uv run` (or with the virtual environment activated).
Relative paths such as `--input` resolve against the current directory; the defaults always
resolve against `inference/data/`, wherever you run from.

```
# Capture: collect data from the relay (leave running while flying/simulating)
uv run python -m pilot_inference.capture.data_logger --port 5001 --pilot-id pilot-07 --flight-id 2026-10-12-leg1

# Performance groups, without hardware
uv run python -m pilot_inference.performance.generate_synthetic_pilots
uv run python -m pilot_inference.performance.compute_features --input data/synthetic_pilot_telemetry.csv
uv run python -m pilot_inference.performance.cluster_pilots
uv run python -m pilot_inference.performance.cluster_pilots --k 3 --truth data/synthetic_pilot_tiers.csv
```

`--pilot-id` labels every row so samples can be grouped per pilot and joined to pilot metadata.
Without it the rows are written as "unknown". `--flight-id` is optional, but pass it for a real
session: without it each relay connection gets its own generated id, so a reconnect mid-flight
splits one flight into two. The logger writes `data/session_flight_data.csv`.

## Performance groups

| Step | Module                                            | Purpose                                                                                      | Input                                                                               | Output                                                                                  |
| ---- | ------------------------------------------------- | -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| 1    | `pilot_inference.capture.data_logger`             | Collects live telemetry from the relay over TCP, tagged with `pilot_id` and `flight_id`      | Telemetry socket stream                                                             | `data/session_flight_data.csv`                                                          |
| 2a   | `pilot_inference.performance.generate_synthetic_pilots` | (Optional, no hardware needed) Generates synthetic pilots in 3 hidden skill tiers, with targets | none                                                                                | `data/synthetic_pilot_telemetry.csv`, `data/synthetic_pilot_tiers.csv`                  |
| 2    | `pilot_inference.performance.compute_features`    | Computes deviation features per flight and per pilot from telemetry that carries targets      | Telemetry CSV with `pilot_id`, `flight_id` and `target_*` columns (only 2a produces these today) | `data/flight_features.csv`, `data/pilot_features.csv`                                   |
| 3    | `pilot_inference.performance.cluster_pilots`      | Sweeps k, or with `--k` forms performance groups                                              | `data/pilot_features.csv`                                                           | Sweep: console only. `--k`: `data/pilot_clusters.csv`, `data/group_centres.csv`          |

### Deviation features

`pilot_inference.performance.compute_features` turns telemetry that carries targets into deviation
features per flight and per pilot (see `docs/telemetry_schema.md` section 5). The input needs
`pilot_id`, `flight_id`, the four actual values and `target_altitude`, `target_heading`,
`target_vertical_speed` and `target_airspeed`, in post-#196 units. The committed CSVs lack these
columns; until real flights carry them, use `generate_synthetic_pilots` (step 2a).

```
uv run python -m pilot_inference.performance.compute_features --input <telemetry_with_targets.csv>
```

Outputs `data/flight_features.csv` and `data/pilot_features.csv`, in raw units. Per metric
(altitude, heading, vertical speed, airspeed) it gives `_mad` (mean absolute deviation), `_std`
(standard deviation of the signed deviation), `_bias` (mean signed deviation) and `_abs_bias` (the
absolute value of each flight's `_bias`, averaged per pilot, so flights on opposite sides of the
target do not cancel). Clustering uses `_std` and `_abs_bias`. The flight file also has
`_rows_used`, the samples that had a valid deviation; the pilot file has a `flights` count
instead.

A flight with no valid target for a metric gets NaN for it and a warning, and its other metrics
are kept. A pilot's feature averages over the flights that have it; a pilot with none gets NaN and
a warning. Rows with a null `pilot_id` or `flight_id` raise an error.

### Forming the groups

`pilot_inference.performance.cluster_pilots` groups pilots with KMeans on `std` and `|bias|` for
each metric, log-scaled and then standardized over the cohort (`docs/adr/0002`). Pilot metadata is
not an input; compare it against the groups afterwards (`docs/adr/0001`). Any pilot with a NaN
feature stops the run with an error naming the pilot and metric.

The script never picks k. Run it once without `--k` to print, for k = 2 to min(6, pilots // 3):

- `silhouette`: how cohesive and separated the groups are, from -1 to 1
- `stability`: mean adjusted Rand index between the full fit and refits on random 80% subsamples
  of pilots; 1 means the same groups every time

Then rerun with the chosen `--k`, which must be one of the swept values (so at least 6 pilots, and
3 or more per group on average). It writes each pilot's group and each group's centre, as the mean
of its members in raw units (ft, deg, ft/min, kts). For synthetic pilots, `--truth` also prints
the adjusted Rand index against the hidden tiers; the file must list each pilot once. The
synthetic tiers overlap, so this is below 1: the default seed gives 0.79, and generator seeds 0 to
11 range from 0.37 to 1.00.

## Tests

```
uv sync              # installs pytest from the dev group
uv run pytest
```

`tests/` mirrors the package: `tests/capture/`, `tests/performance/` (deviation features, the
synthetic pilot generator and performance groups, including recovering the synthetic tiers end to
end), `tests/event_classification/` (the labeler's heading change), and `tests/test_angles.py`
for the angle helper.

## Out of scope: event classification

`pilot_inference/event_classification/` holds a rule-based labeler and a Random Forest that predict
one of 13 flight event labels (`TAXI`, `TAKEOFF`, `CRUISE`, `APPROACH`, `LANDING`, turns,
speed/altitude extremes, high-g events, and normal flight) for every sample. The client has not
asked for this; it is kept apart so it does not get in the way of the performance groups. Flight
segmentation is later work: the client segments flights by hand today, and detecting
instructions automatically (for example from audio) would replace that.

| Step | Module                                                   | Purpose                                                                                                                                 | Input                                          | Output                                                                       |
| ---- | -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- | ---------------------------------------------------------------------------- |
| 1    | `pilot_inference.event_classification.generate_balanced_data` | (Optional, no hardware needed) Generates synthetic, class-balanced flight data                                                    | none                                           | `data/synthetic_flight_data.csv`                                             |
| 2    | `pilot_inference.event_classification.label_generator`   | Applies rule-based thresholds (altitude, speed, vertical speed, roll, g-force, heading change) to assign one of 13 labels per row        | `data/raw_flight_data.csv`                     | `data/labeled_flight_data.csv`                                               |
| 3    | `pilot_inference.event_classification.validate_labels`   | Sanity-checks the labeled dataset: required columns, valid label set, data ranges, spot-checks, and label distribution                  | `data/labeled_flight_data.csv`                 | Console report only (exit code 0/1)                                          |
| 4    | `pilot_inference.event_classification.prepare_data`      | (Optional) Cleans, median-fills, standardizes, and splits the labeled data into train/test CSVs, independent of training               | `data/labeled_flight_data.csv`                 | `data/prepared/*.csv`, `label_mapping.json`, `scaler_params.json`            |
| 5    | `pilot_inference.event_classification.train_model`       | Trains a Random Forest classifier (80/20 stratified split) and reports accuracy/precision/recall                                        | `data/labeled_flight_data.csv`                 | `models/bestModel.pkl`, `data/prepared/test.csv`                             |
| 6    | `pilot_inference.event_classification.evaluate_model`    | Loads the trained model, runs inference, and (if ground-truth labels are present) evaluates it                                          | Trained model + held-out test data (see below) | `predictions_output.csv`, `evaluation_metrics.json` (in `inference/`)        |

```
uv run python -m pilot_inference.event_classification.generate_balanced_data  # optional
uv run python -m pilot_inference.event_classification.label_generator
uv run python -m pilot_inference.event_classification.validate_labels
uv run python -m pilot_inference.event_classification.prepare_data            # optional
uv run python -m pilot_inference.event_classification.train_model
uv run python -m pilot_inference.event_classification.evaluate_model
```

### Which data the committed files come from

The committed `data/labeled_flight_data.csv` has 16,604 rows across 9 event classes and was
labeled from **`data/raw_flight_data_updated.csv`**. `label_generator` reads
`data/raw_flight_data.csv` (597 rows, 2 classes), so running it as documented **overwrites the
committed labeled file** with a much smaller one. Restore it with
`git checkout -- data/labeled_flight_data.csv`, or copy `raw_flight_data_updated.csv` over
`raw_flight_data.csv` first. Both raw files, and the labeled one, predate the `pilot_id` /
`flight_id` columns and the #196 unit fix (see `docs/telemetry_schema.md`).

`data/synthetic_flight_data.csv` (step 1) and `data/session_flight_data.csv` (capture) are not
read automatically: copy or rename the one you want to `data/raw_flight_data.csv` first.

### Step 4 and steps 5/6 are independent

`train_model` reads directly from `data/labeled_flight_data.csv`; it does **not** consume
`prepare_data`'s output. Run `prepare_data` only if you need a separately scaled/split copy of the
data (e.g. for a different model or notebook). `train_model` does write its own held-out 20% split
to `data/prepared/test.csv`, which `evaluate_model` reads first. This is a different file from
`prepare_data`'s `data/prepared/test_processed.csv`.

### Event labels

**Flight phases**

- `TAXI`: Ground operations (altitude < 50ft, speed < 30 knots)
- `TAKEOFF`: Transition from ground to air (low altitude, climbing, 50-100 knots)
- `CRUISE`: Steady flight at altitude (altitude > 3000ft, stable vertical speed)
- `APPROACH`: Descending for landing (500-3000ft, descending)
- `LANDING`: Final approach and touchdown (altitude < 500ft, descending)

**Maneuver events**

- `TURN_LEFT`: Left turn (roll < -5° or heading change < -3°/s)
- `TURN_RIGHT`: Right turn (roll > 5° or heading change > 3°/s)

**Speed events**

- `HIGH_SPEED`: Velocity > 200 knots
- `LOW_SPEED`: Velocity < 60 knots (while airborne)

**Altitude events**

- `HIGH_ALTITUDE`: Altitude > 10,000 feet
- `LOW_ALTITUDE`: Altitude < 1,000 feet (while airborne)

**Special events**

- `HIGH_G_FORCE`: G-force > 1.5g
- `NORMAL_FLIGHT`: Default steady flight (none of the above conditions)

See `FlightEventLabeler.label_row()` in `label_generator.py` for the exact priority order and
thresholds.

### Model details

`train_model` trains a `RandomForestClassifier` on 8 input features (`altitude`, `heading`,
`vertical_speed`, `velocity`, `roll`, `pitch`, `yaw`, `g_force`) against the `event_label` target,
with an 80/20 stratified train/test split. When run, it loads the labeled dataset, displays the
class distribution, splits into train/test sets, saves the held-out test split to
`data/prepared/test.csv`, trains the model, displays feature importance, evaluates on the test
set, prints accuracy/precision/recall, and saves the trained model.

Model parameters:

- `n_estimators`: 100 trees
- `max_depth`: None (unlimited depth)
- `min_samples_split`: 2
- `min_samples_leaf`: 1
- `random_state`: 42 (for reproducibility, both for the model and the train/test split)

`models/bestModel.pkl` is generated, so it is not tracked in git; run `train_model` to create it,
and load it with `joblib.load()`. The altitude and velocity it learns from are mis-scaled (see
`docs/telemetry_schema.md`), so it will not be right on current telemetry.

`evaluate_model` loads `models/bestModel.pkl`. It looks for test data in this order:
`data/prepared/test.csv` (the held-out split `train_model` saves, which the model never trained
on), then `data/labeled_flight_data.csv` (fallback; this is the full dataset the model _did_ train
on, so metrics from this fallback measure memorization, not generalization), then
`inference/labeled_flight_data.csv`. Run `train_model` before `evaluate_model` so the held-out
split exists and the first candidate is used.

## Troubleshooting

- **`ModuleNotFoundError: No module named 'pilot_inference'`**: the package is not installed in
  the environment you are running. Run `uv sync` in `inference/`, or `python -m pip install -e
  inference` in a plain venv. Check you are using that environment's `python`.
- **`ValueError: ... has columns [...], but this logger writes [...]` from `data_logger`**: `--csv`
  points at a CSV written before `pilot_id` and `flight_id` existed, such as the committed
  `data/raw_flight_data.csv`. Appending would misalign every row, so the logger stops. Pass a new
  path, or move the old file aside. This is why the logger writes its own session file by default:
  the committed CSVs predate both columns and the #196 unit fix, so appending to one would also
  mix two scales in a single file.
- **`FileNotFoundError` for `raw_flight_data.csv` in `label_generator`**: neither capture nor
  `generate_balanced_data` overwrites the committed `raw_flight_data.csv`. Copy or rename the file
  you want (see "Which data the committed files come from") before running the labeler.
- **`ValueError: Missing required columns` in `label_generator`, `validate_labels`, or
  `prepare_data`**: the input CSV is missing one of `altitude`, `velocity`, `vertical_speed`,
  `heading`, `roll`, `g_force` (and `pitch`/`yaw`/`event_label` for later steps). Check the header
  row of your CSV against `CSV_FIELDS` in `pilot_inference/capture/data_logger.py`.
- **`validate_labels` reports "Negative velocity values found" or missing label categories**: this
  is expected with small or synthetic datasets that don't exercise every flight phase. It's a
  warning, not a hard failure; `validate_labels` still exits 0 as long as required columns and
  label formatting are valid.
- **`FileNotFoundError: No model files found` in `evaluate_model`**: run `train_model` first to
  produce `models/bestModel.pkl`.
- **`uv run` uses the wrong Python or the wrong project**: run `uv` commands from inside
  `inference/` (where `pyproject.toml` lives), or pass `--project inference` from the repository
  root.
