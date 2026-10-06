"""fontpixel.woff: WOFF1/WOFF2 containers compared with the same Rust calls."""
import pytest

import fontpixel
import fontpixel.sfnt.collection as collection
from fontpixel import sfnt, woff
from fontpixel.woff import woff1, woff2
from sfnt_p3_support import CIRCLE, E, GOLDEN, h, norm

AUX = {
    "major_version": 1,
    "minor_version": 2,
    "metadata": b'<?xml version="1.0"?><metadata version="1.0"/>',
    "private_data": b"\x01\x02\x03",
}


def test_woff1():
    e = woff1.encode(GOLDEN)
    assert e["bytes"] == h("woff1.encode")
    assert norm(e["report"]) == {"entries": []}
    d = woff1.decode(e["bytes"])
    assert set(d) == {"sfnt", "auxiliary", "report"}
    sfnt.Sfnt(d["sfnt"]).validate_checksums()
    ea = woff1.encode(GOLDEN, {"compression_level": 6, "auxiliary": AUX})
    assert ea["bytes"] == h("woff1.encode.aux")
    da = woff1.decode(ea["bytes"])
    assert da["sfnt"] == h("woff1.decode.sfnt")
    assert da["auxiliary"] == AUX
    # list-of-int byte blocks (the serde form) are accepted too
    listed = dict(AUX, metadata=list(AUX["metadata"]), private_data=list(AUX["private_data"]))
    assert woff1.encode(GOLDEN, {"compression_level": 6, "auxiliary": listed})["bytes"] == ea["bytes"]


def test_woff2():
    e = woff2.encode(GOLDEN)
    assert e["bytes"] == h("woff2.encode")
    assert woff2.decode(e["bytes"])["sfnt"] == h("woff2.decode")
    q5 = woff2.encode(
        GOLDEN,
        {"quality": 5, "transform_glyf": False, "transform_hmtx": False, "compression_threads": 1, "auxiliary": AUX},
    )
    assert q5["bytes"] == h("woff2.encode.q5")
    assert woff2.decode(q5["bytes"])["auxiliary"] == AUX
    ttc, _ = collection.encode([GOLDEN, CIRCLE], 0x10000)
    et = woff2.encode(ttc)
    assert et["bytes"] == h("woff2.encode.ttc")
    dt = woff2.decode(et["bytes"])
    assert dt["sfnt"] == h("woff2.decode.ttc")
    assert len(collection.Collection(dt["sfnt"]).faces) == 2
    many = woff2.encode_many([GOLDEN, CIRCLE], None, None, 2)
    assert [m["bytes"].hex() for m in many] == E["woff2.encode_many"]
    assert [m["bytes"] for m in woff2.encode_many([GOLDEN, CIRCLE])] == [m["bytes"] for m in many]
    assert [
        woff2.brotli_encoder_heap_bound(11, 22, 1000, 2),
        woff2.brotli_encoder_proven_heap_bound(11, 22, 1000, 2),
        woff2.brotli_encoder_heap_bound(5, 22, 4096, 1),
        woff2.brotli_encoder_proven_heap_bound(5, 22, 4096, 1),
    ] == E["woff2.brotli_bounds"]
    assert woff2.brotli_encoder_heap_bound(11, 22, 2**63, 2**20) is None


def test_woff_errors_and_limits():
    for mod in (woff1, woff2):
        with pytest.raises(fontpixel.InvalidInputError) as e:
            mod.decode(b"wOFFjunk")
        assert e.value.kind == "InvalidInput"
        with pytest.raises(fontpixel.InvalidInputError):
            mod.encode(b"not an sfnt")
        with pytest.raises(fontpixel.ResourceLimitError) as e:
            mod.encode(GOLDEN, None, {"sfnt": {"resources": {"max_total_bytes": 100}}})
        assert e.value.limit == 100
        with pytest.raises(ValueError):
            mod.encode(GOLDEN, {"no_such_option": 1})
    with pytest.raises(fontpixel.InvalidInputError):
        woff2.encode(GOLDEN, {"quality": 12})
    with pytest.raises(fontpixel.InvalidInputError):
        woff1.encode(GOLDEN, {"compression_level": 10})
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        woff2.encode_many([GOLDEN, CIRCLE], None, {"max_batch_fonts": 1})
    assert e.value.limit == 1
    with pytest.raises(fontpixel.ResourceLimitError):
        woff1.encode(GOLDEN, {"auxiliary": AUX}, {"max_auxiliary_bytes": 4})


def test_woff_defaults():
    d = woff.defaults()
    assert d["woff1.Options"] == {"compression_level": 9, "auxiliary": d["Auxiliary"]}
    assert d["woff2.Options"]["quality"] == 11
    assert d["woff2.Options"]["compression_threads"] == 2
    assert d["Auxiliary"] == {"major_version": 0, "minor_version": 0, "metadata": None, "private_data": None}
    assert d["Limits"]["max_batch_fonts"] == 1024
    assert woff2.encode(GOLDEN, d["woff2.Options"], d["Limits"])["bytes"] == h("woff2.encode")
