# Clustering the client's Prepar3D flights, autumn 2026

> **Status: outline. No results yet.**
>
> Every results section below is empty on purpose. The client's Prepar3D
> flights are not in the repository: loading them is #228 and the per-Flight
> features are #270, both open. The model is #271, also open.
>
> No number in this document may be filled in from synthetic pilots. The
> evaluation harness (#272) runs on synthetic pilots today so that the tests
> themselves could be written and checked where the right answer is known,
> and that output belongs in `inference/results/`, not here. Putting it under
> this filename would read as a finding about the client's pilots.
>
> To fill this in once #270 lands:
>
> ```
> cd inference
> uv run python -m pilot_inference.performance.evaluate --flights <per-Flight features CSV>
> ```
>
> That writes every table and figure named below, plus `metrics.json`, to
> `inference/results/`.

## What this is

Pilots are grouped by how far they drift from their targets, not by who they
are. The grouping is unsupervised: there are no labels, so there is no
accuracy to quote. Three questions stand in for it.

1. Do the groups hold for a pilot the model never saw?
2. Do they survive small changes to the data and the settings?
3. Do they describe anything about how people fly?

## Data

*Pending #228 and #270.* To record here: number of pilots, number of Flights,
the date range, and every cleaning decision made in #270, including which
Flights were dropped and why.

Aggregates only in this document — group sizes, centres, metrics and figures.
No raw samples and no per-sample tables. The client allows the results to be
published but not the data (`docs/client_meeting_notes.md`, "Data handling").

## Model

*Pending #271.* To record here: which models were compared, the rule used to
choose between them, which was chosen, and why. Link ADR 0005 once it exists.

Already fixed by ADR 0002: clustering runs on the log of the standard
deviation and the absolute bias of each flight dynamics factor, standardized
across the cohort. Group centres are reported as the mean of each group's
Flights in raw units (ft, °, fpm, kt), never the KMeans centre, which lives in
log space and cannot be read in those units.

## Results

### 1. Held-out pilot

Each pilot is held out in turn: the model is fitted on everyone else, then
that pilot's Flights are placed and compared with where the full model puts
them. Whole pilots are held out rather than a random share of Flights,
because a random split would put the same pilot on both sides and Flights by
one pilot resemble each other, so the score would mostly measure that.

The two fits number their groups independently, so the held-out fit's
numbering is matched to the full model's on the pilots both saw before the
held-out Flights are compared.

*Table: `inference/results/held_out_pilot.csv`.* Agreement per pilot and
overall.

### 2. Pilot consistency, against a shuffled null

How much of each pilot's flying lands in one group, averaged over pilots with
more than one Flight, then compared with 1,000 shuffles of the group labels.

**This measures consistency, not skill.** A pilot can genuinely have a good
day and a bad one, and that shows up here as inconsistency rather than as
evidence against the groups.

*Figure in `metrics.json` under `pilot_consistency`:* observed value, the
shuffled mean and maximum, and the p-value. The p-value counts the observed
value itself, so it is never 0 — a finite number of shuffles cannot show that.

### 3. Synthetic recovery

The whole pipeline run on `generate_synthetic_pilots` seeds 0–11, scored by
adjusted Rand index against the hidden tiers. This is the only place the truth
is known. Tiers overlap by design in the generator, so 1.0 is not expected and
would suggest the synthetic cohort is easier than real pilots.

*Table: `inference/results/synthetic_recovery.csv`,* with mean and range.

### 4. Sensitivity

How many Flights change group when something small changes.

| variant | reported |
|---|---|
| log floors halved | yes |
| log floors doubled | yes |
| a pilot with very few Flights removed | yes |
| the largest vertical-speed-spike Flights removed | yes |
| leg heading tolerance ±10° / ±20° / ±30° | **not available** |
| settle window shorter / longer | **not available** |

The last two are not reported because the offline feature pipeline has
neither. Targets come straight from the `target_*` columns, and the settle
window exists only in `live.py` for the sidecar. Both arrive with #229.

*Table: `inference/results/sensitivity.csv`.*

A row reading 0 means the groups did not move. That is a result, not a broken
comparison: the harness is tested against a change large enough to move
Flights, so a comparison that always returned 0 would fail its own tests.

### 5. Interpretation

*Tables: `inference/results/group_centres.csv` and `group_descriptions.csv`.
Figures: `groups_pca.png` and `groups_dendrogram.png`.*

Centres are in raw units. The scatter is the first two principal components
of the scaled features, coloured by group with pilot carried by marker shape,
so a group made of a single pilot is visible as one. The dendrogram is Ward
linkage over the same scaled features.

Each group also gets one plain sentence, for example "holds altitude tightly,
drifts on heading". The wording is relative to this cohort and says nothing
about pilots in general.

### 6. Do the groups line up with experience?

**Not reported.** No pilot metadata exists in the repository: #214 is open and
marked `needs-info`, so there are no flight hours and no certification data to
compare against. Nothing here is a null result — the comparison has not been
made.

When #214 lands, this section takes the Spearman correlation between each
pilot's `log(flight_hours)` and their group's deviation, a Kruskal–Wallis test
of hours across groups, and a table of groups against ATP yes/no. With 5 to 10
pilots, report effect sizes and say plainly that the sample is too small for
significance.

ADR 0001 governs this section: metadata interprets the groups afterwards and
is never used to choose k or the model.

## Limitations

These hold however the numbers come out, and belong in any summary of this
work.

- **The cohort is tiny.** A handful of pilots and a few dozen Flights makes
  this exploratory. It describes these pilots on these flights, and is not a
  finding about pilots in general.
- **Targets are rule-based Legs, not the client's segmentation.** The client
  marks seven Segments by hand from the iMotions audio and video. Those have
  not been shared, so targets here come from our own rules and the two may not
  line up.
- **No airspeed targets.** Airspeed deviation is computed against what we
  have, which is not a briefed target.
- **Prepar3D at about 1 Hz, not X-Plane at 20 Hz.** Within-flight variability
  is measured from twenty times fewer samples than the live pipeline will see,
  so the standard deviations are not directly comparable to anything the relay
  will produce.
- **Consistency is not skill.** See section 2.

## Next steps

*To be agreed.* Candidates, in rough order of what would change the result
most:

- the client's seven Segments, which would replace the rule-based Legs
- pilot metadata (#214), which would let section 6 be written at all
- more pilots: every limitation above is really the cohort size

## One-paragraph summary for the client

> We grouped flights by how closely each one held its target altitude,
> heading, vertical speed and airspeed. The groups come only from that flying
> data. Flight hours and certification were deliberately left out of the
> grouping, and are compared against the groups afterwards, so that any
> pattern we see between experience and performance is something the data
> showed us rather than something we built in — this is what we agreed in the
> meeting. The results are exploratory: with this many pilots they describe
> the flights we have rather than pilots in general, and we would want more
> pilots before treating any pattern as a finding.
