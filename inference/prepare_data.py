import csv
import json
import math
import random
from pathlib import Path
from typing import Dict, List, Optional


FEATURE_COLUMNS = [
    "altitude",
    "heading",
    "vertical_speed",
    "velocity",
    "roll",
    "pitch",
    "yaw",
    "g_force",
]
LABEL_COLUMN = "event_label"
TEST_SIZE = 0.2
RANDOM_STATE = 42


def resolve_input_file(inference_dir: Path) -> Path:
    candidates = [
        inference_dir / "labeled_flight_data.csv",
        inference_dir / "Data" / "labeled_flight_data.csv",
    ]
    for path in candidates:
        if path.exists():
            return path
    raise FileNotFoundError(
        "Could not find labeled_flight_data.csv in inference/ or inference/Data/."
    )


def parse_float(value: str) -> Optional[float]:
    if value is None:
        return None
    value = value.strip()
    if not value:
        return None
    try:
        return float(value)
    except ValueError:
        return None


def median(values: List[float]) -> float:
    ordered = sorted(values)
    midpoint = len(ordered) // 2
    if len(ordered) % 2 == 1:
        return ordered[midpoint]
    return (ordered[midpoint - 1] + ordered[midpoint]) / 2


def mean(values: List[float]) -> float:
    return sum(values) / len(values)


def stddev(values: List[float], avg: float) -> float:
    variance = sum((value - avg) ** 2 for value in values) / len(values)
    standard_deviation = math.sqrt(variance)
    return standard_deviation if standard_deviation != 0 else 1.0


def write_csv(path: Path, fieldnames: List[str], rows: List[Dict[str, object]]) -> None:
    with path.open("w", newline="", encoding="utf-8") as file:
        writer = csv.DictWriter(file, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def main() -> None:
    inference_dir = Path(__file__).resolve().parent
    input_file = resolve_input_file(inference_dir)
    output_dir = inference_dir / "dataset"
    output_dir.mkdir(parents=True, exist_ok=True)

    with input_file.open(newline="", encoding="utf-8") as file:
        reader = csv.DictReader(file)
        if reader.fieldnames is None:
            raise ValueError("Input CSV is missing a header row.")

        required_columns = FEATURE_COLUMNS + [LABEL_COLUMN]
        missing_columns = [
            column for column in required_columns if column not in reader.fieldnames
        ]
        if missing_columns:
            raise ValueError(f"Missing required columns: {missing_columns}")

        raw_rows = [row for row in reader if row.get(LABEL_COLUMN, "").strip()]

    feature_values: Dict[str, List[float]] = {column: [] for column in FEATURE_COLUMNS}
    cleaned_rows: List[Dict[str, object]] = []

    for row in raw_rows:
        cleaned_row: Dict[str, object] = {LABEL_COLUMN: row[LABEL_COLUMN].strip()}
        for column in FEATURE_COLUMNS:
            numeric_value = parse_float(row.get(column, ""))
            cleaned_row[column] = numeric_value
            if numeric_value is not None:
                feature_values[column].append(numeric_value)
        cleaned_rows.append(cleaned_row)

    fill_values = {}
    for column, values in feature_values.items():
        if not values:
            raise ValueError(f"Column '{column}' has no valid numeric values.")
        fill_values[column] = median(values)

    for row in cleaned_rows:
        for column in FEATURE_COLUMNS:
            if row[column] is None:
                row[column] = fill_values[column]

    unique_labels = sorted({row[LABEL_COLUMN] for row in cleaned_rows})
    label_mapping = {label: index for index, label in enumerate(unique_labels)}
    for row in cleaned_rows:
        row["label_encoded"] = label_mapping[row[LABEL_COLUMN]]

    random_generator = random.Random(RANDOM_STATE)
    random_generator.shuffle(cleaned_rows)

    split_index = int(len(cleaned_rows) * (1 - TEST_SIZE))
    if len(cleaned_rows) > 1:
        split_index = max(1, min(split_index, len(cleaned_rows) - 1))

    train_rows = cleaned_rows[:split_index]
    test_rows = cleaned_rows[split_index:]

    means = {
        column: mean([float(row[column]) for row in train_rows]) for column in FEATURE_COLUMNS
    }
    stds = {
        column: stddev([float(row[column]) for row in train_rows], means[column])
        for column in FEATURE_COLUMNS
    }

    def scaled_feature_rows(rows: List[Dict[str, object]]) -> List[Dict[str, float]]:
        return [
            {
                column: (float(row[column]) - means[column]) / stds[column]
                for column in FEATURE_COLUMNS
            }
            for row in rows
        ]

    x_train_rows = scaled_feature_rows(train_rows)
    x_test_rows = scaled_feature_rows(test_rows)
    y_train_rows = [{"label_encoded": row["label_encoded"]} for row in train_rows]
    y_test_rows = [{"label_encoded": row["label_encoded"]} for row in test_rows]

    train_processed_rows = []
    for feature_row, original_row in zip(x_train_rows, train_rows):
        combined_row = dict(feature_row)
        combined_row[LABEL_COLUMN] = original_row[LABEL_COLUMN]
        combined_row["label_encoded"] = original_row["label_encoded"]
        train_processed_rows.append(combined_row)

    test_processed_rows = []
    for feature_row, original_row in zip(x_test_rows, test_rows):
        combined_row = dict(feature_row)
        combined_row[LABEL_COLUMN] = original_row[LABEL_COLUMN]
        combined_row["label_encoded"] = original_row["label_encoded"]
        test_processed_rows.append(combined_row)

    write_csv(output_dir / "X_train.csv", FEATURE_COLUMNS, x_train_rows)
    write_csv(output_dir / "X_test.csv", FEATURE_COLUMNS, x_test_rows)
    write_csv(output_dir / "y_train.csv", ["label_encoded"], y_train_rows)
    write_csv(output_dir / "y_test.csv", ["label_encoded"], y_test_rows)
    write_csv(
        output_dir / "train_processed.csv",
        FEATURE_COLUMNS + [LABEL_COLUMN, "label_encoded"],
        train_processed_rows,
    )
    write_csv(
        output_dir / "test_processed.csv",
        FEATURE_COLUMNS + [LABEL_COLUMN, "label_encoded"],
        test_processed_rows,
    )

    with (output_dir / "label_mapping.json").open("w", encoding="utf-8") as file:
        json.dump(label_mapping, file, indent=2)

    scaler_params = {
        "mean": means,
        "std": stds,
        "feature_columns": FEATURE_COLUMNS,
        "filled_missing_with_median": fill_values,
    }
    with (output_dir / "scaler_params.json").open("w", encoding="utf-8") as file:
        json.dump(scaler_params, file, indent=2)

    print(f"Input file: {input_file}")
    print(f"Saved processed dataset to: {output_dir}")
    print(f"Rows kept: {len(cleaned_rows)}")
    print(f"Train rows: {len(train_rows)}")
    print(f"Test rows: {len(test_rows)}")
    print(f"Labels encoded: {label_mapping}")


if __name__ == "__main__":
    main()
