"""
The JSON event contract between a listener and whatever is running it.

The relay's window reads these (#285). The reason they exist at all is that
console text is written for a person, and the person who rewords a print
statement is working here, not in the relay's Rust tests. These tests fail on
this side instead.
"""

import json

import numpy as np
import pytest

from pilot_inference.events import EVENT_KINDS, PROTOCOL_VERSION, emit, encode


def decode(kind, **fields):
    return json.loads(encode(kind, **fields))


def test_every_event_carries_its_kind_and_the_protocol_version():
    payload = decode("status", given=1)
    assert payload["event"] == "status"
    assert payload["v"] == PROTOCOL_VERSION


def test_an_unknown_kind_is_refused_at_the_point_it_is_written():
    with pytest.raises(ValueError, match="Unknown event kind"):
        encode("nonsense", given=1)


def test_the_kinds_the_relay_reads_are_all_declared():
    for kind in ("fitting", "cohort", "started", "status", "instruction", "stopped", "error"):
        assert kind in EVENT_KINDS


def test_numpy_scalars_survive_the_trip():
    """pandas hands back numpy types, and plain json refuses them."""
    payload = decode("status", scored_seconds=np.float64(42.5), group=np.int64(2))
    assert payload["scored_seconds"] == 42.5
    assert payload["group"] == 2


def test_a_path_becomes_a_string_rather_than_killing_the_listener():
    from pathlib import Path

    payload = decode("started", csv=Path("data") / "live.csv")
    assert "live.csv" in payload["csv"]


def test_nested_values_are_kept():
    payload = decode("status", features={"altitude": {"std": 1.5, "abs_bias": 0.5}})
    assert payload["features"]["altitude"]["std"] == 1.5


def test_each_event_is_one_line_so_a_reader_can_split_on_newlines():
    line = encode("status", description="two\nlines")
    assert "\n" not in line
    assert json.loads(line)["description"] == "two\nlines"


def test_emit_writes_one_line_to_stdout(capsys):
    emit("stopped", scored_seconds=12.0, given=3)
    out = capsys.readouterr().out
    assert out.endswith("\n")
    payload = json.loads(out.strip())
    assert payload["event"] == "stopped"
    assert payload["given"] == 3
