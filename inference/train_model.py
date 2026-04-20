"""

Outputs:
    - Trained model files: models/bestModel.pkl & models/finalModel.pkl
    - Evaluation metrics: accuracy, precision, recall
"""

import pandas as pd
import numpy as np
from pathlib import Path
from sklearn.ensemble import RandomForestClassifier
from sklearn.model_selection import train_test_split
from sklearn.metrics import accuracy_score, precision_score, recall_score, classification_report, confusion_matrix
import joblib
import logging

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


class FlightEventModelTrainer:
    """
    Random Forest model trainer for flight event prediction.
    """
    
    def __init__(self, random_state: int = 42):
        """
        Initialize the model trainer.
        
        Args:
            random_state: Random seed for reproducibility
        """
        self.random_state = random_state
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
        
    def load_data(self, data_path: Path) -> pd.DataFrame:
        """
        Load labeled flight data.
        
        Args:
            data_path: Path to labeled CSV file
            
        Returns:
            DataFrame containing labeled flight data
        """
        logger.info(f"Loading data from: {data_path}")
        
        if not data_path.exists():
            raise FileNotFoundError(f"Data file not found: {data_path}")
        
        df = pd.read_csv(data_path)
        logger.info(f"Loaded {len(df)} samples")
        
        # Verify required columns exist
        missing_cols = [col for col in self.feature_columns + [self.target_column] if col not in df.columns]
        if missing_cols:
            raise ValueError(f"Missing required columns: {missing_cols}")
        
        # Display class distribution
        logger.info("\nClass distribution:")
        print(df[self.target_column].value_counts().to_string())
        
        return df
    
    def prepare_data(self, df: pd.DataFrame, test_size: float = 0.2):
        """
        Split data into training and testing sets.
        
        Args:
            df: Input DataFrame
            test_size: Proportion of data for testing (default 0.2)
            
        Returns:
            Tuple of (X_train, X_test, y_train, y_test)
        """
        logger.info(f"\nPreparing data with test size: {test_size}")
        
        # Extract features and target
        X = df[self.feature_columns]
        y = df[self.target_column]
        
        # Split into train and test sets
        X_train, X_test, y_train, y_test = train_test_split(
            X, y, 
            test_size=test_size, 
            random_state=self.random_state,
            stratify=y  # Maintain class distribution in splits
        )
        
        logger.info(f"Training set: {len(X_train)} samples")
        logger.info(f"Test set: {len(X_test)} samples")
        
        return X_train, X_test, y_train, y_test
    
    def train_model(self, X_train: pd.DataFrame, y_train: pd.Series):
        """
        Train Random Forest classifier.
        
        Args:
            X_train: Training features
            y_train: Training labels
        """
        logger.info("\n" + "="*60)
        logger.info("Training Random Forest Model")
        logger.info("="*60)
        
        # Initialize Random Forest with basic parameters
        self.model = RandomForestClassifier(
            n_estimators=100,       # Number of trees
            max_depth=None,         # No limit on tree depth
            min_samples_split=2,    # Minimum samples to split node
            min_samples_leaf=1,     # Minimum samples in leaf node
            random_state=self.random_state,
            n_jobs=-1,              # Use all available cores
            verbose=1               # Show training progress
        )
        
        # Train the model
        logger.info(f"\nTraining on {len(X_train)} samples...")
        self.model.fit(X_train, y_train)
        
        logger.info("✓ Model training completed")
        
        # Display feature importance
        self._display_feature_importance()
    
    def _display_feature_importance(self):
        """Display feature importance from trained model."""
        if self.model is None:
            return
        
        importance_df = pd.DataFrame({
            'Feature': self.feature_columns,
            'Importance': self.model.feature_importances_
        }).sort_values('Importance', ascending=False)
        
        logger.info("\nFeature Importance:")
        print("\n" + importance_df.to_string(index=False))
    
    def evaluate_model(self, X_test: pd.DataFrame, y_test: pd.Series) -> dict:
        """
        Evaluate model on test data.
        
        Args:
            X_test: Test features
            y_test: Test labels
            
        Returns:
            Dictionary containing evaluation metrics
        """
        logger.info("\n" + "="*60)
        logger.info("Model Evaluation")
        logger.info("="*60)
        
        # Generate predictions
        y_pred = self.model.predict(X_test)
        
        # Calculate metrics
        accuracy = accuracy_score(y_test, y_pred)
        precision = precision_score(y_test, y_pred, average='weighted', zero_division=0)
        recall = recall_score(y_test, y_pred, average='weighted', zero_division=0)
        
        metrics = {
            'accuracy': accuracy,
            'precision': precision,
            'recall': recall
        }
        
        # Display results
        logger.info("\nEvaluation Metrics:")
        logger.info(f"  Accuracy:  {accuracy:.4f} ({accuracy*100:.2f}%)")
        logger.info(f"  Precision: {precision:.4f} ({precision*100:.2f}%)")
        logger.info(f"  Recall:    {recall:.4f} ({recall*100:.2f}%)")
        
        # Display classification report
        logger.info("\nDetailed Classification Report:")
        print("\n" + classification_report(y_test, y_pred, zero_division=0))
        
        # Display confusion matrix
        logger.info("Confusion Matrix:")
        cm = confusion_matrix(y_test, y_pred)
        classes = sorted(y_test.unique())
        cm_df = pd.DataFrame(cm, index=classes, columns=classes)
        print("\n" + cm_df.to_string())
        
        return metrics
    
    def save_model(self, models_dir: Path):
        """
        Save trained model to disk.
        
        Args:
            models_dir: Directory to save model files
        """
        if self.model is None:
            raise ValueError("No trained model to save")
        
        # Create models directory if it doesn't exist
        models_dir.mkdir(parents=True, exist_ok=True)
        
        # Save as both bestModel.pkl and finalModel.pkl
        best_model_path = models_dir / 'bestModel.pkl'
        final_model_path = models_dir / 'finalModel.pkl'
        
        logger.info("\nSaving trained models:")
        
        joblib.dump(self.model, best_model_path)
        logger.info(f"  ✓ Saved: {best_model_path}")
        
        joblib.dump(self.model, final_model_path)
        logger.info(f"  ✓ Saved: {final_model_path}")


