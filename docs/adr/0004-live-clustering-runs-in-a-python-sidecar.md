# Live performance groups run in a Python sidecar, not in the relay

To update **Performance groups** while a **Flight** is under way, the relay forwards each **Sample** over TCP to a separate Python program, the sidecar. The sidecar keeps the live pilot's running **Deviation features** and assigns the pilot to **Performance groups** fitted once on a reference cohort, using the existing functions in `inference/pilot_inference/performance/features.py` and `clustering.py`. The relay embeds no Python or ML and only shows the result. We chose this so there is one implementation of the clustering, the one the evidence in docs/adr/0002 was measured on.

## Considered Options

- **Port clustering to Rust inside the relay.** Rejected. It would duplicate KMeans, the log/floor scaling, the standardization over the cohort and the k-sweep stability check. Two copies could drift apart, and the ADR 0002 evidence would only describe one of them. The relay would also need an ML dependency for a 10 to 20 pilot cohort.
- **Embed Python in the relay** (for example through PyO3). Rejected. It couples the relay's build and release to a Python runtime and scikit-learn, and it puts a slow refit on the process that must keep forwarding to iMotions.
- **Python sidecar (chosen).** The sidecar is a separate process started like the other inference tools. A slow update, or a sidecar that is down, cannot block the relay's path to iMotions.

## Consequences

- The sidecar reads the relay's **Mirror** (docs/adr/0003), so the relay already forwards to iMotions unchanged when the sidecar is absent or slow. The copy is best effort, so the sidecar must tolerate dropped samples and must keep reading its socket on a thread separate from the slower work, or it loses the mirror.
- The return channel (how the sidecar tells the relay a pilot's group) is not decided here.
- The groups are fitted once and the live pilot is assigned to the nearest one, rather than refitting with the live pilot included. A refit would renumber the groups between updates, and one live pilot cannot form groups alone (`candidate_ks` needs 6). Until real pilots have **Targets**, the reference cohort is **Synthetic pilots**, so the live group is a demonstration rather than a measurement.
- The client's **Targets** are not available (`docs/telemetry_schema.md` section 3). For our own X-Plane flights we are the instructor: we write a brief of **Instructions** with all four targets, and the operator marks each one live as it is spoken, so the sidecar knows when every target starts. Targets are left blank for a settle window after each instruction, so the turn or level-off is not scored, since **Synthetic pilots** fly only steady holds.
- If the sidecar is later judged too heavy to run beside the relay, this decision should be revisited, since porting to Rust then means re-validating the clustering against ADR 0002's evidence.
