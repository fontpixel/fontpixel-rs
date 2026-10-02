#!/usr/bin/env python3
"""Reproducible SPEC-v3 BDF oracle cases; reference and samples remain read-only."""
import argparse
import hashlib
import json
from pathlib import Path
import random
import sys
import warnings

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT.parent / "reference/bdfparser/src"
sys.path.insert(0, str(REFERENCE))
from bdfparser import Font  # noqa: E402


def glyph(name="A", code="65", bbox="1 1 0 0", rows=("80",), extra=()):
    return "\n".join([f"STARTCHAR {name}", f"ENCODING {code}", *extra, f"BBX {bbox}", "BITMAP", *rows, "ENDCHAR"]) + "\n"


def font(props=(), glyphs=None, header=(), version="2.1"):
    glyphs = [glyph()] if glyphs is None else glyphs
    return "\n".join([f"STARTFONT {version}", "FONT sample", "SIZE 8 75 75", "FONTBOUNDINGBOX 8 8 0 -1", *header,
                       f"STARTPROPERTIES {len(props)}", *props, "ENDPROPERTIES", f"CHARS {len(glyphs)}"]) + "\n" + "".join(glyphs) + "ENDFONT\n"


def cases():
    base = font(["PIXEL_SIZE 8", 'FAMILY_NAME "tiny"'])
    yield "basic", base
    yield "no glyphs", font(glyphs=[])
    yield "all metrics", font(['MinSpace 4', 'COPYRIGHT "say ""hello"""', 'FLAG', 'RAW token value', 'COMMENT "prop comment"'],
        [glyph(extra=["SWIDTH 500 -1", "DWIDTH 8 0", "SWIDTH1 0 1000", "DWIDTH1 0 -8", "VVECTOR 4,7"])],
        ["METRICSSET 2", "CONTENTVERSION 7", "SWIDTH 100 0", "DWIDTH 8 0", "SWIDTH1 0 1000", "DWIDTH1 0 -8", "VVECTOR 4 , 7", 'COMMENT "header"', 'COMMENT "a "q""'])
    yield "unencoded interleaved", font(glyphs=[glyph("u", "-1 99"), glyph(), glyph("v", "-2 -2147483648"), glyph("b", "66")])
    yield "duplicates", font(['X 1','Y 2','x 3','COMMENT c','Comment "d"','comment "e"'],
        [glyph(), glyph("B", "66"), glyph("A replacement", "65"), glyph("negative", "-7 42")])
    for cp in ["-2147483648", "2147483647", "1114112", "55296", "65535", "+00065", "-0", "-1", "-2"]:
        yield f"encoding {cp}", font(glyphs=[glyph(code=cp)])
    for value in ['+0','-0','0012','-2147483648','4294967295','4294967296','-2147483649','123_456','１２','+','-','"unclosed','"a" trailing','""""','__proto__','a\u001fb']:
        yield f"property {value!r}", font([f"123 {value}", "__proto__ ok", "Constructor xyz"])
    for value in ["1_2","0x10","Infinity","inf","2147483648","-2147483649","+","-","１２","1.0","1\u001f2","1\u00852","1\ufeff2"]:
        for key, good in [("SIZE", "8 75 75"),("FONTBOUNDINGBOX", "8 8 0 -1"),("CHARS", "1"),("ENCODING", "65"),("BBX", "1 1 0 0"),("METRICSSET","2")]:
            template = font(header=["METRICSSET 2"])
            yield f"bad {key} {value!r}", template.replace(f"{key} {good}", f"{key} {value}")
    for version in ["2", "2.0", "02.100", "10.25", "0.00001", "0.0001", "0.000000001", "1.0000000000000002", "99999999999999999999999999999999", "-2", "2.", ".2", "2e1", "2.1 trailing"]:
        yield f"version {version}", font(version=version)
    for bbox in ["-1 1 0 0", "1 -1 0 0", "0 0 -2147483648 2147483647", "0 3 0 0", "3 0 0 0"]:
        yield f"bbox {bbox}", font(glyphs=[glyph(bbox=bbox)])
    for rows in [(), ("f",),("001",),("ffff",),("80","00"),("xyz",),("CA FE",),("0x80",),("COMMENT ignored","80"),("80", "invalid-extra")]:
        yield f"rows {rows}", font(glyphs=[glyph(rows=rows)])
    for metrics in ["VVECTOR 4,7", "VVECTOR 4,,, 7", "VVECTOR ,4,7", "VVECTOR 4 7,", "VVECTOR 4", "SWIDTH 1 2 extra", "SWIDTH 1 2\nSWIDTH wrong", "ENCODING 65\nENCODING wrong", "BBX 1 1 0 0\nBBX wrong"]:
        yield f"metrics {metrics}", font(glyphs=[glyph(extra=[metrics])])
    for eol in ["\n","\r","\r\n"]:
        yield f"eol {eol!r}", "\ufeff" + base.replace("\n",eol)
    for char in [" ","\t","\v","\f", "\u001f", "\u0085", "\ufeff"]:
        yield f"whitespace {char!r}", base.replace(" ",char)
    for key in ["STARTFONT", "FONT", "SIZE", "FONTBOUNDINGBOX", "STARTPROPERTIES", "ENDPROPERTIES", "CHARS", "ENCODING", "BBX", "BITMAP", "ENDCHAR", "ENDFONT"]:
        yield f"missing {key}", "\n".join(line for line in base.splitlines() if not line.startswith(key + " ") and line != key) + "\n"
    for ending in ["", "STARTFONT 2.1", "STARTPROPERTIES 0", "ENDPROPERTIES", "CHARS 1", "BBX 1 1 0 0", "BITMAP", "ENDCHAR"]:
        index = base.find(ending) if ending else 0
        source = base[: index + len(ending)] if ending else ""
        yield f"eof {ending}", source
    for count in ["wrong","", "999999999999999999999999999", "-1", "+00002", "2 extra"]:
        yield f"prop count {count}", base.replace("STARTPROPERTIES 2",f"STARTPROPERTIES {count}")
    yield "missing property block", font().replace("STARTPROPERTIES 0\nENDPROPERTIES\n", "")
    yield "no chars after props", base.replace("CHARS 1\n", "")
    yield "missing endchar before start", font(glyphs=[glyph().replace("ENDCHAR\n", ""), glyph("B","66")])
    yield "unknown lines", base.replace("FONT sample", "WIBBLE xxx\nFONT sample").replace("CHARS 1", "Unknown outside\nCHARS 1").replace("BITMAP", "Unknown glyph\nBITMAP")
    yield "duplicate headers", base.replace("SIZE 8 75 75", "SIZE 2 3 4\nSIZE 8 75 75")
    for registry, enc, codes in [("ISO8859","1",[65,160]),("MICROSOFT","CP1252",[128,255]),("GB2312.1980","0",[0x2121,0x484b]),("GB2312.1980","1",[0xa1a1,0xc8cb]),("BIG5","0",[0xa440,0xa448]),("JISX0208.1983","0",[0x2121]),("SHIFTJIS","0",[0x82a0]),("GB18030","0",[0x81308130]),("UNKNOWN","RU",[1114112]),("UTF32BE","1",[55296])]:
        yield f"charset {registry}", font([f'CHARSET_REGISTRY "{registry}"',f'CHARSET_ENCODING "{enc}"'],[glyph(code=str(cp)) for cp in codes])
    for char in ["\u200b", "\u2028", "\u2060", "\u0378", "\U0010ffff", "🙂", "\u00ad"]:
        yield f"unicode diagnostic {ord(char):x}", base.replace("FONT sample", f"Unknown{char} value\nFONT sample")
    mutations = random.Random(0xBDF00BAD)
    alphabet = "STARTFONT CHARS\n\r\t\v\f0123456789abcdef+-_,\"'\x00\x1f\u0085\ufeff\u200b🙂"
    for i in range(400):
        text = base
        for _ in range(mutations.randrange(1, 7)):
            at = mutations.randrange(len(text) + 1)
            if mutations.randrange(2): text = text[:at] + mutations.choice(alphabet) + text[at:]
            else: text = text[:at] + text[at + mutations.randrange(1, 20):]
        yield f"mutated {i}", text
    rng = random.Random(0xBDF30102)
    for i in range(320):
        gs=[]
        for j in range(rng.randrange(0,12)):
            w,h=rng.randrange(0,34),rng.randrange(0,18)
            rows=[bytes(rng.randrange(256) for _ in range((w+7)//8)).hex() for _ in range(h)]
            # Empty rows are skipped by SPEC; keep zero width dimensions explicit.
            cp=rng.choice([-1,-2,65,66,67,0x110000,rng.randrange(0,0x110000)])
            extra=[f"SWIDTH {rng.randrange(-1000,1000)} 0",f"DWIDTH {w} {rng.randrange(-2,2)}"]
            gs.append(glyph(f"glyph {i} {j}", str(cp),f"{w} {h} {rng.randrange(-9,9)} {rng.randrange(-9,9)}",rows,extra))
        data=font([f'FAMILY_NAME "seeded {i}"',f"PIXEL_SIZE {rng.randrange(1,34)}",'CHARSET_REGISTRY "ISO10646"','CHARSET_ENCODING "1"'],gs)
        if i%7==0: data=data.replace("\n","\r\n")
        if i%11==0: data=data.replace("ENCODING", "ENCODING\t")
        yield f"seeded {i}",data


def oracle(name, source):
    result={"name":name,"source":source}
    with warnings.catch_warnings(record=True) as seen:
        warnings.simplefilter("always")
        try:
            f=Font().load_string(source)
        except Exception as e:
            result["parse_error"]=str(e)
        else:
            result["metadata"]={"headers":f.headers,"props":f.props,"propnames":f._propnames,"glyphs":list(f.glyphs.values()),"unencoded":f.unencoded}
            try: result["output"]=f.tobdf()
            except Exception as e: result["write_error"]=str(e)
        result["warnings"]=[str(w.message) for w in seen]
    return result


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--check",action="store_true");args=parser.parse_args()
    entries=[oracle(name,source) for name,source in cases()]
    target=ROOT/"crates/fontpixel/tests/data/bdf_reference.json"
    payload=(json.dumps(entries,ensure_ascii=False,separators=(",",":"))+"\n").encode()
    if args.check:
        if target.read_bytes()!=payload: raise SystemExit("BDF oracle drift")
    else: target.write_bytes(payload)
    for source in sorted((ROOT/"tests/data/bdf").glob("*.bdf")):
        if source.name.endswith(".golden.bdf"): continue
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            payload=Font(source).tobdf().encode()
        golden=source.with_suffix(".golden.bdf")
        if args.check:
            if golden.read_bytes()!=payload: raise SystemExit(f"BDF golden drift: {source}")
        else: golden.write_bytes(payload)
    print(f"BDF oracle: {len(entries)} cases; {target.stat().st_size} bytes; reference font.py SHA256 {hashlib.sha256((REFERENCE/'bdfparser/font.py').read_bytes()).hexdigest()}")

if __name__=="__main__":main()
