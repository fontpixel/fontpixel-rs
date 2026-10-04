#!/usr/bin/env python3
"""Complete, independent W6.6 Tong and Unifont merge checks (no timing).

prepare copies the original Tong builder/data, runs only that copy, verifies all
20 original BDFs, and keeps the four older regional samples as separate cases.
run checks every native glyph/metric and the complete JSON report, then records
every difference from the original regional build. Documented original-policy
differences NEVER excuse a native-contract mismatch. All paths are explicit;
new evidence directories/files refuse replacement. No Cargo is invoked.
"""
from __future__ import annotations

import argparse
from bisect import bisect_right
from collections import Counter
from copy import deepcopy
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unicodedata

BLOCKS_SHA256 = "c0edefaf1a19771e830a82735472716af6bf3c3975f6c2a23ffbe2580fbbcb15"
BUILD_SHA256 = "ffe1ef41d0fd519557af5416421006952bbc033a1a5398db9511c1ddee06fbc6"
REGIONS = ("SC", "TC", "JP", "KR")
SIZES = ("Large", "Small")
SPACINGS = ("prop", "mono")
ALL_SCALARS = ((0, 0xD7FF), (0xE000, 0x10FFFF))
GEOMETRIC = ((0x2500, 0x259F), (0x2800, 0x28FF), (0xE0A0, 0xE0D4))
# Independent policy expressed as UCD block names, not imported Rust ranges.
CJK_BLOCKS = frozenset("""Hangul Jamo
CJK Radicals Supplement
Kangxi Radicals
Ideographic Description Characters
CJK Symbols and Punctuation
Hiragana
Katakana
Bopomofo
Hangul Compatibility Jamo
Kanbun
Bopomofo Extended
CJK Strokes
Katakana Phonetic Extensions
Enclosed CJK Letters and Months
CJK Compatibility
CJK Unified Ideographs Extension A
CJK Unified Ideographs
Hangul Jamo Extended-A
Hangul Syllables
Hangul Jamo Extended-B
CJK Compatibility Ideographs
Vertical Forms
CJK Compatibility Forms
Small Form Variants
Halfwidth and Fullwidth Forms
Ideographic Symbols and Punctuation
Kana Extended-B
Kana Supplement
Kana Extended-A
Small Kana Extension
Enclosed Ideographic Supplement
CJK Unified Ideographs Extension B
CJK Unified Ideographs Extension C
CJK Unified Ideographs Extension D
CJK Unified Ideographs Extension E
CJK Unified Ideographs Extension F
CJK Unified Ideographs Extension I
CJK Compatibility Ideographs Supplement
CJK Unified Ideographs Extension G
CJK Unified Ideographs Extension H
CJK Unified Ideographs Extension J""".splitlines())
PROFILES = ("category", "fallback", "auto", "regional-geometry")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()


def save(path, value):
    path = Path(path)
    data = json_bytes(value)
    with path.open("xb") as stream:
        stream.write(gzip.compress(data, mtime=0) if path.suffix == ".gz" else data)


def load(path):
    path = Path(path)
    data = path.read_bytes()
    return json.loads(gzip.decompress(data) if path.suffix == ".gz" else data)


def inventory(root):
    root = Path(root)
    files = {}
    for path in sorted(root.rglob("*")):
        require(not path.is_symlink(), f"source symlink requires explicit review: {path}")
        if path.is_file():
            files[str(path.relative_to(root))] = {"bytes": path.stat().st_size, "sha256": digest(path)}
    return files


def protected(path):
    root = Path(__file__).resolve().parents[2]
    return any(Path(path).resolve().is_relative_to(root / name)
               for name in ("reference", "samples", "validation-inputs", "original-task"))


def fresh_directory(path, inputs=()):
    path = Path(path).resolve()
    require(not protected(path), f"output is in a protected tree: {path}")
    for source in inputs:
        require(not path.is_relative_to(Path(source).resolve()), f"output lies inside input: {source}")
    path.mkdir(parents=True, exist_ok=False)
    return path


def unquote(value):
    return value[1:-1].replace('""', '"') if value.startswith('"') and value.endswith('"') else value


