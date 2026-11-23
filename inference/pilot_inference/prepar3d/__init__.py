"""
Turn the client's Prepar3D sample into one deviation-feature row per Flight.

The pipeline is four steps, each usable on its own:

    load.py            one TSV -> a sample table, parsed by header name
    clean.py           trim to airborne, flag gaps and vertical-speed spikes
    legs.py            detect Legs, attach the scenario's Targets
    build_features.py  the CLI that runs all three over a folder

Vocabulary (Flight, Leg, Target, Settle window, Deviation features) is in
GLOSSARY.md. The deviation maths itself is not here: it is reused from
pilot_inference.performance.features.

The input is under an IRB protocol. Nothing in this package writes client
data anywhere but inference/data/client/ and inference/results/, both
gitignored.
"""
