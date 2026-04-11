"""
Generate balanced synthetic flight data for testing ML pipeline
"""

import pandas as pd
import numpy as np
from pathlib import Path

def generate_balanced_dataset(samples_per_class: int = 100):
    """
    Generate equal samples for each flight event class.
    
    Args:
        samples_per_class: Number of samples per event label
    """
    data = []
    
    # 1. TAXI events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T10:00:{i:02d}',
            'altitude': np.random.uniform(0, 30),
            'heading': np.random.uniform(0, 360),
            'vertical_speed': 0,
            'velocity': np.random.uniform(0, 25),
            'roll': np.random.uniform(-2, 2),
            'pitch': np.random.uniform(-2, 2),
            'yaw': np.random.uniform(0, 360),
            'g_force': 1.0
        })
    
    # 2. TAKEOFF events
    for i in range(samples_per_class):
        progress = i / samples_per_class
        data.append({
            'timestamp': f'2026-04-08T10:10:{i:02d}',
            'altitude': progress * 900,
            'heading': 90 + np.random.uniform(-5, 5),
            'vertical_speed': 600 + np.random.uniform(-100, 100),
            'velocity': 50 + progress * 50,
            'roll': np.random.uniform(-3, 3),
            'pitch': 5 + progress * 5,
            'yaw': 90,
            'g_force': 1.1
        })
    
    # 3. CRUISE events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T10:20:{i:02d}',
            'altitude': 5000 + np.random.uniform(-100, 100),
            'heading': 90 + np.random.uniform(-2, 2),
            'vertical_speed': np.random.uniform(-100, 100),
            'velocity': 180 + np.random.uniform(-20, 20),
            'roll': np.random.uniform(-3, 3),
            'pitch': np.random.uniform(-2, 2),
            'yaw': 90,
            'g_force': 1.0
        })
    
    # 4. APPROACH events
    for i in range(samples_per_class):
        progress = i / samples_per_class
        data.append({
            'timestamp': f'2026-04-08T10:30:{i:02d}',
            'altitude': 2000 - progress * 1500,
            'heading': 135 + np.random.uniform(-5, 5),
            'vertical_speed': -400 + np.random.uniform(-50, 50),
            'velocity': 120 - progress * 30,
            'roll': np.random.uniform(-3, 3),
            'pitch': -3 - progress * 2,
            'yaw': 135,
            'g_force': 1.0
        })
    
    # 5. LANDING events
    for i in range(samples_per_class):
        progress = i / samples_per_class
        data.append({
            'timestamp': f'2026-04-08T10:40:{i:02d}',
            'altitude': 400 - progress * 400,
            'heading': 135 + np.random.uniform(-2, 2),
            'vertical_speed': -300 - progress * 100,
            'velocity': max(80 - progress * 60, 30),
            'roll': np.random.uniform(-2, 2),
            'pitch': -5,
            'yaw': 135,
            'g_force': 1.2 if progress > 0.9 else 1.0
        })
    
    # 6. TURN_LEFT events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T10:50:{i:02d}',
            'altitude': 3000 + np.random.uniform(-50, 50),
            'heading': (90 - i * 2) % 360,
            'vertical_speed': np.random.uniform(-50, 50),
            'velocity': 150 + np.random.uniform(-10, 10),
            'roll': -15 - np.random.uniform(0, 10),
            'pitch': np.random.uniform(-2, 2),
            'yaw': 90,
            'g_force': 1.2
        })
    
    # 7. TURN_RIGHT events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T11:00:{i:02d}',
            'altitude': 3000 + np.random.uniform(-50, 50),
            'heading': (90 + i * 2) % 360,
            'vertical_speed': np.random.uniform(-50, 50),
            'velocity': 150 + np.random.uniform(-10, 10),
            'roll': 15 + np.random.uniform(0, 10),
            'pitch': np.random.uniform(-2, 2),
            'yaw': 90,
            'g_force': 1.2
        })
    
    # 8. HIGH_SPEED events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T11:10:{i:02d}',
            'altitude': 4000 + np.random.uniform(-200, 200),
            'heading': 90 + np.random.uniform(-5, 5),
            'vertical_speed': np.random.uniform(-100, 100),
            'velocity': 210 + np.random.uniform(0, 50),
            'roll': np.random.uniform(-2, 2),
            'pitch': np.random.uniform(-2, 2),
            'yaw': 90,
            'g_force': 1.0
        })
    
    # 9. LOW_SPEED events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T11:20:{i:02d}',
            'altitude': 500 + np.random.uniform(-50, 50),
            'heading': 135 + np.random.uniform(-5, 5),
            'vertical_speed': np.random.uniform(-200, -50),
            'velocity': 50 + np.random.uniform(-10, 5),
            'roll': np.random.uniform(-3, 3),
            'pitch': -3,
            'yaw': 135,
            'g_force': 1.0
        })
    
    # 10. HIGH_ALTITUDE events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T11:30:{i:02d}',
            'altitude': 12000 + np.random.uniform(-500, 500),
            'heading': 90 + np.random.uniform(-2, 2),
            'vertical_speed': np.random.uniform(-100, 100),
            'velocity': 200 + np.random.uniform(-20, 20),
            'roll': np.random.uniform(-2, 2),
            'pitch': np.random.uniform(-1, 1),
            'yaw': 90,
            'g_force': 1.0
        })
    
    # 11. LOW_ALTITUDE events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T11:40:{i:02d}',
            'altitude': 200 + np.random.uniform(-50, 50),
            'heading': np.random.uniform(0, 360),
            'vertical_speed': np.random.uniform(-200, 200),
            'velocity': 100 + np.random.uniform(-20, 20),
            'roll': np.random.uniform(-5, 5),
            'pitch': np.random.uniform(-3, 3),
            'yaw': np.random.uniform(0, 360),
            'g_force': 1.0
        })
    
    # 12. HIGH_G_FORCE events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T11:50:{i:02d}',
            'altitude': 2000 + np.random.uniform(-200, 200),
            'heading': np.random.uniform(0, 360),
            'vertical_speed': np.random.uniform(-500, 500),
            'velocity': 150 + np.random.uniform(-30, 30),
            'roll': np.random.uniform(-30, 30),
            'pitch': np.random.uniform(-15, 15),
            'yaw': np.random.uniform(0, 360),
            'g_force': 1.6 + np.random.uniform(0, 0.5)
        })
    
    # 13. NORMAL_FLIGHT events
    for i in range(samples_per_class):
        data.append({
            'timestamp': f'2026-04-08T12:00:{i:02d}',
            'altitude': 1500 + np.random.uniform(-100, 100),
            'heading': 90 + np.random.uniform(-3, 3),
            'vertical_speed': np.random.uniform(-200, 200),
            'velocity': 120 + np.random.uniform(-15, 15),
            'roll': np.random.uniform(-4, 4),
            'pitch': np.random.uniform(-3, 3),
            'yaw': 90,
            'g_force': 1.0
        })
    
    # Create DataFrame and shuffle
    df = pd.DataFrame(data)
    df = df.sample(frac=1).reset_index(drop=True)  # Shuffle
    
    return df


def main():
    inference_dir = Path(__file__).parent
    output_file = inference_dir / 'Data' / 'raw_flight_data.csv'
    
    print("Generating balanced synthetic flight data...")
    df = generate_balanced_dataset(samples_per_class=100)
    
    df.to_csv(output_file, index=False)
    print(f"✓ Generated {len(df)} samples (100 per class)")
    print(f"✓ Saved to: {output_file}")
    print("\nNow run: py label_generator.py")


if __name__ == '__main__':
    main()