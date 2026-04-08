"""
Flight Phase and Event Labeling System

This script applies rule-based labeling to raw flight telemetry data.
Labels are assigned based on thresholds for altitude, velocity, vertical speed,
heading, roll, and g-force parameters.

Author: Pilot Training Data Synchronization Project
Date: April 08, 2026
"""

import pandas as pd
import numpy as np
from pathlib import Path
from typing import List, Dict, Any
import logging

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


class FlightEventLabeler:
    """
    Rule-based flight event labeling system.
    
    Assigns flight phase and event labels based on telemetry parameters
    using predefined thresholds and rules.
    """
    
    def __init__(self):
        """Initialize labeling thresholds and rules."""
        # Altitude thresholds (in feet)
        self.LOW_ALTITUDE_THRESHOLD = 1000
        self.HIGH_ALTITUDE_THRESHOLD = 10000
        self.CRUISE_MIN_ALTITUDE = 3000
        
        # Velocity thresholds (in knots)
        self.LOW_SPEED_THRESHOLD = 60
        self.HIGH_SPEED_THRESHOLD = 200
        self.TAXI_MAX_SPEED = 30
        self.TAKEOFF_MIN_SPEED = 50
        self.TAKEOFF_MAX_SPEED = 100
        
        # Vertical speed thresholds (in feet per minute)
        self.CLIMB_THRESHOLD = 500
        self.DESCENT_THRESHOLD = -300
        self.TAKEOFF_CLIMB_THRESHOLD = 300
        
        # Heading change thresholds (in degrees per second or rate of change)
        self.TURN_HEADING_THRESHOLD = 3
        
        # Roll thresholds (in degrees)
        self.TURN_ROLL_THRESHOLD = 5
        self.LEFT_TURN_THRESHOLD = -5
        self.RIGHT_TURN_THRESHOLD = 5
        
        # G-force thresholds
        self.HIGH_G_THRESHOLD = 1.5
        self.NORMAL_G_MIN = 0.8
        self.NORMAL_G_MAX = 1.2
        
        logger.info("FlightEventLabeler initialized with rule thresholds")
    
    def label_row(self, row: pd.Series, prev_row: pd.Series = None) -> str:
        """
        Apply rule-based labeling to a single data row.
        
        Priority order:
        1. TAXI - Ground operations
        2. TAKEOFF - Transition from ground to air
        3. LANDING - Final approach and touchdown
        4. APPROACH - Descending for landing
        5. CRUISE - Steady flight at altitude
        6. HIGH_G_FORCE - Exceptional g-forces
        7. TURN_LEFT / TURN_RIGHT - Turning maneuvers
        8. HIGH_SPEED / LOW_SPEED - Speed-based events
        9. HIGH_ALTITUDE / LOW_ALTITUDE - Altitude-based events
        10. NORMAL_FLIGHT - Default steady flight
        
        Args:
            row: Current data row with telemetry parameters
            prev_row: Previous row for calculating rates of change (optional)
        
        Returns:
            Event label as string
        """
        altitude = row.get('altitude', 0)
        velocity = row.get('velocity', 0)
        vertical_speed = row.get('vertical_speed', 0)
        heading = row.get('heading', 0)
        roll = row.get('roll', 0)
        g_force = row.get('g_force', 1.0)
        
        # Calculate heading change if previous row exists
        heading_change = 0
        if prev_row is not None and 'heading' in prev_row:
            heading_change = self._calculate_heading_change(
                prev_row['heading'], 
                heading
            )
        
        # Priority 1: TAXI - Low altitude, low speed
        if altitude < 50 and velocity < self.TAXI_MAX_SPEED:
            return 'TAXI'
        
        # Priority 2: TAKEOFF - Low altitude, increasing speed, climbing
        if (altitude < self.LOW_ALTITUDE_THRESHOLD and 
            velocity >= self.TAKEOFF_MIN_SPEED and 
            velocity <= self.TAKEOFF_MAX_SPEED and
            vertical_speed > self.TAKEOFF_CLIMB_THRESHOLD):
            return 'TAKEOFF'
        
        # Priority 3: LANDING - Low altitude, descending, moderate speed
        if (altitude < 500 and 
            vertical_speed < -100 and 
            velocity > self.TAXI_MAX_SPEED):
            return 'LANDING'
        
        # Priority 4: APPROACH - Descending from altitude
        if (altitude < self.CRUISE_MIN_ALTITUDE and 
            altitude > 500 and
            vertical_speed < self.DESCENT_THRESHOLD):
            return 'APPROACH'
        
        # Priority 5: CRUISE - Steady flight at altitude
        if (altitude >= self.CRUISE_MIN_ALTITUDE and 
            abs(vertical_speed) < 300 and
            velocity > self.LOW_SPEED_THRESHOLD):
            return 'CRUISE'
        
        # Priority 6: HIGH_G_FORCE - Exceptional g-forces
        if g_force > self.HIGH_G_THRESHOLD:
            return 'HIGH_G_FORCE'
        
        # Priority 7: TURN_LEFT - Left banking or heading change
        if roll < self.LEFT_TURN_THRESHOLD or heading_change < -self.TURN_HEADING_THRESHOLD:
            return 'TURN_LEFT'
        
        # Priority 8: TURN_RIGHT - Right banking or heading change
        if roll > self.RIGHT_TURN_THRESHOLD or heading_change > self.TURN_HEADING_THRESHOLD:
            return 'TURN_RIGHT'
        
        # Priority 9: Speed-based labels
        if velocity > self.HIGH_SPEED_THRESHOLD:
            return 'HIGH_SPEED'
        
        if velocity < self.LOW_SPEED_THRESHOLD and altitude > 100:
            return 'LOW_SPEED'
        
        # Priority 10: Altitude-based labels
        if altitude > self.HIGH_ALTITUDE_THRESHOLD:
            return 'HIGH_ALTITUDE'
        
        if altitude < self.LOW_ALTITUDE_THRESHOLD and altitude > 100:
            return 'LOW_ALTITUDE'
        
        # Default: NORMAL_FLIGHT
        return 'NORMAL_FLIGHT'
    
    def _calculate_heading_change(self, prev_heading: float, curr_heading: float) -> float:
        """
        Calculate heading change accounting for 360-degree wraparound.
        
        Args:
            prev_heading: Previous heading in degrees
            curr_heading: Current heading in degrees
        
        Returns:
            Heading change in degrees (-180 to 180)
        """
        change = curr_heading - prev_heading
        
        # Normalize to -180 to 180 range
        if change > 180:
            change -= 360
        elif change < -180:
            change += 360
        
        return change
    
    def label_dataset(self, input_path: Path, output_path: Path) -> pd.DataFrame:
        """
        Process entire dataset and apply labels to each row.
        
        Args:
            input_path: Path to input CSV file
            output_path: Path to output CSV file
        
        Returns:
            Labeled DataFrame
        """
        logger.info(f"Reading input data from: {input_path}")
        
        try:
            df = pd.read_csv(input_path)
        except FileNotFoundError:
            logger.error(f"Input file not found: {input_path}")
            raise
        except Exception as e:
            logger.error(f"Error reading input file: {e}")
            raise
        
        logger.info(f"Loaded {len(df)} rows from input file")
        
        # Validate required columns
        required_columns = ['altitude', 'velocity', 'vertical_speed', 
                          'heading', 'roll', 'g_force']
        missing_columns = [col for col in required_columns if col not in df.columns]
        
        if missing_columns:
            logger.error(f"Missing required columns: {missing_columns}")
            raise ValueError(f"Missing columns: {missing_columns}")
        
        # Apply labeling row by row
        logger.info("Applying rule-based labels...")
        labels = []
        
        for idx in range(len(df)):
            prev_row = df.iloc[idx - 1] if idx > 0 else None
            curr_row = df.iloc[idx]
            label = self.label_row(curr_row, prev_row)
            labels.append(label)
        
        # Add event_label column
        df['event_label'] = labels
        
        # Validate labels
        self._validate_labels(df)
        
        # Save labeled dataset
        logger.info(f"Saving labeled data to: {output_path}")
        df.to_csv(output_path, index=False)
        
        # Generate summary statistics
        self._print_label_statistics(df)
        
        logger.info("Labeling complete!")
        return df
    
    def _validate_labels(self, df: pd.DataFrame) -> None:
        """
        Validate that all rows have valid labels.
        
        Args:
            df: Labeled DataFrame
        
        Raises:
            ValueError if validation fails
        """
        logger.info("Validating labels...")
        
        # Check for missing labels
        missing_count = df['event_label'].isna().sum()
        if missing_count > 0:
            logger.error(f"Found {missing_count} rows with missing labels")
            raise ValueError(f"{missing_count} rows have missing labels")
        
        # Check for empty string labels
        empty_count = (df['event_label'] == '').sum()
        if empty_count > 0:
            logger.error(f"Found {empty_count} rows with empty labels")
            raise ValueError(f"{empty_count} rows have empty labels")
        
        # Verify all labels are from expected set
        expected_labels = {
            'TAXI', 'TAKEOFF', 'CRUISE', 'APPROACH', 'LANDING',
            'TURN_LEFT', 'TURN_RIGHT', 'HIGH_SPEED', 'LOW_SPEED',
            'HIGH_ALTITUDE', 'LOW_ALTITUDE', 'HIGH_G_FORCE', 'NORMAL_FLIGHT'
        }
        
        actual_labels = set(df['event_label'].unique())
        unexpected_labels = actual_labels - expected_labels
        
        if unexpected_labels:
            logger.warning(f"Found unexpected labels: {unexpected_labels}")
        
        logger.info("✓ All rows have valid labels")
    
    def _print_label_statistics(self, df: pd.DataFrame) -> None:
        """
        Print summary statistics about label distribution.
        
        Args:
            df: Labeled DataFrame
        """
        logger.info("\n" + "="*60)
        logger.info("LABEL DISTRIBUTION SUMMARY")
        logger.info("="*60)
        
        label_counts = df['event_label'].value_counts().sort_index()
        total_rows = len(df)
        
        for label, count in label_counts.items():
            percentage = (count / total_rows) * 100
            logger.info(f"{label:20s}: {count:6d} rows ({percentage:5.2f}%)")
        
        logger.info("="*60)
        logger.info(f"Total rows: {total_rows}")
        logger.info("="*60 + "\n")


def main():
    """Main execution function."""
    # Define paths relative to inference folder
    inference_dir = Path(__file__).parent
    input_file = inference_dir / 'Data' / 'raw_flight_data.csv'  # ← Updated
    output_file = inference_dir / 'Data' / 'labeled_flight_data.csv'  # ← Updated
    
    logger.info("="*60)
    logger.info("Flight Event Labeling System")
    logger.info("="*60)
    
    # Create labeler instance
    labeler = FlightEventLabeler()
    
    # Process dataset
    try:
        labeled_df = labeler.label_dataset(input_file, output_file)
        
        # Display sample labeled rows
        logger.info("\nSample labeled rows (first 10):")
        print("\n" + labeled_df.head(10).to_string())
        
        logger.info("\nSample labeled rows (last 10):")
        print("\n" + labeled_df.tail(10).to_string())
        
    except Exception as e:
        logger.error(f"Labeling failed: {e}")
        raise
    
    logger.info("\n✓ Labeling process completed successfully")


if __name__ == '__main__':
    main()