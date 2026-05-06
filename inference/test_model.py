"""
Flight Event Prediction - Model Testing Script

This script tests trained Random Forest models for flight event prediction.
It loads saved models, runs inference on test data, and evaluates performance.

Outputs:
    - Prediction results: predictions_output.csv
    - Evaluation metrics: accuracy, precision, recall
"""

import pandas as pd
import numpy as np
from pathlib import Path
from sklearn.metrics import accuracy_score, precision_score, recall_score, classification_report, confusion_matrix
import joblib
import logging
from datetime import datetime

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


class FlightEventModelTester:
    """
    Tester for trained Random Forest flight event prediction models.
    """

    def __init__(self):
        """Initialize the model tester."""
        self.model = None
        self.feature_columns = [
            'altitude',
            'heading',
            'vertical_speed',
            'velocity',
            'roll',
            'pitch',
            'yaw',
            'g_force'
        ]
        self.target_column = 'event_label'

    def load_model(self, model_path: Path):
        """
        Load trained Random Forest model from disk.

        Args:
            model_path: Path to saved model (.pkl file)
        """
        logger.info(f"Loading model from: {model_path}")

        if not model_path.exists():
            raise FileNotFoundError(f"Model file not found: {model_path}")

        self.model = joblib.load(model_path)
        logger.info(f"✓ Model loaded successfully: {type(self.model).__name__}")

        # Display model parameters
        if hasattr(self.model, 'n_estimators'):
            logger.info(f"  - Number of estimators: {self.model.n_estimators}")
        if hasattr(self.model, 'max_depth'):
            logger.info(f"  - Max depth: {self.model.max_depth}")

    def load_test_data(self, data_path: Path) -> pd.DataFrame:
        """
        Load test dataset for inference.

        Args:
            data_path: Path to test data CSV file

        Returns:
            DataFrame containing test data
        """
        logger.info(f"\nLoading test data from: {data_path}")

        if not data_path.exists():
            raise FileNotFoundError(f"Test data file not found: {data_path}")

        df = pd.read_csv(data_path)
        logger.info(f"✓ Loaded {len(df)} test samples")

        # Verify required feature columns exist
        missing_cols = [col for col in self.feature_columns if col not in df.columns]
        if missing_cols:
            raise ValueError(f"Missing required feature columns: {missing_cols}")

        # Check if labels exist (for evaluation)
        has_labels = self.target_column in df.columns
        if has_labels:
            logger.info(f"✓ Test data contains ground truth labels")
            logger.info("\nActual label distribution:")
            print(df[self.target_column].value_counts().to_string())
        else:
            logger.warning("⚠ Test data does not contain labels (evaluation will be skipped)")

        return df

    def run_inference(self, df: pd.DataFrame) -> np.ndarray:
        """
        Run inference on test data to generate predictions.

        Args:
            df: DataFrame containing test features

        Returns:
            Array of predicted labels
        """
        if self.model is None:
            raise ValueError("No model loaded. Call load_model() first.")

        logger.info("\n" + "="*60)
        logger.info("Running Inference")
        logger.info("="*60)

        # Extract features
        X_test = df[self.feature_columns]

        logger.info(f"\nGenerating predictions for {len(X_test)} samples...")
        predictions = self.model.predict(X_test)

        logger.info("✓ Inference completed")
        logger.info(f"\nPredicted label distribution:")
        unique, counts = np.unique(predictions, return_counts=True)
        for label, count in zip(unique, counts):
            logger.info(f"  {label}: {count} ({count/len(predictions)*100:.2f}%)")

        return predictions

    def evaluate_predictions(self, y_true: pd.Series, y_pred: np.ndarray) -> dict:
        """
        Evaluate model predictions against ground truth labels.

        Args:
            y_true: Ground truth labels
            y_pred: Predicted labels

        Returns:
            Dictionary containing evaluation metrics
        """
        logger.info("\n" + "="*60)
        logger.info("Model Evaluation")
        logger.info("="*60)

        # Calculate metrics
        accuracy = accuracy_score(y_true, y_pred)
        precision = precision_score(y_true, y_pred, average='weighted', zero_division=0)
        recall = recall_score(y_true, y_pred, average='weighted', zero_division=0)

        metrics = {
            'accuracy': accuracy,
            'precision': precision,
            'recall': recall,
            'timestamp': datetime.now().isoformat()
        }

        # Display evaluation metrics
        logger.info("\n📊 Evaluation Metrics:")
        logger.info("="*60)
        logger.info(f"  Accuracy:  {accuracy:.4f} ({accuracy*100:.2f}%)")
        logger.info(f"  Precision: {precision:.4f} ({precision*100:.2f}%)")
        logger.info(f"  Recall:    {recall:.4f} ({recall*100:.2f}%)")
        logger.info("="*60)

        # Display detailed classification report
        logger.info("\n📋 Detailed Classification Report:")
        print("\n" + classification_report(y_true, y_pred, zero_division=0))

        # Display confusion matrix
        logger.info("🔢 Confusion Matrix:")
        cm = confusion_matrix(y_true, y_pred)
        classes = sorted(y_true.unique())

        # Create formatted confusion matrix
        cm_df = pd.DataFrame(cm, index=classes, columns=classes)
        cm_df.index.name = 'Actual'
        cm_df.columns.name = 'Predicted'
        print("\n" + cm_df.to_string())

        return metrics

    def save_predictions(self, df: pd.DataFrame, predictions: np.ndarray, output_path: Path):
        """
        Save prediction results to CSV file.

        Args:
            df: Original test DataFrame
            predictions: Predicted labels
            output_path: Path to save predictions
        """
        logger.info(f"\n💾 Saving predictions to: {output_path}")

        # Create output DataFrame with original data and predictions
        output_df = df.copy()
        output_df['predicted_event_label'] = predictions

        # Add prediction confidence if available (for Random Forest)
        if hasattr(self.model, 'predict_proba'):
            probabilities = self.model.predict_proba(df[self.feature_columns])
            max_probabilities = np.max(probabilities, axis=1)
            output_df['prediction_confidence'] = max_probabilities

        # Save to CSV
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_df.to_csv(output_path, index=False)

        logger.info(f"✓ Saved {len(output_df)} predictions")
        logger.info(f"  Columns: {', '.join(output_df.columns.tolist())}")

    def save_metrics(self, metrics: dict, output_path: Path):
        """
        Save evaluation metrics to JSON file.

        Args:
            metrics: Dictionary of evaluation metrics
            output_path: Path to save metrics
        """
        import json

        logger.info(f"\n💾 Saving metrics to: {output_path}")

        output_path.parent.mkdir(parents=True, exist_ok=True)

        with output_path.open('w') as f:
            json.dump(metrics, f, indent=2)

        logger.info("✓ Metrics saved successfully")


