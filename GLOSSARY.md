# Pilot Data Synchronization

Captures what a pilot does in the X-Plane simulator, streams it to iMotions, and builds a dataset used to measure and classify pilot performance.

## Language

### Capture

**Telemetry**:
The pilot-side instrument readings taken from the simulator while a flight is under way.
_Avoid_: Flight data, signal

**Sample**:
One complete set of telemetry values at a single moment, recorded as one row.
_Avoid_: Reading, record, tick

**Pilot-side reading**:
A value taken from the pilot's own instruments, so it carries simulated sensor error rather than the aircraft's true physical state.
_Avoid_: Ground truth, actual state

**Plugin**:
The component inside X-Plane that extracts telemetry.
_Avoid_: Extension, mod

**Relay**:
The external program that receives telemetry from the plugin and forwards it to iMotions.
_Avoid_: Bridge, proxy, server

**Baton**:
The library that carries telemetry between the plugin and the relay.
_Avoid_: IPC layer, transport

**Mock server**:
A stand-in for iMotions used when no real iMotions instance is available.
_Avoid_: Fake iMotions

### Performance measurement

**Airspeed**:
The pilot's indicated forward speed, in knots.
_Avoid_: Velocity, speed

**Target**:
The value a pilot is supposed to be holding for altitude, heading, vertical speed or airspeed.
_Avoid_: Designated value, setpoint, reference

**Deviation**:
Actual minus target for one metric. Heading deviation is the wrapped angular difference.
_Avoid_: Error, delta, offset

**Deviation features**:
Summaries of a pilot's **Deviation** over a **Flight**, per **Flight dynamics factor**: how large the deviation is on average and how steady it is. A pilot's deviation features are the average of their flights' deviation features, so every **Flight** counts equally.
_Avoid_: Performance score, error metrics

**Flight dynamics factors**:
The four metrics the client cares about: heading, vertical speed, altitude and airspeed.
_Avoid_: Core metrics, KPIs

### Pilots and flights

**Pilot**:
A person who flies in the simulator and whose performance is being measured.
_Avoid_: User, subject, participant

**Flight**:
One continuous simulator session by one pilot.
_Avoid_: Session, run, trip

**Pilot metadata**:
Facts about a pilot that are not telemetry: certification level, flight hours and pilot rating.
_Avoid_: Pilot profile, demographics

### Classification

**Flight event label**:
One of the 13 classes assigned to a sample, naming what the aircraft is doing at that moment.
_Avoid_: Class, tag, category

**Flight phase**:
A flight event label that names a stage of a flight: taxi, takeoff, cruise, approach or landing.
_Avoid_: Stage, segment

**Synthetic data**:
Generated telemetry, balanced across flight event labels, used when real flights are not available.
_Avoid_: Fake data, dummy data, mock data

## Relationships

- A **Pilot** flies many **Flights**; a **Flight** produces many **Samples**.
- A **Sample** has one value per telemetry field, and one **Deviation** per **Flight dynamics factor** once a **Target** exists.
- A **Sample** receives exactly one **Flight event label**; a **Flight phase** is a kind of flight event label.

## Flagged ambiguities

- **Pilot rating** is unresolved: the client may mean a licensing rating (instrument, multi-engine) or a performance score. Do not use it as a precise term until clarified.
- **Target** source is undecided: instructor-assigned, or taken from the autopilot. No targets are captured yet.
