# Pilot Data Synchronization

Streams pilot telemetry from X-Plane to iMotions and builds ML datasets from it. Vocabulary is defined in `CONTEXT.md`; use those terms. Setup, troubleshooting and the full data flow are in `README.md`, and the telemetry contract is in `docs/telemetry_schema.md`. Don't duplicate them here.

## Components

Data flow: `xplane_plugin` (C++) → `baton` (Rust library, compiled to C++) → `relay` (Rust GUI) → iMotions over TCP. `inference` (Python) separately logs the same TCP stream and trains a model.

- `xplane_plugin/`: X-Plane plugin, built with Meson. `baton` lives in `xplane_plugin/subprojects/baton`.
- `relay/`: Rust relay with a GUI.
- `src/server/`: mock iMotions TCP server for testing without iMotions.
- `inference/`: logging, labeling, training. See `inference/README.md`.

## Build and test

Run from the repo root, in order, per component:

- Plugin (also builds baton): `cd xplane_plugin && meson setup build && meson compile -C build && meson test -C build`
- Relay: `cd relay && cargo test`
- Baton: `cd xplane_plugin/subprojects/baton && cargo test`
- Inference: `cd inference && uv sync`, then `uv run python <script>`. `uv run pytest` runs the tests in `inference/tests/`; most scripts are untested.

CI is in `.github/workflows/` (`meson-build.yml`, `super-linter.yml`). Run `meson setup` with the cross-file for your OS if not on native Windows (see README).

## Gotchas

- Hardware-bound: anything needing live X-Plane or iMotions cannot be verified by an agent. Say so rather than claiming it works.
- Data in `inference/Data/` was collected before #196, so `altitude` and `velocity` are mis-scaled (divide by 3.28084 and 1.94384). See `docs/telemetry_schema.md`.
- `yaw` is true heading, effectively a duplicate of `heading`.
- The 20 Hz send on Windows and Linux lives in the plugin window's draw callback.
- Heading deviation needs wrapped angular difference, and floored modulo is Python-only; do not port the formula to C++ or Rust as written.
- `toml .rust-toolchain.toml` is misnamed and not in effect.

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues (oss-slu/PilotDataSynchronization) via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default five triage roles (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`) plus `area:*` labels (`area:ml`, `area:relay`, `area:xplane-plugin`, `area:docs`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