def parse_bdf(path):
    """Retain physical records, duplicates/unencoded, every glyph field and row.

    This validator intentionally accepts the unambiguous original-workload BDF
    subset. Unexpected glyph fields are retained and compared, never discarded.
    Each bitmap row is checked against BBX; wire spelling/padding is also kept.
    """
    font = {"headers": [], "properties": [], "comments": [], "glyphs": []}
    glyph = None
    bitmap = properties = False
    property_count = chars = None
    ended = False
    numeric = {"ENCODING", "SWIDTH", "DWIDTH", "SWIDTH1", "DWIDTH1", "VVECTOR", "BBX"}
    for number, line in enumerate(Path(path).read_text(encoding="utf-8").splitlines(), 1):
        key, _, value = line.partition(" ")
        if glyph is not None:
            if key == "ENDCHAR":
                require(bitmap and "ENCODING" in glyph["fields"] and "BBX" in glyph["fields"],
                        f"{path}:{number}: incomplete glyph")
                width, height, _, _ = glyph["fields"]["BBX"]
                require(width >= 0 and height >= 0 and len(glyph["rows"]) == height,
                        f"{path}:{number}: bitmap/BBX row count")
                for row in glyph["rows"]:
                    require(len(row) == ((width + 7) // 8) * 2 and all(c in "0123456789abcdefABCDEF" for c in row),
                            f"{path}:{number}: bitmap width or non-hex data")
                glyph["pixels"] = [bin(int(row, 16))[2:].zfill(len(row) * 4)[:width] if row else ""
                                   for row in glyph["rows"]]
                font["glyphs"].append(glyph)
                glyph = None
                bitmap = False
            elif bitmap:
                glyph["rows"].append(line)
            elif key == "BITMAP":
                bitmap = True
            else:
                require(key not in glyph["fields"], f"{path}:{number}: repeated glyph field {key}")
                glyph["fields"][key] = list(map(int, value.split())) if key in numeric else value
            continue
        require(not ended, f"{path}:{number}: trailing data after ENDFONT")
        if key == "STARTCHAR":
            require(not properties, f"{path}:{number}: unfinished properties")
            glyph = {"name": value, "fields": {}, "rows": []}
        elif key == "COMMENT":
            # Native BDF notice projection may emit COMMENT inside properties.
            font["comments"].append(unquote(value))
        elif key == "STARTPROPERTIES":
            require(not properties and property_count is None, f"{path}:{number}: properties repeated")
            property_count = int(value)
            properties = True
        elif key == "ENDPROPERTIES":
            require(properties and len(font["properties"]) == property_count,
                    f"{path}:{number}: property count mismatch")
            properties = False
        elif properties:
            font["properties"].append([key, unquote(value)])
        elif key == "CHARS":
            require(chars is None, f"{path}:{number}: repeated CHARS")
            chars = int(value)
        elif key == "ENDFONT":
            ended = True
        else:
            font["headers"].append([key, value])
    require(glyph is None and not properties and ended and chars == len(font["glyphs"]),
            f"{path}: CHARS/STARTCHAR count or complete-file mismatch")
    require(font["headers"] and font["headers"][0] == ["STARTFONT", "2.1"], f"{path}: unexpected BDF version")
    font["chars"] = chars
    return font


def scalar_records(font):
    records = {}
    for index, glyph in enumerate(font["glyphs"]):
        encoding = glyph["fields"]["ENCODING"]
        require(len(encoding) == 1 and 0 <= encoding[0] <= 0x10FFFF and not 0xD800 <= encoding[0] <= 0xDFFF,
                f"workload contains unencoded/non-scalar/alternate record {index}: {encoding}")
        cp = encoding[0]
        require(cp not in records, f"physical duplicate ENCODING U+{cp:04X} (dictionary comparison would hide it)")
        require(all(len(glyph["fields"].get(key, [])) == size for key, size in (("SWIDTH", 2), ("DWIDTH", 2), ("BBX", 4))),
                f"missing complete metrics for U+{cp:04X}")
        records[cp] = (index, glyph)
    return records


def property_value(font, key, default=None):
    values = [value for name, value in font["properties"] if name == key]
    require(len(values) <= 1, f"ambiguous duplicate property {key}")
    return values[0] if values else default


def glyph_signature(glyph):
    # All source glyph data/metrics are compared. Hex case alone is irrelevant.
    return {"name": glyph["name"], "fields": glyph["fields"], "pixels": glyph["pixels"],
            "rows": [row.upper() for row in glyph["rows"]]}


def ranges(points):
    result = []
    for cp in sorted(points):
        if result and cp == result[-1][1] + 1:
            result[-1][1] = cp
        else:
            result.append([cp, cp])
    return result


def merge_ranges(items):
    result = []
    for start, end in sorted(items, key=lambda item: (item[0], item[1])):
        if result and start <= result[-1][1] + 1:
            result[-1][1] = max(result[-1][1], end)
        else:
            result.append([start, end])
    return result


def subtract_ranges(domain, removed):
    result = []
    for start, end in domain:
        cursor = start
        for low, high in removed:
            if high < cursor:
                continue
            if low > end:
                break
            if cursor < low:
                result.append([cursor, low - 1])
            cursor = max(cursor, high + 1)
        if cursor <= end:
            result.append([cursor, end])
    return result


def contains(intervals, cp):
    return any(start <= cp <= end for start, end in intervals)


class Policy:
    def __init__(self, path):
        require(digest(path) == BLOCKS_SHA256, "Unicode Blocks input differs from pinned Unicode 17 file")
        self.blocks = []
        for line in Path(path).read_text().splitlines():
            line = line.split("#", 1)[0].strip()
            if not line:
                continue
            span, name = line.split(";")
            start, end = (int(x, 16) for x in span.strip().split(".."))
            self.blocks.append((start, end, name.strip()))
        require(CJK_BLOCKS <= {name for _, _, name in self.blocks}, "required CJK blocks absent")
        self.starts = [start for start, _, _ in self.blocks]
        self.cjk = merge_ranges((start, end) for start, end, name in self.blocks if name in CJK_BLOCKS)
        self.western = subtract_ranges(ALL_SCALARS, self.cjk)
        self.domains = {"all": ALL_SCALARS, "cjk": self.cjk, "western": self.western,
                        "cjk,geometric": merge_ranges([*self.cjk, *GEOMETRIC]),
                        "western,-geometric": subtract_ranges(self.western, GEOMETRIC)}

    def is_cjk(self, cp):
        index = bisect_right(self.starts, cp) - 1
        return index >= 0 and cp <= self.blocks[index][1] and self.blocks[index][2] in CJK_BLOCKS

    def is_han(self, cp):
        index = bisect_right(self.starts, cp) - 1
        return index >= 0 and cp <= self.blocks[index][1] and self.blocks[index][2].startswith(
            ("CJK Unified Ideographs", "CJK Compatibility Ideographs"))

    def missing(self, selection, points):
        missing = subtract_ranges(self.domains[selection], ranges(points))
        counts = {}
        for start, end in missing:
            cursor = start
            while cursor <= end:
                index = bisect_right(self.starts, cursor) - 1
                if index >= 0 and cursor <= self.blocks[index][1]:
                    stop, name = min(end, self.blocks[index][1]), self.blocks[index][2]
                else:
                    following = index + 1
                    stop = min(end, self.starts[following] - 1) if following < len(self.blocks) else end
                    name = "No_Block"
                entry = counts.setdefault(name, {"block": name, "first": cursor, "last": stop, "count": 0})
                entry["last"] = stop
                entry["count"] += stop - cursor + 1
                cursor = stop + 1
        return ([{"start": a, "end": b} for a, b in missing],
                sorted(counts.values(), key=lambda row: row["first"]), sum(b - a + 1 for a, b in missing))


def selection_plan(inputs, selections, policy, fallback=False):
    lookups = [scalar_records(font) for font in inputs]
    selected = {}
    for cp in sorted(set().union(*(set(item) for item in lookups))):
        candidates = [index for index, selection in enumerate(selections)
                      if contains(policy.domains[selection], cp)]
        part = next((index for index in candidates if cp in lookups[index]), None)
        used_fallback = part is None
        if part is None and fallback and candidates:
            part = next(index for index, records in enumerate(lookups) if cp in records)
        if part is not None:
            index, glyph = lookups[part][cp]
            selected[cp] = {"part": part, "glyph_index": index, "fallback": used_fallback,
                            "glyph": deepcopy(glyph)}
    return selected


def round_ratio(numerator, denominator):
    require(denominator > 0, "positive source advance required")
    sign = -1 if numerator < 0 else 1
    quotient, remainder = divmod(abs(numerator), denominator)
    return sign * (quotient + (2 * remainder >= denominator))


def spacing_plan(selected, mode, policy, geometric="half"):
    """Independent documented Tong-width (14/7) transformations, full records."""
    adjustments, diagnostic_counts = [], Counter()
    if mode != "mono":
        return adjustments, diagnostic_counts
    han = [row["glyph"]["fields"]["DWIDTH"][0] for cp, row in selected.items()
           if not row["fallback"] and policy.is_han(cp)]
    require(han and Counter(han).most_common(1)[0][0] == 14 and all(width == 14 for width in han),
            "Tong selected Han width fixture changed")
    for cp, row in selected.items():
        if policy.is_cjk(cp) or geometric == "full" and contains(GEOMETRIC, cp):
            continue
        glyph = row["glyph"]
        fields = glyph["fields"]
        advance, vertical = fields["DWIDTH"]
        require(advance >= 0 and vertical == 0, "Tong has unexpected negative/vertical advance")
        if advance == 7:
            continue
        item = {"part_index": row["part"], "glyph_index": row["glyph_index"], "codepoints": [cp],
                "original_advance": advance, "target_advance": 7, "left_padding": 0, "right_padding": 0}
        if advance > 7:
            item["kind"] = "misaligned"
        else:
            item["kind"] = "padded"
            left = (7 - advance) // 2
            item.update(left_padding=left, right_padding=7 - advance - left)
            fields["BBX"][2] += left
            fields["DWIDTH"][0] = 7
            if advance == 0:
                diagnostic_counts["merge.metrics.scalable_zero_advance"] += 1
            else:
                numerator = fields["SWIDTH"][0] * 7
                fields["SWIDTH"][0] = round_ratio(numerator, advance)
                if numerator % advance:
                    diagnostic_counts["merge.metrics.scalable_rounding"] += 1
        adjustments.append(item)
    adjustments.sort(key=lambda item: (item["part_index"], item["glyph_index"]))
    return adjustments, diagnostic_counts


def expected_license(font):
    copyright_text = property_value(font, "COPYRIGHT")
    license_text = property_value(font, "LICENSE")
    # These frozen fixtures expose only the Tong exact OFL title, synthetic MIT,
    # or Unifont's mixed prose notice (not a recognized exact identifier).
    parts = [part.strip().lower().rstrip(".") for text in (license_text, copyright_text) if text
             for part in text.replace("\n", ";").split(";")]
    identifier = "OFL-1.1" if "sil open font license 1.1" in parts else "MIT" if "mit license" in parts else None
    reserved = []
    for text in (copyright_text, license_text):
        for part in (text or "").replace("\n", ";").split(";"):
            if part.strip().startswith("Reserved Font Names "):
                reserved.extend(value.strip() for value in part.strip()[20:].split(","))
    return {"copyright": copyright_text, "license": license_text or identifier,
            "license_url": property_value(font, "LICENSE_URL"), "reserved_names": reserved,
            "modification": "allowed" if identifier else "unknown",
            "provenance": f"exact standard identifier/title: {identifier}" if identifier else
            "no recognized explicit license/permission declaration"}


def compare_value(errors, field, actual, expected):
    if actual != expected:
        errors.append({"field": field, "actual": actual, "expected": expected})


def verify_report(report, inputs, selections, expected, adjustments, diagnostics, policy,
                  family, requested, effective, geometric, native, native_path):
    errors = []
    merge = report.get("merge", {})
    for key, value in {"family": family, "unicode_version": "17.0.0", "requested_spacing": requested,
                       "effective_spacing": effective, "geometric_width": geometric,
                       "full_width": 14 if effective == "mono" else None}.items():
        compare_value(errors, "merge." + key, merge.get(key), value)
    compare_value(errors, "family", report.get("family"), family)
    compare_value(errors, "glyphs", report.get("glyphs"), len(expected))
    parts = merge.get("parts", [])
    licenses = merge.get("licenses", [])
    compare_value(errors, "parts.length", len(parts), len(inputs))
    compare_value(errors, "licenses.length", len(licenses), len(inputs))
    labels = [property_value(font, "FAMILY_NAME") for font in inputs]
    require(all(labels), "fixture requires explicit component family names")
    expected_licenses = []
    metrics = merge.get("metrics", {})
    ascent = max(int(property_value(font, "FONT_ASCENT")) for font in inputs)
    descent = max(int(property_value(font, "FONT_DESCENT")) for font in inputs)
    for key, value in {"natural_ascent": ascent, "natural_descent": descent, "ascent": ascent, "descent": descent,
                       "ascent_overridden": False, "descent_overridden": False}.items():
        compare_value(errors, "metrics." + key, metrics.get(key), value)
    metric_parts = [{"part_index": index, "original_ascent": int(property_value(font, "FONT_ASCENT")),
                     "original_descent": int(property_value(font, "FONT_DESCENT")),
                     "shifted_ascent": int(property_value(font, "FONT_ASCENT")),
                     "shifted_descent": int(property_value(font, "FONT_DESCENT"))} for index, font in enumerate(inputs)]
    compare_value(errors, "metrics.parts", metrics.get("parts"), metric_parts)
    for index, (font, selection) in enumerate(zip(inputs, selections, strict=True)):
        points = scalar_records(font)
        missing, blocks, count = policy.missing(selection, points)
        contribution = sum(row["part"] == index for row in expected.values())
        part_expected = {"index": index, "label": labels[index], "selection": selection, "dy": 0,
                         "taken_glyphs": contribution, "taken_codepoints": contribution, "taken_sequences": 0,
                         "fallback_codepoints": sum(row["part"] == index and row["fallback"] for row in expected.values()),
                         "fallback_sequences": 0, "selected_missing": count, "missing_ranges": missing,
                         "missing_blocks": blocks, "omitted_legacy_mappings": 0,
                         "omitted_invalid_mappings": 0, "omitted_unencoded_glyphs": 0}
        if index < len(parts):
            compare_value(errors, f"parts[{index}]", parts[index], part_expected)
        license_record = dict(part_index=index, label=labels[index], **expected_license(font))
        expected_licenses.append(license_record)
        if index < len(licenses):
            compare_value(errors, f"licenses[{index}]", licenses[index], license_record)
    actual_adjustments = merge.get("adjustments", [])
    compare_value(errors, "adjustments.length", len(actual_adjustments), len(adjustments))
    for index, (actual, wanted) in enumerate(zip(actual_adjustments, adjustments)):
        compare_value(errors, f"adjustments[{index}]", {key: actual.get(key) for key in wanted}, wanted)
        if not actual.get("reason"):
            errors.append({"field": f"adjustments[{index}].reason", "expected": "nonempty reason"})
    diagnostic_entries = merge.get("diagnostics", {}).get("entries", [])
    actual_codes = Counter(entry.get("code") for entry in diagnostic_entries)
    for code in ("merge.metrics.scalable_zero_advance", "merge.metrics.scalable_rounding"):
        compare_value(errors, "diagnostics." + code, actual_codes[code], diagnostics[code])
    compare_value(errors, "diagnostics.merge.license.unknown", actual_codes["merge.license.unknown"],
                  sum(row["modification"] == "unknown" for row in expected_licenses))
    if any(entry.get("level") == "error" for entry in diagnostic_entries):
        errors.append({"field": "diagnostics", "expected": "no successful merge error diagnostics"})
    postscript = "".join(c for c in family if c.isascii() and (c.isalnum() or c in "-_")) or "FontpixelMerged"
    for key, value in {"FAMILY_NAME": family, "STYLE_NAME": "Regular", "FACE_NAME": family,
                       "POSTSCRIPT_NAME": postscript, "FONT_ASCENT": str(ascent), "FONT_DESCENT": str(descent),
                       "SPACING": "M" if effective == "mono" else "P"}.items():
        compare_value(errors, "bdf." + key, property_value(native, key), value)
    boxes = [row["glyph"]["fields"]["BBX"] for row in expected.values()]
    left = min([0, *(box[2] for box in boxes)])
    bottom = min([0, *(box[3] for box in boxes)])
    right = max([0, *(box[2] + box[0] for box in boxes)])
    top = max([0, *(box[3] + box[1] for box in boxes)])
    headers = dict(native["headers"])
    compare_value(errors, "bdf.FONTBOUNDINGBOX", headers.get("FONTBOUNDINGBOX"),
                  f"{right - left} {top - bottom} {left} {bottom}")
    compare_value(errors, "bdf.SIZE", headers.get("SIZE"), dict(inputs[0]["headers"]).get("SIZE"))
    header = "Merged by fontpixel; parts: " + ", ".join(labels)
    expected_comments = []
    for key, field in (("COPYRIGHT", "copyright"), ("LICENSE", "license"), ("LICENSE_URL", "license_url")):
        if key == "LICENSE_URL" and not any(row[field] for row in expected_licenses):
            continue
        lines = [header] + [f"[{row['label']}] {row[field] or 'Unknown'}" for row in expected_licenses]
        expected_comments.extend(lines)
        compare_value(errors, "bdf." + key, property_value(native, key), " | ".join(lines))
    # Full ordered notice lines are required, including repeated headers; a set
    # comparison would hide erased attribution from a component.
    compare_value(errors, "bdf.complete_notice_comments", native["comments"], expected_comments)
    outputs = report.get("outputs", [])
    compare_value(errors, "outputs.length", len(outputs), 1)
    if outputs:
        out = outputs[0]
        compare_value(errors, "outputs[0].format", out.get("format"), "bdf")
        compare_value(errors, "outputs[0].output_name", Path(out.get("output", "")).name, Path(native_path).name)
        compare_value(errors, "outputs[0].bytes", out.get("bytes"), Path(native_path).stat().st_size)
        compare_value(errors, "outputs[0].assets", out.get("assets"),
                      [{"path": Path(native_path).name, "bytes": Path(native_path).stat().st_size}])
        entries = out.get("report", {}).get("entries", [])
        # Combined output report must retain every merge diagnostic in order;
        # read diagnostics can precede it and writer diagnostics follow it.
        found = any(entries[start:start + len(diagnostic_entries)] == diagnostic_entries
                    for start in range(len(entries) - len(diagnostic_entries) + 1))
        if not found:
            errors.append({"field": "outputs[0].report", "expected": "all ordered merge diagnostics"})
        compare_value(errors, "bdf.notice_loss_reports",
                      sum(entry.get("code") == "bdf.write.notice_line_breaks" for entry in entries),
                      2 + int(any(row["license_url"] for row in expected_licenses)))
        if any(entry.get("level") == "error" for entry in entries):
            errors.append({"field": "outputs[0].report", "expected": "no successful output error diagnostics"})
    return errors


def compare_glyphs(expected, native):
    actual = scalar_records(native)
    errors = []
    for cp in sorted(set(expected) | set(actual)):
        wanted = glyph_signature(expected[cp]["glyph"]) if cp in expected else None
        got = glyph_signature(actual[cp][1]) if cp in actual else None
        if wanted != got:
            errors.append({"codepoint": cp, "expected_source": {key: expected[cp][key] for key in
                           ("part", "glyph_index", "fallback")} if cp in expected else None,
                           "expected": wanted, "actual": got})
    return errors


def original_differences(expected, original, inputs, provenance, latin_provenance, policy, sample):
    """Compare the WHOLE original font; attach explanations to exact records."""
    reference = scalar_records(original)
    sources = [scalar_records(font) for font in inputs]
    differences = []
    for cp in sorted(set(expected) | set(reference)):
        actual = expected.get(cp)
        wanted = glyph_signature(reference[cp][1]) if cp in reference else None
        got = glyph_signature(actual["glyph"]) if actual else None
        if wanted == got:
            continue
        reasons = []
        if actual is None:
            require(cp in sources[0] and not policy.is_cjk(cp) and cp not in sources[1],
                    f"unexplained original missing glyph U+{cp:04X}")
            reasons.append("regional_source_present_but_cjk_selection_excludes_and_latin_absent")
        elif cp not in reference:
            reasons.append("latin_source_additional_codepoint")
        else:
            source = sources[actual["part"]][cp][1]
            if glyph_signature(source) != got:
                adv = source["fields"]["DWIDTH"][0]
                reasons.append("mono_zero_advance_padding" if adv == 0 else "mono_narrow_advance_padding")
            if glyph_signature(source) != wanted:
                if actual["part"] == 0:
                    require(sample, f"baseline regional source changed for U+{cp:04X}")
                    reasons.append("regional_sample_source_revision")
                elif contains(GEOMETRIC, cp):
                    reasons.append("original_regional_geometry_vs_latin_halfwidth")
                else:
                    reasons.append("original_regional_and_latin_build_precedence")
            if actual["fallback"]:
                reasons.append("explicit_merge_fallback")
        require(reasons, f"unclassified original discrepancy U+{cp:04X}")
        differences.append({"codepoint": cp, "categories": reasons, "original": wanted, "merged_expected": got,
                            "original_source": provenance.get(str(cp)),
                            "latin_source": latin_provenance.get(str(cp)),
                            "expected_source": {key: actual[key] for key in ("part", "glyph_index", "fallback")} if actual else None,
                            "regional_present": cp in sources[0], "latin_present": cp in sources[1],
                            "category_cjk": policy.is_cjk(cp)})
    return differences


def capture(command, directory):
    directory = Path(directory)
    save(directory / "command.json", {"argv": list(map(str, command)), "cwd": str(directory),
                                      "env": {"PYTHONDONTWRITEBYTECODE": "1"}})
    with (directory / "stdout").open("xb") as stdout, (directory / "stderr").open("xb") as stderr:
        result = subprocess.run(list(map(str, command)), cwd=directory, stdout=stdout, stderr=stderr,
                                env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"}, check=False)
    save(directory / "process.json", {"returncode": result.returncode,
                                      "stdout_sha256": digest(directory / "stdout"),
                                      "stderr_sha256": digest(directory / "stderr")})
    return result.returncode


def rejection_outcome(code, directory, sentinel, expected_exit, message=None, diagnostic=None):
    """Require the expected structured error and ordinary-error rollback."""
    try:
        error = load(Path(directory) / "stderr")
    except (ValueError, OSError) as failure:
        error = {"invalid_error_json": str(failure)}
    detail = error.get("error", {})
    entries = (error.get("report") or {}).get("entries", [])
    preserved = sentinel.is_file() and sentinel.read_bytes() == b"existing-output-sentinel\n"
    correct = (code == expected_exit and detail.get("exit_code") == expected_exit and preserved
               and (message is None or message in detail.get("message", ""))
               and (diagnostic is None or any(entry.get("code") == diagnostic for entry in entries)))
    return {"pass": correct, "returncode": code, "expected_exit": expected_exit,
            "expected_message": message, "expected_diagnostic": diagnostic,
            "existing_output_preserved": preserved, "error_json": error}


def stem(size, spacing, region):
    return "TongPixelSans" + ("Mono" if spacing == "mono" else "") + "14" + size + "-" + region


def prepare(args):
    baseline, original_bdf, samples = (Path(getattr(args, key)).resolve() for key in ("baseline", "bdf", "samples"))
    work = fresh_directory(args.work, (baseline, original_bdf, samples))
    before = {"baseline": inventory(baseline), "bdf": inventory(original_bdf), "samples": inventory(samples)}
    save(work / "protected-before.json", before)
    require(digest(baseline / "tools/build.py") == BUILD_SHA256, "original Tong builder identity differs")
    shutil.copytree(baseline, work / "source")
    require(inventory(work / "source") == before["baseline"], "copied source hash mismatch")
    (work / "bdf").mkdir()
    logs = fresh_directory(work / "build-log")
    code = capture([sys.executable, "-B", work / "source/tools/build.py", work / "bdf"], logs)
    require(code == 0, "original builder failed; raw logs retained")
    names = [stem(size, spacing, region) + ".bdf" for size in SIZES for spacing in SPACINGS
             for region in (*REGIONS, "Latin")]
    require(sorted(path.name for path in (work / "bdf").iterdir()) == sorted(names), "builder output inventory mismatch")
    for name in names:
        require(digest(work / "bdf" / name) == digest(original_bdf / name), f"original rebuilt BDF differs: {name}")
        font = parse_bdf(work / "bdf" / name)
        require(len(scalar_records(font)) == (3645 if name.endswith("-Latin.bdf") else 38653),
                f"complete original workload count changed: {name}")
    # Execute only the hash-verified writable copy, never protected originals.
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("w66_original_tong", work / "source/tools/build.py")
    require(spec is not None and spec.loader is not None, "cannot load copied original builder")
    build = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(build)
    (work / "provenance").mkdir()
    (work / "samples").mkdir()
    for path in samples.glob("*.bdf"):
        shutil.copyfile(path, work / "samples" / path.name)
    cases = []
    for size in SIZES:
        latin_faces = build.latin_faces(root=work / "source", size=size)
        regional_faces = {region: (prop, mono) for region, prop, mono in build.faces(root=work / "source", size=size)}
        for spacing_index, spacing in enumerate(SPACINGS):
            for region in (*REGIONS, "Latin"):
                face = latin_faces[spacing_index] if region == "Latin" else regional_faces[region][spacing_index]
                provenance = {str(cp): {"src": glyph.get("src"), "state": glyph.get("state"),
                                        "unicode_category": unicodedata.category(chr(cp)),
                                        "borrowed_region": bool(glyph.get("src") and glyph["src"].split(".")[-1] in REGIONS
                                                                and glyph["src"].split(".")[-1] != region)}
                              for cp, glyph in sorted(face.items())}
                save(work / "provenance" / (stem(size, spacing, region) + ".json.gz"), provenance)
            for region in REGIONS:
                cases.append({"id": f"baseline-{size}-{spacing}-{region}", "size": size, "spacing": spacing,
                              "region": region, "vintage": "baseline", "regional": "bdf/" + stem(size, spacing, region) + ".bdf",
                              "latin": "bdf/" + stem(size, spacing, "Latin") + ".bdf",
                              "original": "bdf/" + stem(size, spacing, region) + ".bdf"})
    identities = []
    for path in sorted((work / "samples").glob("*.bdf")):
        counterpart = work / "bdf" / path.name
        sample, original = parse_bdf(path), parse_bdf(counterpart)
        sample_records, original_records = scalar_records(sample), scalar_records(original)
        changes = [{"codepoint": cp, "sample": glyph_signature(sample_records[cp][1]) if cp in sample_records else None,
                    "original": glyph_signature(original_records[cp][1]) if cp in original_records else None}
                   for cp in sorted(set(sample_records) | set(original_records))
                   if cp not in sample_records or cp not in original_records
                   or glyph_signature(sample_records[cp][1]) != glyph_signature(original_records[cp][1])]
        change_path = "provenance/sample-" + path.stem + ".json.gz"
        save(work / change_path, {"all_glyph_differences": changes,
                                 "sample_headers": {k: sample[k] for k in ("headers", "properties", "comments")},
                                 "original_headers": {k: original[k] for k in ("headers", "properties", "comments")}})
        identities.append({"name": path.name, "sample_sha256": digest(path), "original_sha256": digest(counterpart),
                           "glyph_differences": len(changes), "complete_comparison": change_path})
    for region in REGIONS:
        cases.append({"id": "sample-Large-prop-" + region, "size": "Large", "spacing": "prop", "region": region,
                      "vintage": "sample", "regional": "samples/" + stem("Large", "prop", region) + ".bdf",
                      "latin": "samples/" + stem("Large", "prop", "Latin") + ".bdf",
                      "original": "bdf/" + stem("Large", "prop", region) + ".bdf"})
    require(len(cases) == 20, "16 baseline plus four regional sample cases required")
    after = {"baseline": inventory(baseline), "bdf": inventory(original_bdf), "samples": inventory(samples)}
    save(work / "protected-after.json", after)
    require(before == after, "protected input tree changed during preparation")
    require(inventory(work / "source") == before["baseline"], "reference execution mutated copied source")
    plan = {"schema": 1, "status": "reference-prepared-native-pending", "root": str(work),
            "python": sys.version, "unicode_database": unicodedata.unidata_version, "builder_sha256": BUILD_SHA256,
            "blocks": "source/build-data/coverage/ucd/Blocks.txt", "cases": cases, "sample_identities": identities,
            "artifacts": inventory(work)}
    save(work / "plan.json", plan)
    print(json.dumps({"status": plan["status"], "cases": len(cases), "plan": str(work / "plan.json")}))


def validate_case(case, profile, root, binary, destination, policy):
    inputs = [parse_bdf(root / case[key]) for key in ("regional", "latin")]
    original = parse_bdf(root / case["original"])
    selections = ["cjk,geometric", "western,-geometric"] if profile == "regional-geometry" else ["cjk", "western"]
    fallback = profile == "fallback"
    requested = "auto" if profile == "auto" else case["spacing"]
    effective = "prop" if requested == "auto" else requested
    require(all(property_value(font, "SPACING") == "P" for font in inputs), "original SPACING P identity changed")
    geometric = "full" if profile == "regional-geometry" else "half"
    expected = selection_plan(inputs, selections, policy, fallback)
    adjustments, diagnostics = spacing_plan(expected, effective, policy, geometric)
    family = "Merge Audit " + case["id"] + " " + profile
    native = destination / "merged.bdf"
    command = [binary, "merge", "--family", family]
    for key, selection in zip(("regional", "latin"), selections, strict=True):
        command += ["--part", root / case[key], "--take", selection]
    command += ["--spacing", requested, "--geometric-width", geometric, "-o", native, "--report", "json"]
    if fallback:
        command.append("--fallback")
    negatives = []
    if effective == "mono":
        # Original mono permits >7 western widths; default merge must reject
        # these with the precise spacing error and preserve an existing output.
        require(any(item["kind"] == "misaligned" for item in adjustments), "expected Tong mono-wide fixture disappeared")
        rejection = fresh_directory(destination / "without-allow-misaligned")
        sentinel = rejection / "merged.bdf"
        sentinel.write_bytes(b"existing-output-sentinel\n")
        denied = [sentinel if item == native else item for item in command]
        code = capture(denied, rejection)
        negatives.append({"case": "mono-default-rejection", **rejection_outcome(
            code, rejection, sentinel, 1, message="Western advance exceeds halfwidth target")})
        command.append("--allow-misaligned")
    elif profile == "category":
        # Recognized original Tong OFL declarations permit merge; BDF's
        # multiline notice projection still must fail strict before commit.
        rejection = fresh_directory(destination / "strict-notice-rejection")
        sentinel = rejection / "merged.bdf"
        sentinel.write_bytes(b"existing-output-sentinel\n")
        denied = [sentinel if item == native else item for item in command] + ["--strict"]
        code = capture(denied, rejection)
        negatives.append({"case": "strict-notice-rejection", **rejection_outcome(
            code, rejection, sentinel, 3, diagnostic="bdf.write.notice_line_breaks")})
    code = capture(command, destination)
    result = {"id": case["id"], "profile": profile, "returncode": code,
              "native_contract_pass": False, "negative_checks": negatives, "source": case,
              "default_resource_limits": True}
    if code != 0:
        result["failure"] = "native command failed; never treated as a policy difference"
        return result
    require(native.is_file(), "successful native command produced no BDF")
    font, report = parse_bdf(native), load(destination / "stdout")
    glyph_errors = compare_glyphs(expected, font)
    report_errors = verify_report(report, inputs, selections, expected, adjustments, diagnostics, policy,
                                  family, requested, effective, geometric, font, native)
    provenance = load(root / "provenance" / (stem(case["size"], case["spacing"], case["region"]) + ".json.gz"))
    latin_provenance = load(root / "provenance" / (stem(case["size"], case["spacing"], "Latin") + ".json.gz"))
    differences = original_differences(expected, original, inputs, provenance, latin_provenance, policy,
                                       case["vintage"] == "sample")
    save(destination / "native-contract-errors.json.gz", {"glyphs": glyph_errors, "report": report_errors})
    save(destination / "original-complete-differences.json.gz", differences)
    save(destination / "all-glyph-source-map.json.gz",
         [{"codepoint": cp, "part": row["part"], "glyph_index": row["glyph_index"], "fallback": row["fallback"],
           "original_source": provenance.get(str(cp)), "latin_source": latin_provenance.get(str(cp))}
          for cp, row in expected.items()])
    result.update(native_contract_pass=not glyph_errors and not report_errors and all(row["pass"] for row in negatives),
                  native_glyphs=font["chars"], expected_glyphs=len(expected), original_glyphs=original["chars"],
                  native_glyph_errors=len(glyph_errors), report_errors=len(report_errors),
                  original_exact_glyph_equivalence=not differences, original_differences=len(differences),
                  original_difference_categories=dict(Counter(category for row in differences for category in row["categories"])),
                  original_metadata={key: original[key] for key in ("headers", "properties", "comments")},
                  merged_metadata={key: font[key] for key in ("headers", "properties", "comments")},
                  native_sha256=digest(native))
    return result


def run(args):
    plan = load(args.plan)
    require(plan["schema"] == 1 and len(plan["cases"]) == 20, "complete prepared plan required")
    root = Path(plan["root"])
    for name, identity in plan["artifacts"].items():
        require((root / name).stat().st_size == identity["bytes"] and digest(root / name) == identity["sha256"],
                f"prepared artifact changed: {name}")
    destination = fresh_directory(args.output, (root,))
    binary = Path(args.fontpixel).resolve()
    before_binary = digest(binary)
    policy = Policy(root / plan["blocks"])
    results = []
    for case in plan["cases"]:
        for profile in args.profiles:
            directory = fresh_directory(destination / (case["id"] + "-" + profile))
            try:
                result = validate_case(case, profile, root, binary, directory, policy)
            except Exception as error:
                result = {"id": case["id"], "profile": profile, "native_contract_pass": False,
                          "failure": type(error).__name__ + ": " + str(error)}
            save(directory / "result.json", result)
            results.append(result)
            print(json.dumps({key: result.get(key) for key in ("id", "profile", "native_contract_pass", "failure")}), flush=True)
    require(digest(binary) == before_binary, "native binary changed during validation")
    for name, identity in plan["artifacts"].items():
        require(digest(root / name) == identity["sha256"], f"prepared source changed during native validation: {name}")
    passed = all(result["native_contract_pass"] for result in results)
    save(destination / "summary.json", {"schema": 1, "status": "pass" if passed else "fail",
                                       "scope": "full native contract; original policy differences retained without exemption",
                                       "plan_sha256": digest(args.plan), "binary_sha256": before_binary,
                                       "profiles": args.profiles, "results": results,
                                       "artifacts": inventory(destination)})
    return 0 if passed else 1


def unifont(args):
    output = Path(args.output)
    require(not protected(output), "cannot write proof into protected input tree")
    inputs = [parse_bdf(args.latin), parse_bdf(args.unifont)]
    native = parse_bdf(args.native)
    policy = Policy(args.blocks)
    selections = ["western", "all"]
    expected = selection_plan(inputs, selections, policy)
    require([font["chars"] for font in inputs] == [3645, 57086] and len(expected) == 57106,
            "full frozen Unifont/Latin workload identity/count changed")
    require(Counter(row["part"] for row in expected.values()) == {0: 3645, 1: 53461},
            "genuine western-first selected contribution changed")
    glyph_errors = compare_glyphs(expected, native)
    report = load(args.report)
    report_errors = verify_report(report, inputs, selections, expected, [], Counter(), policy,
                                  "Unifont Western Mix", "prop", "prop", "half", native, args.native)
    independent = {"physical_records": True, "parser": "strict independent standard-library BDF parser"}
    if args.reference:
        reference = Path(args.reference).resolve()
        require(not protected(reference), "execute bdfparser only from a local-work copy")
        source = reference / "src/bdfparser/__init__.py"
        require(source.is_file(), "copied bdfparser source path is missing")
        sys.dont_write_bytecode = True
        spec = importlib.util.spec_from_file_location("w66_bdfparser", source,
                                                     submodule_search_locations=[str(source.parent)])
        require(spec is not None and spec.loader is not None, "cannot load copied bdfparser package")
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        a, b, merged = (module.Font(str(path)) for path in (args.latin, args.unifont, args.native))
        selected = {cp: a.glyphs[cp] if row["part"] == 0 else b.glyphs[cp] for cp, row in expected.items()}
        if merged.glyphs != selected:
            report_errors.append({"field": "independent_bdfparser", "expected": "complete raw selected records"})
        independent["bdfparser_sha256"] = digest(source)
    result = {"schema": 1, "status": "pass" if not glyph_errors and not report_errors else "fail",
              "native_contract_pass": not glyph_errors and not report_errors, "independent": independent,
              "source_counts": [font["chars"] for font in inputs], "native_glyphs": native["chars"],
              "expected_glyphs": len(expected), "contributions": dict(Counter(row["part"] for row in expected.values())),
              "glyph_errors": glyph_errors, "report_errors": report_errors,
              "inputs": {key: {"path": str(getattr(args, key)), "sha256": digest(getattr(args, key))}
                         for key in ("latin", "unifont", "native", "report", "blocks")},
              "all_glyph_sources": [{"codepoint": cp, "part": row["part"], "glyph_index": row["glyph_index"]}
                                    for cp, row in expected.items()]}
    save(output, result)
    print(json.dumps({"status": result["status"], "glyph_errors": len(glyph_errors), "report_errors": len(report_errors)}))
    return 0 if result["native_contract_pass"] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare", help="copy and build the genuine original Tong reference")
    for key in ("baseline", "bdf", "samples", "work"):
        prep.add_argument("--" + key, type=Path, required=True)
    native = commands.add_parser("run", help="validate all 16 baseline + four separate sample cases")
    for key in ("plan", "fontpixel", "output"):
        native.add_argument("--" + key, type=Path, required=True)
    native.add_argument("--profiles", choices=PROFILES, nargs="+", default=list(PROFILES))
    union = commands.add_parser("unifont", help="complete Unifont glyph/source/report/name gate")
    for key in ("native", "report", "latin", "unifont", "blocks", "output"):
        union.add_argument("--" + key, type=Path, required=True)
    union.add_argument("--reference", type=Path, help="optional copied bdfparser repository for a second decoder")
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args)
        return 0
    return run(args) if args.command == "run" else unifont(args)


if __name__ == "__main__":
    raise SystemExit(main())
