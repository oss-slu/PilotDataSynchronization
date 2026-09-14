# Pilot Data Synchronization

Captures a Pilot's simulated flight telemetry from X-Plane, classifies it into Flight Events for
ML analysis, and — once the client's requirements are fully met — will measure Deviation from
Designated Values for a pilot-performance clustering study.

## Language

**Pilot**:
The person operating the simulated aircraft during a Flight. Identified by `pilot_id`, a field the
telemetry stream doesn't capture yet.
_Avoid_: User, subject

**Flight**:
One continuous simulated flying session by a single Pilot, producing a stream of Telemetry
Samples. Identified by `flight_id`, a field the telemetry stream doesn't capture yet.
_Avoid_: Session, run

**Telemetry Sample**:
One row of instrument readings — altitude, heading, vertical speed, velocity, roll, pitch, yaw,
g-force — captured at a point during a Flight.
_Avoid_: Reading, data point, row

**Flight Event**:
One of 13 rule-based labels (TAXI, TAKEOFF, CRUISE, APPROACH, LANDING, TURN_LEFT, TURN_RIGHT,
HIGH_SPEED, LOW_SPEED, HIGH_ALTITUDE, LOW_ALTITUDE, HIGH_G_FORCE, NORMAL_FLIGHT) assigned to a
Telemetry Sample from its instrument values. Feeds the ML classification pipeline; unrelated to
Deviation — the two are separate analyses over the same Telemetry Samples, and nothing in the
codebase currently connects them.
_Avoid_: Label, category

**Flight-Dynamics Factor**:
One of the four Telemetry Sample fields the client evaluates Pilot performance against: heading,
vertical speed, altitude, airspeed. Narrower than a full Telemetry Sample — roll, pitch, yaw, and
g-force are captured but are not Flight-Dynamics Factors.
_Avoid_: Metric, parameter

**Designated Value**:
The target value for a Flight-Dynamics Factor that a Pilot's actual reading is measured against.
Appears in code/schema as `target_*`. **Open question, unresolved with the client**: whether this
is an instructor-assigned scenario target or an autopilot-commanded value. No Designated Value is
currently captured anywhere in the system, so Deviation cannot yet be computed from real data.
_Avoid_: Goal, reference value

**Deviation**:
The signed difference between a Pilot's actual reading for a Flight-Dynamics Factor and its
Designated Value (`actual − designated`). Heading uses a wrapped angular difference; the other
three factors use plain subtraction. Cannot be computed until Designated Value is resolved.
_Avoid_: Error, delta

**Certification Level**:
A Pilot's licensing qualification (e.g. Student, Private, Commercial, ATP). Exact category set
still TBD with the client. Distinct from Pilot Rating.
_Avoid_: Rating, qualification

**Pilot Rating**:
An evaluative performance score for a Pilot, distinct from Certification Level (resolved
2026-09-14 — previously ambiguous between the two).
_Avoid_: Certification, license
