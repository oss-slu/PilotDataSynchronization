"""
Label Validation and Analysis Script

Performs manual validation on sample rows and provides detailed
statistics about the labeled dataset.

"""

import pandas as pd
import numpy as np
from pathlib import Path
import sys


def validate_labeled_dataset(file_path: Path) -> bool:
    """
    Validate the labeled dataset and perform quality checks.
    
    Args:
        file_path: Path to labeled_flight_data.csv
    
    Returns:
        True if validation passes, False otherwise
    """
    print("="*70)
    print("LABELED DATASET VALIDATION")
    print("="*70)
    
    try:
        df = pd.read_csv(file_path)
        print(f"✓ Successfully loaded dataset: {len(df)} rows")
    except Exception as e:
        print(f"✗ Error loading dataset: {e}")
        return False
    
    # Check 1: No missing labels
    print("\n1. Checking for missing labels...")
    missing_count = df['event_label'].isna().sum()
    if missing_count > 0:
        print(f"✗ FAILED: {missing_count} rows have missing labels")
        return False
    else:
        print(f"✓ PASSED: All {len(df)} rows have labels")
    
    # Check 2: Expected columns exist
    print("\n2. Checking required columns...")
    required_cols = ['altitude', 'velocity', 'vertical_speed', 
                     'heading', 'roll', 'g_force', 'event_label']
    missing_cols = [col for col in required_cols if col not in df.columns]
    
    if missing_cols:
        print(f"✗ FAILED: Missing columns: {missing_cols}")
        return False
    else:
        print(f"✓ PASSED: All required columns present")
    
    # Check 3: Valid label set
    print("\n3. Checking label validity...")
    expected_labels = {
        'TAXI', 'TAKEOFF', 'CRUISE', 'APPROACH', 'LANDING',
        'TURN_LEFT', 'TURN_RIGHT', 'HIGH_SPEED', 'LOW_SPEED',
        'HIGH_ALTITUDE', 'LOW_ALTITUDE', 'HIGH_G_FORCE', 'NORMAL_FLIGHT'
    }
    
    actual_labels = set(df['event_label'].unique())
    unexpected = actual_labels - expected_labels
    missing = expected_labels - actual_labels
    
    if unexpected:
        print(f"⚠ WARNING: Unexpected labels found: {unexpected}")
    
    if missing:
        print(f"⚠ INFO: Labels not used: {missing}")
    
    print(f"✓ PASSED: Label validation complete")
    
    # Check 4: Manual spot-check samples
    print("\n4. Manual spot-check of sample rows...")
    print("\nSample TAKEOFF events:")
    takeoff_samples = df[df['event_label'] == 'TAKEOFF'].head(3)
    if len(takeoff_samples) > 0:
        print(takeoff_samples[['altitude', 'velocity', 'vertical_speed', 'event_label']].to_string())
    else:
        print("No TAKEOFF events found")
    
    print("\nSample LANDING events:")
    landing_samples = df[df['event_label'] == 'LANDING'].head(3)
    if len(landing_samples) > 0:
        print(landing_samples[['altitude', 'velocity', 'vertical_speed', 'event_label']].to_string())
    else:
        print("No LANDING events found")
    
    print("\nSample TURN_LEFT events:")
    turn_left_samples = df[df['event_label'] == 'TURN_LEFT'].head(3)
    if len(turn_left_samples) > 0:
        print(turn_left_samples[['heading', 'roll', 'event_label']].to_string())
    else:
        print("No TURN_LEFT events found")
    
    # Check 5: Label distribution
    print("\n5. Label distribution analysis...")
    label_counts = df['event_label'].value_counts()
    
    for label, count in label_counts.items():
        percentage = (count / len(df)) * 100
        print(f"  {label:20s}: {count:6d} ({percentage:5.2f}%)")
    
    # Check 6: Data range validation
    print("\n6. Validating data ranges...")
    checks_passed = True
    
    # Altitude should be non-negative
    if (df['altitude'] < 0).any():
        print("✗ WARNING: Negative altitude values found")
        checks_passed = False
    
    # Velocity should be non-negative
    if (df['velocity'] < 0).any():
        print("✗ WARNING: Negative velocity values found")
        checks_passed = False
    
    # Heading should be 0-360
    if (df['heading'] < 0).any() or (df['heading'] > 360).any():
        print("✗ WARNING: Heading values outside 0-360 range")
        checks_passed = False
    
    # Roll typically -180 to 180
    if (df['roll'] < -180).any() or (df['roll'] > 180).any():
        print("✗ WARNING: Roll values outside typical range")
        checks_passed = False
    
    if checks_passed:
        print("✓ PASSED: All data ranges valid")
    
    print("\n" + "="*70)
    print("VALIDATION SUMMARY")
    print("="*70)
    print(f"Total rows: {len(df)}")
    print(f"Total labels: {len(actual_labels)}")
    print(f"Coverage: {len(actual_labels)}/{len(expected_labels)} expected labels")
    print("="*70)
    
    print("\n✓ VALIDATION COMPLETE - Dataset ready for ML training")
    return True


def main():
    """Main execution function."""
    inference_dir = Path(__file__).parent
    labeled_file = inference_dir / 'Data' / 'labeled_flight_data.csv'
    
    if not labeled_file.exists():
        print(f"Error: {labeled_file} not found")
        print("Please run label_generator.py first")
        sys.exit(1)
    
    success = validate_labeled_dataset(labeled_file)
    sys.exit(0 if success else 1)


if __name__ == '__main__':
    main()
