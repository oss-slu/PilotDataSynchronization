# Cluster on the log of std and |bias|, not mad or signed bias

`inference/features.py` emits `mad`, `std` and signed `bias` per flight dynamics factor. Pilot clustering (#216) uses only `std` (how much the pilot wobbles) and `|bias|` (how far the pilot sits off target), 8 features in total. These are the two independent parts of deviation: mean squared deviation is exactly `std² + bias²`. Each feature is clustered on a log scale, `log(x + floor)`, and then standardized over the cohort. The floors are 1 ft, 0.1°, 2 ft/min and 0.1 kt.

## Considered Options

- **All 12 (`mad`, `std`, signed `bias`).** Rejected. Signed bias puts a pilot 50 ft high and one 50 ft low at opposite ends of an axis, although they performed equally well. `mad` mostly restates `std` and `|bias|`, so with KMeans's Euclidean distance it counts each metric twice.
- **`mad` + `std`.** Rejected because of that overlap.
- **`mad` only (4).** The fewest dimensions, but it cannot separate a steady pilot who sits off target from an on-target pilot who oscillates.
- **Linear scale.** Rejected after testing on synthetic pilots. Deviation sizes are positive and pilots differ by multiples, so on a linear scale the worst pilots stretch every axis and the rest bunch together. KMeans then splits the worst tier and merges the other two (mean adjusted Rand index against the true tier about 0.40, against 0.87 on the log scale, over synthetic seeds 0 to 11).
- **Log with no floor.** Rejected. It would make 0.01 ft vs 0.1 ft of bias as far apart as 10 ft vs 100 ft.

## Consequences

With 10 to 20 pilots, even 8 features is far below the roughly 70 × features sample size recommended for segmentation (Dolnicar et al., 2014). The k sweep's subsample stability is how we find out whether the performance groups hold up. If they don't, drop to 4 features before adding any. `rows_used` and `flights` measure how much data a pilot has, not performance, and are excluded. Group centres are reported as raw-unit means of their members, since KMeans centres live in log-scaled space.
