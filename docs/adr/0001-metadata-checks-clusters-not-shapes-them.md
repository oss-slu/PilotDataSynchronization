# Pilot metadata checks performance groups; it does not form them

The client asked for pilot rating and flight hours to be "factored in" to pilot clustering (#216). We form **Performance groups** from **Deviation features** only. **Pilot metadata** (certification level, flight hours, pilot rating) is compared against the groups afterwards, for example by checking whether high-hours pilots land in the low-deviation group. Identifiers (`pilot_id`, `flight_id`) are never features either way.

## Considered Options

- **Metadata as clustering features.** Rejected. Groups would mix how a pilot flies with who the pilot is: two pilots who fly identically but have 50 and 5,000 hours could be split apart. It would also make clustering wait on #214, where the type of pilot rating is still unresolved.
- **Metadata afterwards (chosen).** Groups describe flying behaviour alone. Comparing them with metadata is an independent check that they mean something, and it still answers the client's question of how rating and hours relate to performance.

## Consequences

When presenting results, explain to the client that metadata informs the interpretation of performance groups rather than their formation. If the client insists on metadata as an input, revisit this ADR, since it changes what every group means.
