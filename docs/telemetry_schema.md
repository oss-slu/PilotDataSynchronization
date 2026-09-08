# Telemetry Data Schema & Performance Deviation Metrics

This document is the contract for pilot telemetry data used by every downstream
script (feature engineering, deviation calculation, clustering). It reflects what
the X-Plane plugin actually emits today, plus the fields the client
(Dr. Gajapriya) has asked for that we still need to add.

Source context: the "Client Requirements Summary — Dr. Gajapriya" meeting notes
(client kickoff meeting), the plugin implementation in
[xplane_plugin/src/pilotdatasync-xp11.cpp](../xplane_plugin/src/pilotdatasync-xp11.cpp),
and the DataRef reference in [xplane_plugin/docs/key_datarefs.md](../xplane_plugin/docs/key_datarefs.md).

## 1. What the plugin outputs today

The plugin reads pilot-side cockpit DataRefs at up to 20 Hz (throttled to one
send per 50ms) and streams them through Baton → Relay → `data_logger.py`, which
writes one CSV row per complete sample.

Current CSV output (`inference/Data/raw_flight_data.csv`):

| Field | Type | Units | Source (DataRef) | Notes |
|---|---|---|---|---|
| `timestamp` | ISO-8601 string (UTC, ms precision) | — | wall-clock time at CSV write, not sim time | Set by `data_logger.py`, not the plugin |
| `altitude` | float | feet, MSL | `sim/cockpit2/gauges/indicators/altitude_ft_pilot` | Pilot's barometric altimeter reading |
| `heading` | float | degrees magnetic | `sim/cockpit2/gauges/indicators/heading_AHARS_deg_mag_pilot` | AHARS-sourced |
| `vertical_speed` | float | feet/min | `sim/cockpit2/gauges/indicators/vvi_fpm_pilot` | Positive = climbing |
| `velocity` | float | knots | `sim/cockpit2/gauges/indicators/airspeed_kts_pilot` | Indicated airspeed, not true airspeed |
| `roll` | float | degrees | `sim/cockpit2/gauges/indicators/roll_AHARS_deg_pilot` | |
| `pitch` | float | degrees | `sim/cockpit2/gauges/indicators/pitch_AHARS_deg_pilot` | |
| `yaw` | float | degrees | `sim/flightmodel/position/psi` | No pilot-side yaw DataRef exists; flightmodel yaw is used as a fallback |
| `g_force` | float | G | `sim/flightmodel/forces/g_nrml` | Vertical (normal) component only; horizontal/side G is read by the plugin but not sent |

All values are pilot-side instrument readings (subject to simulated sensor
error), not the aircraft's raw physical state — see note 2 in
`key_datarefs.md`.

## 2. What the client asked for vs. what exists

The client's four flight-dynamics factors — **heading, vertical airspeed,
altitude, airspeed** — are covered by the fields above. `roll`, `pitch`,
`yaw`, and `g_force` are extra fields the plugin already emits but the client
did not ask for; keep them, since they're free and may help explain clustering
outliers, but they are not part of the deviation metric set.

## 3. Gap: no "designated" (target) values today

**This is the critical gap.** The client's performance metric is defined as
`deviation = actual − designated` (target), e.g. "plugin set to fly at
300,000 ft, actual reading is 290,000 ft." Right now:

- The plugin only reads and sends **actual** instrument values. There is no
  DataRef read, UI control, or CSV column for a *designated/target* value for
  altitude, heading, vertical speed, or airspeed.
- Nothing in `relay/`, `data_logger.py`, or the `inference/` scripts defines
  or stores a target value either.

To satisfy the client's requirement, the pipeline needs a source of target
values per flight (or per flight segment), one of:

1. **Instructor/scenario-defined targets** — a target altitude/heading/speed
   assigned for a given exercise or leg, entered before or during the flight
   (e.g. via the plugin UI or a scenario config file), OR
2. **Autopilot/FMS target DataRefs** — X-Plane exposes commanded values for
   some parameters (e.g. autopilot altitude/heading bugs) that could stand in
   for "designated" values when the autopilot is being used as the reference.

Until one of these is implemented, `target_*` columns in section 4 will be
unpopulated (`NaN`) and deviation cannot be computed from real data — only
from synthetic data where targets are generated. **Flag this to the client
directly:** ask whether "designated" means an instructor-assigned briefing
target, an autopilot bug setting, or something else, since the schema and
ingestion path differ depending on the answer.