def main():
    """Main execution function."""
    # Define paths relative to inference folder
    inference_dir = Path(__file__).parent
    data_file = inference_dir / 'Data' / 'labeled_flight_data.csv'
    models_dir = inference_dir / 'models'
    
    logger.info("="*60)
    logger.info("Flight Event Prediction - Model Training")
    logger.info("="*60)
    logger.info(f"\nInput data: {data_file}")
    logger.info(f"Output directory: {models_dir}")
    
    try:
        # Initialize trainer
        trainer = FlightEventModelTrainer(random_state=42)
        
        # Load data
        df = trainer.load_data(data_file)
        
        # Prepare train/test split
        X_train, X_test, y_train, y_test = trainer.prepare_data(df, test_size=0.2)
        
        # Train model
        trainer.train_model(X_train, y_train)
        
        # Evaluate model
        metrics = trainer.evaluate_model(X_test, y_test)
        
        # Save model
        trainer.save_model(models_dir)
        
        logger.info("\n" + "="*60)
        logger.info("Training Complete!")
        logger.info("="*60)
        logger.info(f"\nFinal Metrics:")
        logger.info(f"  Accuracy:  {metrics['accuracy']:.4f}")
        logger.info(f"  Precision: {metrics['precision']:.4f}")
        logger.info(f"  Recall:    {metrics['recall']:.4f}")
        
    except Exception as e:
        logger.error(f"\nTraining failed: {e}")
        raise


if __name__ == '__main__':
    main()
