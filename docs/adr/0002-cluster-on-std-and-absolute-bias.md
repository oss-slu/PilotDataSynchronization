# Cluster on the log of std and |bias|, not mad or signed bias

`inference/pilot_inference/performance/features.py` emits `mad`, `std`, signed `bias` and `abs_bias` per flight dynamics factor. Pilot clustering (#216) uses only `std` (how much the pilot wobbles) and `|bias|` (how far the pilot sits off target), 8 features in total. `|bias|` is the mean of each flight's own `|bias|`, not the absolute value of the mean signed bias, which would cancel a pilot's flights on opposite sides of the target. These are the two independent parts of deviation: mean squared deviation is exactly `std² + bias²`. Each feature is clustered on a log scale, `log(x + floor)`, and then standardized over the cohort. The floors are 1 ft, 0.1°, 2 ft/min and 0.1 kt.

## Considered Options

- **All 12 (`mad`, `std`, signed `bias`).** Rejected. Signed bias puts a pilot 50 ft high and one 50 ft low at opposite ends of an axis, although they performed equally well. `mad` mostly restates `std` and `|bias|`, so with KMeans's Euclidean distance it counts each metric twice.
- **`mad` + `std`.** Rejected because of that overlap. The synthetic pilots cannot confirm it (see Evidence).
- **`mad` only (4).** The fewest dimensions, but it cannot separate a steady pilot who sits off target from an on-target pilot who oscillates. This is a structural argument, not a measured one: synthetic pilots draw bias and wobble from one scale, so they move together and never present that case (see Evidence).
- **Absolute value of each pilot's mean signed bias.** Rejected. A pilot 60 ft high on one flight and 60 ft low on the next averages to zero bias and looks on target. Tested on synthetic pilots (see Evidence).
- **Linear scale.** Rejected after testing on synthetic pilots (see Evidence). Deviation sizes are positive and pilots differ by multiples, so on a linear scale the worst pilots stretch every axis and the rest bunch together.
- **Log with no floor.** Rejected. It would make 0.01 ft vs 0.1 ft of bias as far apart as 10 ft vs 100 ft.

## Evidence

Mean adjusted Rand index against the hidden tier, k = 3, synthetic seeds 0 to 11, on synthetic pilots whose adjacent tiers overlap. To reproduce, run `generate_pilots` for each seed, `compute_features`, then the clustering steps in `clustering.py` with each feature set.

| Features | Log | Linear |
| --- | --- | --- |
| `std` + per-flight `abs_bias` (chosen) | 0.78 | 0.51 |
| `mad` only | 0.76 | 0.50 |
| `mad` + `std` | 0.74 | 0.48 |
| `std` + abs of each pilot's mean signed bias | 0.55 | 0.36 |

Per-seed scores for the chosen option run from 0.37 to 1.00, so differences of a few hundredths are noise. The runs support two choices: log over linear, and per-flight `abs_bias` over the per-pilot variant. Both orderings also held at each of two heavier overlap settings tried, though the gap between log and linear narrowed to 0.03 at the first. The runs do not separate the chosen features from the `mad` options, so those rejections rest on the structural arguments above.

Two limits. The generator scales deviations multiplicatively, which is the assumption the log scale encodes, so this shows that log handles that structure and not that real pilots have it. And the overlap was set after trying a few settings, not derived from real flights.

An earlier generator kept the tiers' scale bands apart. On it the chosen features and both `mad` options scored 1.00 on either scale, so it could not compare them, while the per-pilot variant scored 0.87 on log and 0.41 on linear. Figures from before the tiers overlapped do not apply here.

## Consequences

With 10 to 20 pilots, even 8 features is far below the roughly 70 × features sample size recommended for segmentation (Dolnicar et al., 2014). The k sweep's subsample stability is how we find out whether the performance groups hold up. If they don't, drop to 4 features before adding any. `rows_used` and `flights` measure how much data a pilot has, not performance, and are excluded. Group centres are reported as raw-unit means of their members, since KMeans centres live in log-scaled space.