## 4. Full schema (target state)

### 4a. Telemetry table (one row per sample)

| Field | Type | Units | Source | Status |
|---|---|---|---|---|
| `timestamp` | ISO-8601 string, UTC | — | data_logger.py | exists |
| `pilot_id` | string | — | pilot metadata join key | **missing — needs to be added to the send path** |
| `flight_id` | string | — | generated per session | **missing** |
| `altitude` | float | ft MSL | plugin | exists |
| `target_altitude` | float | ft MSL | scenario/autopilot (TBD, see §3) | **missing** |
| `altitude_deviation` | float | ft | computed | derived, see §5 |
| `heading` | float | deg magnetic | plugin | exists |
| `target_heading` | float | deg magnetic | scenario/autopilot (TBD) | **missing** |
| `heading_deviation` | float | deg | computed | derived, see §5 |
| `vertical_speed` | float | ft/min | plugin | exists |
| `target_vertical_speed` | float | ft/min | scenario/autopilot (TBD) | **missing** |
| `vertical_speed_deviation` | float | ft/min | computed | derived, see §5 |
| `velocity` (airspeed) | float | kts (indicated) | plugin | exists |
| `target_velocity` | float | kts | scenario/autopilot (TBD) | **missing** |
| `velocity_deviation` | float | kts | computed | derived, see §5 |
| `roll` | float | deg | plugin | exists (extra, not part of deviation set) |
| `pitch` | float | deg | plugin | exists (extra) |
| `yaw` | float | deg | plugin | exists (extra) |
| `g_force` | float | G | plugin | exists (extra) |

### 4b. Pilot metadata table (one row per pilot, joined on `pilot_id`)

None of these fields exist anywhere in the codebase today; they must be
collected separately (e.g. an intake form or a small CSV maintained
alongside the telemetry data) and joined in at feature-engineering time.

| Field | Type | Units | Notes |
|---|---|---|---|
| `pilot_id` | string | — | join key to telemetry table |
| `certification_level` | string (categorical) | — | e.g. Student, Private, Commercial, ATP — exact category set TBD with client |
| `flight_hours` | float | hours | total logged flight hours |
| `pilot_rating` | string or numeric | — | client mentioned "pilot rating" as distinct from certification; needs clarification on whether this is a licensing rating (e.g. instrument rating) or a performance score |

**Open question for the client:** clarify whether `pilot_rating` is a
licensing qualification (instrument, multi-engine, etc.) or an evaluative
score, since that changes its type and how it factors into clustering.

## 5. Deviation formulas

Deviation is defined consistently as **actual − designated (target)** for
each metric, per the client's example (actual 290,000 ft vs. target 300,000 ft
→ deviation = −10,000 ft).

```
altitude_deviation         = altitude - target_altitude              (ft)
heading_deviation          = angular_diff(heading, target_heading)   (deg, range [-180, 180])
vertical_speed_deviation   = vertical_speed - target_vertical_speed  (ft/min)
velocity_deviation         = velocity - target_velocity              (kts)
```

`heading_deviation` cannot use plain subtraction because heading wraps at
360°/0° (e.g. actual 5°, target 355° should deviate by +10°, not −350°).
Use a wrapped angular difference:

```
angular_diff(actual, target) = ((actual - target + 180) mod 360) - 180
```

For clustering, each deviation should also be available in absolute-value
and/or normalized (e.g. z-scored per pilot or per metric) form, since raw
units differ in scale (ft vs. deg vs. kts) — this will be finalized in the
feature-engineering step, not in this schema doc.

## 6. Summary of gaps to flag to the client

1. **No designated/target values are currently captured** for altitude,
   heading, vertical speed, or airspeed — the single largest gap, since the
   client's entire performance metric depends on it. Need a decision on
   source (instructor-assigned scenario target vs. autopilot bug).
2. **No pilot metadata pipeline exists** — certification level, flight
   hours, and pilot rating are not collected or stored anywhere in the repo.
3. **`pilot_rating` is ambiguous** — needs clarification from the client on
   whether it's a licensing rating or a performance score.
4. **No `pilot_id` / `flight_id` fields** in the current send path — needed
   to support "10-20 pilots" and multiple flights per pilot, and to join
   telemetry to pilot metadata.
5. **Timestamp is wall-clock, not simulation time** — fine for a single
   continuous session, but worth noting if flights are paused/resumed.
