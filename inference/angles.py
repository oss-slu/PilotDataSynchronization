"""
Angle helpers shared by heading deviation and heading-change labeling.

Python only: the formula relies on floored modulo (numpy.mod and Python's %),
which differs from C++ and Rust, so do not port it to the plugin or the relay
as written. See docs/telemetry_schema.md section 5.
"""

import numpy as np


def angular_diff(actual, target):
    """
    Wrapped angular difference actual - target, in degrees.

    Works on scalars and numpy arrays. The result lies in [-180, 180), so an
    exact +180 difference maps to -180. NaN inputs give NaN.

    Examples:
        angular_diff(5, 355) == 10
        angular_diff(355, 5) == -10
    """
    wrapped = np.mod(np.asarray(actual) - np.asarray(target) + 180.0, 360.0) - 180.0
    # np.mod can round a tiny negative remainder up to 360, which lands on +180.
    return np.where(wrapped >= 180.0, -180.0, wrapped)[()]
