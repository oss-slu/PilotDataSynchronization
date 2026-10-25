# Client meeting notes: Q&A and simulator visit

Source: transcript of a meeting with the client (Dr. Tamilselvan) in the simulator lab, with a walkthrough of the Prepar3D machine. The meeting date is not recorded in the transcript. The transcript is auto-generated and partly garbled, so anything unclear is marked **unconfirmed**. The earlier kickoff notes are referenced from [telemetry_schema.md](telemetry_schema.md).

Vocabulary follows [GLOSSARY.md](../GLOSSARY.md).

## Where the data comes from today

- The client's existing data was collected in **Prepar3D**, not X-Plane. Prepar3D is closed software, so it cannot be reconfigured; the client's own plugin extracts data from it.
- The Prepar3D export is TSV, and the export rate is one sample per second. A sample rate of roughly 90 to 93 Hz was also mentioned for the simulator. **Unconfirmed**, the two statements conflict.
- The client has 44 pilots. We received a de-identified sample.
- Two pilots in the full set have missing data, and the client will find out why. **Which two is unconfirmed**: the transcript says "six and seven" and "04 and 07", but P004 and P007 are in the sample we received (see #228), so those numbers are probably misheard.
- Every pilot flew one scenario, between one and six trials each. More scenarios and routes are planned.
- The client already segmented each flight into 7 phases (takeoff through landing) and will share the segmentation.
- X-Plane is not installed on the lab machine yet. The client has no date and will give an update the following week. Development proceeds on Prepar3D data in the meantime.
- The simulator machine runs Windows only.

## Scenario and what counts as on target

- The scenario's headings: climbs on heading **190 and 40**, to **3100 ft**; approach legs on heading **220 and 0**.
- Judge every leg on all four **Flight dynamics factors**: altitude, airspeed, heading and vertical speed. Approaches are judged on all four too, not on glide path or touchdown point alone.
- The client describes **Performance** as the gap between asked-for and achieved values, for example a pilot who climbs to 3000 ft when asked for 3100 ft.
- Route headings are **true headings**. The plugin currently sends **magnetic** heading (`heading_AHARS_deg_mag_pilot`). The mismatch matters once **Targets** exist, since the two differ by the magnetic variation. **Needs resolving before computing heading Deviation.**
- Units: feet, knots and feet per minute.

## Flights

- Time on the ground does not matter. Measure each **Flight** from takeoff to the final landing and leave out the hour or more spent on the ground.
- Stopping and restarting a recording partway through a flight is still **one Flight**. Join the parts by timestamp.
- A full scenario run is about 40 minutes.

## Targets

- Pilots receive **standardized instructions** during the flight (for example "change heading to 20"). The instructions are timestamped in iMotions, and the eye tracker records audio and video.
- This partly answers the **Target** source question: targets would be instructor-assigned, not taken from the autopilot. We have none of the timestamps yet, and the client may not be able to share iMotions data at first.
- Without instruction timestamps the pipeline cannot tell a late turn from an early one. A later goal is a model that detects the instruction and segments automatically. Until then, use the client's manual segmentation.

## Pilot metadata

- **Pilot rating** means the type of certification, not a score. The client will send certification level, total flight hours and rating for the pilots in the sample.
- For performance groups, the client prioritizes **experience (total flight hours) over certification level**.
- The client agreed to metadata being used **at the end**, to interpret groups and not to form them. This confirms [ADR 0001](adr/0001-metadata-checks-clusters-not-shapes-them.md).
- The client did clustering by hand last year and wants it automated.

## Clustering and where it runs

- The goal is real-time clustering. It is not expected to be computationally heavy.
- It should run on the **iMotions computer**, which already shows the pilots' views, and not on the simulator machine.
- Unresolved: where the result is shown (we assume not to the pilot, since it could add stress), and how often to cluster.
- The sensors run at different rates: simulator about 90 Hz, EEG 256 Hz, EDA 128 Hz and eye tracking 51 Hz (as transcribed, **unconfirmed**). The client is not sure there is a common rate to cluster at. iMotions synchronizes the timestamps.
- Later work combines flight data with the three physiological streams (eye tracking, EDA, EEG). These are kept in separate files, separate from demographics. Flight data alone is the starting point.

## Data handling

- The data is covered by an IRB protocol. Only people named on the protocol may access the full data. Copies may not be made or published.
- The sample the client shared is de-identified, and use in this class project is fine. Publishing results in a journal or conference is allowed.
- The repository is public, so **never commit raw client data**.

## Network port

- The X-Plane plugin sent to a placeholder port in the code. A new port was set, but it is **still unverified** that it matches the port iMotions listens on. The client, who handles the iMotions machine, will check. See [README.md](../README.md) for the open question.
  - Answered since this meeting: iMotions listens on 5050, read off the machine under Global Settings, API, Event Receiving. See the comment on #195.
- Whether developers get administrator rights on the simulator machine was not settled.

## Open items

| Item | Owner |
| --- | --- |
| Confirm which port iMotions listens on | Client |
| Share the 7-phase segmentation and the scenario briefing | Client |
| Send certification level, hours and rating per pilot | Client |
| Find out which pilots have missing data, and why | Client |
| X-Plane installation date | Client |
| Decide magnetic vs. true heading for heading **Targets** | Team |
| Decide where clustering results are shown, and how often to cluster | Team and client |
| Whether the team continues next semester | Unknown |
