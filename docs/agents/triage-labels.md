# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this table.

Edit the right-hand column to match whatever vocabulary you actually use.

## Area labels

Orthogonal to the triage roles above: these say which part of the project an issue touches. Apply one or more alongside the triage label. They are not triage states, so `triage` should never remove them when changing state.

| Label               | Covers                          |
| ------------------- | ------------------------------- |
| `area:ml`           | ML / inference work (`inference/`) |
| `area:relay`        | The relay component (`relay/`)  |
| `area:xplane-plugin`| The X-Plane plugin (`xplane_plugin/`) |
| `area:docs`         | Documentation (`docs/`)         |

Existing type labels (`bug`, `enhancement`, `documentation`, `question`, etc.) remain in use. Create any missing area label with `gh label create` before first applying it.
