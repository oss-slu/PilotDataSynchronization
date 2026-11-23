"""
One JSON object per line on stdout, for a program reading this one.

The relay's window runs the listeners as child processes and shows what they
report (#285). It could scrape the console lines, but those exist for a human
reading a terminal, and the person who rewords one is working in Python and
will never run the relay's Rust tests. So the two halves get a contract
instead: with `--json`, a listener writes events here and nothing else to
stdout, and `tests/test_events.py` fails if the shape changes.

Without `--json` nothing in this module is used and the console output is
exactly what it always was.
"""

from __future__ import annotations

import json
import sys
from typing import Any

# Bumped when a field changes meaning or disappears. The reader logs a version
# it does not know rather than guessing at the payload.
PROTOCOL_VERSION = 1

# Every event a listener can emit. A reader is expected to ignore a kind it
# does not know, so adding one here is not a breaking change.
EVENT_KINDS = (
    "fitting",
    "cohort",
    "started",
    "status",
    "instruction",
    "stopped",
    "error",
)


def _plain(value: Any) -> Any:
    """
    Make numpy scalars and Paths serialisable.

    pandas hands back numpy types, and `json` refuses them with a TypeError
    that would surface as the listener dying for no visible reason.
    """
    if hasattr(value, "item"):
        return value.item()
    return str(value)


def encode(kind: str, **fields: Any) -> str:
    """The line for one event, without writing it. Separated out so tests can
    read it without capturing stdout."""
    if kind not in EVENT_KINDS:
        raise ValueError(f"Unknown event kind {kind!r}. Add it to EVENT_KINDS.")
    payload = {"event": kind, "v": PROTOCOL_VERSION}
    payload.update(fields)
    return json.dumps(payload, default=_plain)


def emit(kind: str, **fields: Any) -> None:
    """
    Write one event and flush it.

    The flush is deliberate. Python block-buffers a piped stdout, so without
    it the reader would see nothing for kilobytes at a time, or until the
    process exited. `python -u` also fixes that, and this works whether or not
    the caller remembered it.
    """
    sys.stdout.write(encode(kind, **fields) + "\n")
    sys.stdout.flush()
