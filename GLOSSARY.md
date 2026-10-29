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

**Mirror**:
A second destination the **Relay** copies every packet to, so the Python side can read a flight while iMotions records it. The copy is best effort: packets are dropped if the mirror falls behind, and the iMotions stream is never held up for it (`docs/adr/0003`).
_Avoid_: Python stream, fan-out, secondary connection, sink

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

**Performance group**:
A set of pilots whose **Deviation features** resemble each other. Groups are not ranked.
_Avoid_: Tier (a **Synthetic pilot**'s known level is its **Hidden tier**, a different thing), skill level, class

**Segment**:
One of the seven phases of a **Flight**, from takeoff through landing.
_Avoid_: Phase, section, leg (the five-value **Flight phase** in the out-of-scope section and a **Leg** are different things)

**Leg**:
A stretch of the route flown on one heading, such as the 190° climb or the 220° approach.
_Avoid_: Segment (the client's seven phases), phase

**Instruction**:
The timestamped moment a pilot is told a new **Target**, such as "change heading to 20". Instructions are standardized across pilots.
_Avoid_: Command, prompt, target change

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

### Synthetic pilots

**Synthetic pilot**:
A generated pilot with generated flights, built to deviate by a known amount, used to check that **Performance groups** find real differences before real pilots are available.
_Avoid_: Test pilot, fake pilot, mock pilot

**Hidden tier**:
The known deviation level a **Synthetic pilot** is generated at, kept apart from its telemetry so **Performance groups** can be compared against it. Real pilots have none.
_Avoid_: True group, skill level

### Out of scope: event classification

The client has not asked for these terms; they are kept apart from the model above.

**Flight event label**:
One of the 13 classes assigned to a **Sample**, naming what the aircraft is doing at that moment.
_Avoid_: Class, tag, category

**Flight phase**:
A **Flight event label** that names a stage of a flight: taxi, takeoff, cruise, approach or landing. There are five, and they are not the client's seven **Segments**.
_Avoid_: Stage, segment

**Synthetic data**:
Generated telemetry, balanced across **Flight event labels**, used when real flights are not available.
_Avoid_: Fake data, dummy data, mock data

## Relationships

- A **Pilot** flies many **Flights**; a **Flight** produces many **Samples**.
- A **Sample** has one value per telemetry field, and one **Deviation** per **Flight dynamics factor** once a **Target** exists.
- A **Flight** is divided into **Segments**, and is also flown as a sequence of **Legs**. An **Instruction** sets a new **Target** from that moment on.
- A **Pilot** falls into exactly one **Performance group**, decided by their **Deviation features** alone.
- **Pilot metadata** is compared against **Performance groups** after they are formed; it does not form them.
- Out of scope: a **Sample** receives exactly one **Flight event label**, and a **Flight phase** is a kind of **Flight event label**.

## Flagged ambiguities

- **Pilot rating** is the type of certification, not a performance score (client, see `docs/client_meeting_notes.md`). Whether it means the same as certification level, or an extra rating such as instrument, is still unclear.
- **Target** source is the **Instruction**: the client gives standardized, timestamped instructions during the flight, and iMotions holds the timestamps. None have been shared and no targets are captured yet, and the heading reference (true vs. magnetic) is unresolved.
- **Leg** and **Segment** may or may not line up. The client's seven **Segments** are unknown until she shares them, so how they map to our **Legs** is open.
- **Segment** names and boundaries are unknown until the client shares her seven-phase segmentation, which she marks by hand from the iMotions audio and video. Detecting **Instructions** automatically, for example from the eye tracker's audio, is later work; today the client segments by hand.
