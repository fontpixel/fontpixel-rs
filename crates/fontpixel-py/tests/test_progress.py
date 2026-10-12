"""Progress callbacks and cancellation of ``fontpixel.ops.merge``/``convert``.

The callbacks map ``fontpixel_ops::merge_with_progress`` and
``convert_with_progress``: a dict per completed step, ``False`` cancels with
``CancelledError``, an exception cancels and is re-raised, and results never
depend on the callback.
"""
from __future__ import annotations

import os
from pathlib import Path

import pytest

import fontpixel as fp
from fontpixel import ops

REPO = Path(__file__).resolve().parents[3]
DATA = REPO / "tests" / "data"
SPLEEN = (DATA / "bdf" / "spleen-5x8.bdf").read_bytes()
BLOCK = (DATA / "bdf" / "3x5-block.bdf").read_bytes()
FILES = {"fonts/s.bdf": SPLEEN, "fonts/b.bdf": BLOCK}
KEYS = {"step", "done", "total", "item", "glyphs", "bytes"}


def merge_request():
    return {
        "family": "Mix",
        "parts": [
            {"input": "fonts/s.bdf", "take": "western"},
            {"input": "fonts/b.bdf", "take": "all", "dy": -1},
        ],
        "outputs": [{"destination": "out/m.bdf"}, {"destination": "out/m.woff2"}],
    }


def convert_request():
    return {"inputs": ["fonts/s.bdf", "fonts/b.bdf"], "output": "out", "to": "pcf", "each": True}


def test_merge_progress_reports_each_step_and_equals_merge():
    plain = ops.merge(merge_request(), files=FILES)
    events = []
    observed = ops.merge(merge_request(), files=FILES, progress=events.append)
    assert observed.outputs == plain.outputs
    assert observed.json(True) == plain.json(True) and observed.text() == plain.text()
    assert all(set(event) == KEYS for event in events)
    assert [(e["step"], e["done"], e["total"]) for e in events] == [
        ("read", 1, 2), ("read", 2, 2), ("merge", 1, 1), ("encode", 1, 2), ("encode", 2, 2)]
    assert [e["item"] for e in events] == [
        "fonts/s.bdf", "fonts/b.bdf", None, "out/m.bdf", "out/m.woff2"]
    assert events[2]["glyphs"] == plain.summary["glyphs"]
    sizes = [s["bytes"] for s in plain.summary["summaries"]]
    assert [e["bytes"] for e in events[3:]] == sizes
    assert all(e["glyphs"] > 0 and e["bytes"] is None for e in events[:2])
    # progress=None is the plain operation.
    assert ops.merge(merge_request(), files=FILES, progress=None).outputs == plain.outputs


def test_convert_progress_alternates_reads_and_encodes():
    plain = ops.convert(convert_request(), files=FILES)
    events = []
    observed = ops.convert(convert_request(), files=FILES, progress=events.append)
    assert observed.outputs == plain.outputs and observed.json(True) == plain.json(True)
    assert [(e["step"], e["done"], e["total"], e["item"]) for e in events] == [
        # outputs are reported as platform paths (out\\s.pcf on Windows)
        ("read", 1, 2, "fonts/s.bdf"), ("encode", 1, 2, os.path.join("out", "s.pcf")),
        ("read", 2, 2, "fonts/b.bdf"), ("encode", 2, 2, os.path.join("out", "b.pcf"))]
    assert [e["bytes"] for e in events[1::2]] == [len(plain.outputs["out/s.pcf"]),
                                                  len(plain.outputs["out/b.pcf"])]
    # vectorize is convert: the same callback contract.
    seen = []
    ops.vectorize(convert_request(), files=FILES, progress=seen.append)
    assert seen == events


@pytest.mark.parametrize("stop", range(5))
def test_returning_false_cancels_at_every_step(stop):
    calls = []

    def callback(event):
        calls.append(event)
        return len(calls) <= stop or False

    with pytest.raises(fp.CancelledError) as error:
        ops.merge(merge_request(), files=FILES, progress=callback)
    assert len(calls) == stop + 1
    assert error.value.kind == "Cancelled" and error.value.exit_code == 1
    assert isinstance(error.value, fp.FontpixelError)


def test_only_false_cancels():
    for value in (None, True, 0, "", [], "stop"):
        result = ops.convert(convert_request(), files=FILES, progress=lambda _e, v=value: v)
        assert len(result.outputs) == 2


def test_callback_exceptions_cancel_and_propagate(tmp_path, monkeypatch):
    class Stop(Exception):
        pass

    def callback(event):
        if event["step"] == "encode":
            raise Stop(event["item"])

    with pytest.raises(Stop, match="out/m.bdf"):
        ops.merge(merge_request(), files=FILES, progress=callback)
    with pytest.raises(TypeError, match="progress must be a callable"):
        ops.merge(merge_request(), files=FILES, progress=42)
    # Filesystem mode: a cancelled group leaves the destinations untouched.
    monkeypatch.chdir(tmp_path)
    (tmp_path / "fonts").mkdir()
    for name, data in FILES.items():
        (tmp_path / name).write_bytes(data)
    (tmp_path / "out").mkdir()
    (tmp_path / "out" / "m.bdf").write_bytes(b"previous")
    with pytest.raises(fp.CancelledError):
        ops.merge(merge_request(), progress=lambda e: e["done"] < e["total"] or e["step"] != "encode")
    assert (tmp_path / "out" / "m.bdf").read_bytes() == b"previous"
    assert not (tmp_path / "out" / "m.woff2").exists()
    # The same request without cancelling writes both outputs.
    ops.merge(merge_request(), progress=lambda e: None)
    assert (tmp_path / "out" / "m.bdf").read_bytes() != b"previous"
    assert (tmp_path / "out" / "m.woff2").exists()