def main():
    """Main execution function for model testing."""
    # Define paths relative to inference folder
    inference_dir = Path(__file__).parent

    # Model paths (try both bestModel.pkl and finalModel.pkl)
    models_dir = inference_dir / 'Models'
    model_paths = [
        models_dir / 'bestModel.pkl',
        models_dir / 'finalModel.pkl'
    ]

    # Data paths (try multiple locations)
    data_candidates = [
        inference_dir / 'dataset' / 'test.csv',  # Processed test set
        inference_dir / 'Data' / 'labeled_flight_data.csv',  # Full labeled dataset
        inference_dir / 'labeled_flight_data.csv'  # Alternative location
    ]

    # Output paths
    output_predictions = inference_dir / 'predictions_output.csv'
    output_metrics = inference_dir / 'evaluation_metrics.json'

    logger.info("="*60)
    logger.info("Flight Event Prediction - Model Testing")
    logger.info("="*60)

    try:
        # Initialize tester
        tester = FlightEventModelTester()

        # Load model (try available model files)
        model_loaded = False
        for model_path in model_paths:
            if model_path.exists():
                tester.load_model(model_path)
                model_loaded = True
                break

        if not model_loaded:
            raise FileNotFoundError(
                f"No model files found. Searched: {', '.join(str(p) for p in model_paths)}\n"
                "Please train a model first using train_model.py"
            )

        # Load test data (try available data files)
        test_data = None
        data_path_used = None
        for data_path in data_candidates:
            if data_path.exists():
                test_data = tester.load_test_data(data_path)
                data_path_used = data_path
                break

        if test_data is None:
            raise FileNotFoundError(
                f"No test data found. Searched: {', '.join(str(p) for p in data_candidates)}\n"
                "Please ensure labeled or processed data is available."
            )

        # Run inference
        predictions = tester.run_inference(test_data)

        # Evaluate if ground truth labels are available
        metrics = None
        if tester.target_column in test_data.columns:
            y_true = test_data[tester.target_column]
            metrics = tester.evaluate_predictions(y_true, predictions)

            # Save metrics
            tester.save_metrics(metrics, output_metrics)
        else:
            logger.info("\n⚠ Skipping evaluation (no ground truth labels available)")

        # Save predictions
        tester.save_predictions(test_data, predictions, output_predictions)

        # Final summary
        logger.info("\n" + "="*60)
        logger.info("✅ Testing Complete!")
        logger.info("="*60)
        logger.info(f"\nModel used: {model_path}")
        logger.info(f"Test data: {data_path_used}")
        logger.info(f"Predictions saved: {output_predictions}")

        if metrics:
            logger.info(f"Metrics saved: {output_metrics}")
            logger.info(f"\n📊 Final Results:")
            logger.info(f"  Accuracy:  {metrics['accuracy']:.4f}")
            logger.info(f"  Precision: {metrics['precision']:.4f}")
            logger.info(f"  Recall:    {metrics['recall']:.4f}")

        logger.info("\n✓ All outputs saved in inference/ folder")

    except Exception as e:
        logger.error(f"\n❌ Testing failed: {e}")
        raise


if __name__ == '__main__':
    main()