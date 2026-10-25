"""Where inputs and outputs live, relative to the inference/ project folder."""

from pathlib import Path

INFERENCE_DIR = Path(__file__).resolve().parents[1]
DATA_DIR = INFERENCE_DIR / "data"
PREPARED_DIR = DATA_DIR / "prepared"
MODELS_DIR = INFERENCE_DIR / "models"
