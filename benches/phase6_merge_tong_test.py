#!/usr/bin/env python3
"""Small stdlib regressions for the W6.6 oracle; no native/reference workloads."""
from collections import Counter
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest

import phase6_merge_tong as gate


BLOCKS = Path(__file__).resolve().parents[2] / "validation-inputs/phase2/tong-baseline/build-data/coverage/ucd/Blocks.txt"


def bdf(glyphs, family="Component", copyright_text="MIT License"):
    parts = ["STARTFONT 2.1", "FONT Fixture", "SIZE 14 75 75", "FONTBOUNDINGBOX 14 14 0 -3",
             "STARTPROPERTIES 5", f'FAMILY_NAME "{family}"', "FONT_ASCENT 11", "FONT_DESCENT 3",
             'SPACING "P"', f'COPYRIGHT "{copyright_text}"', "ENDPROPERTIES", f"CHARS {len(glyphs)}"]
    for cp, advance, scalable in glyphs:
        parts += [f"STARTCHAR U+{cp:04X}", f"ENCODING {cp}", f"SWIDTH {scalable} 0",
                  f"DWIDTH {advance} 0", "BBX 1 1 0 0", "BITMAP", "80", "ENDCHAR"]
    return "\n".join(parts + ["ENDFONT"]) + "\n"


class OracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.policy = gate.Policy(BLOCKS)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def font(self, name, glyphs, **kwargs):
        path = self.root / name
        path.write_text(bdf(glyphs, **kwargs))
        return gate.parse_bdf(path)

    def test_physical_duplicates_and_unencoded_never_collapse_into_dictionary(self):
        duplicate = self.font("duplicate.bdf", [(65, 7, 500), (65, 14, 1000)])
        self.assertEqual(duplicate["chars"], 2)
        with self.assertRaisesRegex(ValueError, "physical duplicate"):
            gate.scalar_records(duplicate)
        path = self.root / "unencoded.bdf"
        path.write_text(bdf([(65, 7, 500)]).replace("ENCODING 65", "ENCODING -1 65"))
        font = gate.parse_bdf(path)
        self.assertEqual(font["glyphs"][0]["fields"]["ENCODING"], [-1, 65])
        with self.assertRaisesRegex(ValueError, "unencoded"):
            gate.scalar_records(font)

    def test_declared_counts_bitmap_dimensions_and_trailing_records_fail(self):
        text = bdf([(65, 7, 500)])
        for index, altered in enumerate((text.replace("CHARS 1", "CHARS 2"),
                                         text.replace("\n80\n", "\n8000\n"),
                                         text.replace("BBX 1 1", "BBX 1 2"),
                                         text.replace("STARTPROPERTIES 5", "STARTPROPERTIES 4"),
                                         text + "COMMENT extra\n")):
            path = self.root / f"bad-{index}.bdf"
            path.write_text(altered)
            with self.assertRaises(ValueError):
                gate.parse_bdf(path)

    def test_all_glyph_fields_and_metrics_compared_independent_of_order(self):
        source = self.font("source.bdf", [(65, 7, 500), (0x4E00, 14, 1000)])
        expected = gate.selection_plan([source], ["all"], self.policy)
        native = deepcopy(source)
        native["glyphs"].reverse()
        self.assertEqual(gate.compare_glyphs(expected, native), [])
        native["glyphs"][0]["fields"]["SWIDTH"][1] = 1
        errors = gate.compare_glyphs(expected, native)
        self.assertEqual([row["codepoint"] for row in errors], [0x4E00])
        native = deepcopy(source)
        native["glyphs"][0]["fields"]["ATTRIBUTES"] = "1"
        self.assertEqual(len(gate.compare_glyphs(expected, native)), 1)

    def test_unicode_policy_uses_pinned_external_blocks_and_true_complement(self):
        for cp in (0x1100, 0x2E80, 0x4E00, 0xFE50, 0xFF61, 0x16FE0, 0x1AFF0, 0x323B0, 0x3347F):
            self.assertTrue(self.policy.is_cjk(cp), hex(cp))
        for cp in (0x02CA, 0x212B, 0x2500, 0x2B05, 0x4DC0, 0xFE20, 0xE0B0, 0x33480):
            self.assertFalse(self.policy.is_cjk(cp), hex(cp))
        self.assertFalse(gate.contains(self.policy.western, 0xD800))
        self.assertFalse(gate.contains(self.policy.western, 0x110000))
        self.assertEqual(sum(b - a + 1 for a, b in self.policy.cjk + self.policy.western), 0x110000 - 0x800)
        path = self.root / "Blocks.txt"
        path.write_text(BLOCKS.read_text() + "\n# changed\n")
        with self.assertRaisesRegex(ValueError, "pinned"):
            gate.Policy(path)

    def test_missing_ranges_cover_unassigned_positions_and_exact_blocks(self):
        missing, blocks, total = self.policy.missing("all", {0, 2, 0x10FFFF})
        self.assertEqual(missing, [{"start": 1, "end": 1}, {"start": 3, "end": 0xD7FF},
                                   {"start": 0xE000, "end": 0x10FFFE}])
        self.assertEqual(total, 0x110000 - 0x800 - 3)
        self.assertEqual(sum(row["count"] for row in blocks), total)
        basic = next(row for row in blocks if row["block"] == "Basic Latin")
        self.assertEqual(basic, {"block": "Basic Latin", "first": 1, "last": 127, "count": 126})
        self.assertTrue(any(row["block"] == "No_Block" for row in blocks))

    def test_missing_region_glyph_is_not_silently_filled_until_fallback(self):
        regional = self.font("regional.bdf", [(0x4E00, 14, 1000), (0x02CA, 14, 1000), (65, 14, 1000)])
        latin = self.font("latin.bdf", [(65, 7, 500)])
        selected = gate.selection_plan([regional, latin], ["cjk", "western"], self.policy)
        self.assertEqual(set(selected), {65, 0x4E00})
        self.assertEqual(selected[65]["part"], 1)
        fallback = gate.selection_plan([regional, latin], ["cjk", "western"], self.policy, True)
        self.assertEqual(set(fallback), {65, 0x02CA, 0x4E00})
        self.assertEqual((fallback[0x02CA]["part"], fallback[0x02CA]["fallback"]), (0, True))
        differences = gate.original_differences(selected, regional, [regional, latin], {}, {}, self.policy, False)
        missing = next(row for row in differences if row["codepoint"] == 0x02CA)
        self.assertEqual(missing["categories"], ["regional_source_present_but_cjk_selection_excludes_and_latin_absent"])
        self.assertTrue(missing["regional_present"])
        self.assertFalse(missing["latin_present"])

    def test_mono_zero_wide_and_scalable_rounding_are_all_retained(self):
        regional = self.font("regional.bdf", [(0x4E00, 14, 1000)])
        latin = self.font("latin.bdf", [(65, 4, 285), (66, 0, 23), (67, 14, 1000)])
        latin["glyphs"][0]["fields"].update(VVECTOR=[2, 8], SWIDTH1=[0, 1000], DWIDTH1=[0, 14])
        selected = gate.selection_plan([regional, latin], ["cjk", "western"], self.policy)
        adjustments, diagnostics = gate.spacing_plan(selected, "mono", self.policy)
        self.assertEqual([row["kind"] for row in adjustments], ["padded", "padded", "misaligned"])
        self.assertEqual(selected[65]["glyph"]["fields"]["DWIDTH"], [7, 0])
        self.assertEqual(selected[65]["glyph"]["fields"]["SWIDTH"], [499, 0])
        self.assertEqual(selected[65]["glyph"]["fields"]["BBX"], [1, 1, 1, 0])
        self.assertEqual(selected[65]["glyph"]["fields"]["VVECTOR"], [2, 8])
        self.assertEqual(selected[65]["glyph"]["fields"]["SWIDTH1"], [0, 1000])
        self.assertEqual(selected[65]["glyph"]["fields"]["DWIDTH1"], [0, 14])
        self.assertEqual(selected[66]["glyph"]["fields"]["SWIDTH"], [23, 0])
        self.assertEqual(selected[66]["glyph"]["fields"]["BBX"], [1, 1, 3, 0])
        self.assertEqual(selected[67]["glyph"]["fields"]["DWIDTH"], [14, 0])
        self.assertEqual(diagnostics, {"merge.metrics.scalable_rounding": 1, "merge.metrics.scalable_zero_advance": 1})
        self.assertEqual(gate.round_ratio(-15, 2), -8)
        self.assertEqual(gate.round_ratio(15, 2), 8)

    def test_full_geometry_preserves_provided_metrics_without_synthesis(self):
        regional = self.font("regional.bdf", [(0x4E00, 14, 1000), (0x2500, 14, 1000)])
        latin = self.font("latin.bdf", [(0x2500, 2, 142)])
        half_source = gate.selection_plan([regional, latin], ["cjk", "western"], self.policy)
        adjustments, _ = gate.spacing_plan(half_source, "mono", self.policy, "full")
        self.assertEqual(adjustments, [])
        self.assertEqual(half_source[0x2500]["glyph"]["fields"]["DWIDTH"], [2, 0])
        regional_source = gate.selection_plan([regional, latin], ["cjk,geometric", "western,-geometric"], self.policy)
        gate.spacing_plan(regional_source, "mono", self.policy, "full")
        self.assertEqual(regional_source[0x2500]["part"], 0)
        self.assertEqual(regional_source[0x2500]["glyph"]["fields"]["DWIDTH"], [14, 0])

    def test_source_revision_never_excuses_wrong_native_record(self):
        original = self.font("original.bdf", [(0x4E00, 14, 1000)])
        sample = self.font("sample.bdf", [(0x4E00, 14, 1000)])
        sample["glyphs"][0]["fields"]["BBX"][2] = 1
        latin = self.font("latin.bdf", [(65, 7, 500)])
        selected = gate.selection_plan([sample, latin], ["cjk", "western"], self.policy)
        changes = gate.original_differences(selected, original, [sample, latin], {}, {}, self.policy, True)
        self.assertIn("regional_sample_source_revision", next(row for row in changes if row["codepoint"] == 0x4E00)["categories"])
        native = deepcopy(sample)
        native["glyphs"].append(deepcopy(latin["glyphs"][0]))
        native["glyphs"][0]["fields"]["BBX"][2] = 2
        self.assertEqual(len(gate.compare_glyphs(selected, native)), 1)
        with self.assertRaisesRegex(ValueError, "baseline regional source changed"):
            gate.original_differences(selected, original, [sample, latin], {}, {}, self.policy, False)

    def test_complete_notice_assessment_does_not_guess_mixed_prose_license(self):
        tong = self.font("tong.bdf", [(65, 7, 500)], copyright_text="Original notice; SIL Open Font License 1.1; Reserved Font Names Source, Plangothic")
        notice = gate.expected_license(tong)
        self.assertEqual(notice["license"], "OFL-1.1")
        self.assertEqual(notice["reserved_names"], ["Source", "Plangothic"])
        mixed = self.font("mixed.bdf", [(65, 7, 500)], copyright_text="Author. License: SIL Open Font License version 1.1 and GPLv2+.")
        notice = gate.expected_license(mixed)
        self.assertEqual(notice["modification"], "unknown")
        self.assertIsNone(notice["license"])

    def test_full_report_rejects_missing_ranges_notice_loss_and_source_counts(self):
        inputs = [self.font("region.bdf", [(0x4E00, 14, 1000)], family="Region"),
                  self.font("latin.bdf", [(65, 7, 500)], family="Latin")]
        expected = gate.selection_plan(inputs, ["cjk", "western"], self.policy)
        output = self.root / "merged.bdf"
        notices = ["Merged by fontpixel; parts: Region, Latin", "[Region] MIT License", "[Latin] MIT License"]
        licenses = ["Merged by fontpixel; parts: Region, Latin", "[Region] MIT", "[Latin] MIT"]
        text = bdf([(0x4E00, 14, 1000), (65, 7, 500)], family="Mixed")
        text = text.replace("FONTBOUNDINGBOX 14 14 0 -3", "FONTBOUNDINGBOX 1 1 0 0")
        text = text.replace("STARTPROPERTIES 5", "STARTPROPERTIES 9")
        text = text.replace('FAMILY_NAME "Mixed"', 'FAMILY_NAME "Mixed"\nSTYLE_NAME "Regular"\nFACE_NAME "Mixed"\nPOSTSCRIPT_NAME "Mixed"')
        notice_lines = "\n".join('COMMENT "' + line + '"' for line in notices)
        license_lines = "\n".join('COMMENT "' + line + '"' for line in licenses)
        text = text.replace('COPYRIGHT "MIT License"', notice_lines + '\nCOPYRIGHT "' + " | ".join(notices)
                            + '"\n' + license_lines + '\nLICENSE "' + " | ".join(licenses) + '"')
        output.write_text(text)
        native = gate.parse_bdf(output)
        self.assertEqual(native["comments"], notices + licenses)
        report = {"family": "Mixed", "glyphs": 2, "merge": {
            "family": "Mixed", "unicode_version": "17.0.0", "requested_spacing": "prop", "effective_spacing": "prop",
            "geometric_width": "half", "full_width": None, "parts": [], "licenses": [], "adjustments": [],
            "diagnostics": {"entries": []}, "metrics": {"ascent": 11, "descent": 3, "natural_ascent": 11,
                "natural_descent": 3, "ascent_overridden": False, "descent_overridden": False,
                "parts": [{"part_index": i, "original_ascent": 11, "original_descent": 3,
                           "shifted_ascent": 11, "shifted_descent": 3} for i in range(2)]}},
            "outputs": [{"output": str(output), "format": "bdf", "bytes": output.stat().st_size,
                         "assets": [{"path": output.name, "bytes": output.stat().st_size}],
                         "report": {"entries": [{"level": "warning", "impact": "loss", "code": "bdf.write.notice_line_breaks"},
                                                {"level": "warning", "impact": "loss", "code": "bdf.write.notice_line_breaks"}]}}]}
        for index, selection in enumerate(("cjk", "western")):
            missing, blocks, count = self.policy.missing(selection, gate.scalar_records(inputs[index]))
            label = ("Region", "Latin")[index]
            report["merge"]["parts"].append({"index": index, "label": label, "selection": selection, "dy": 0,
                "taken_glyphs": 1, "taken_codepoints": 1, "taken_sequences": 0, "fallback_codepoints": 0,
                "fallback_sequences": 0, "selected_missing": count, "missing_ranges": missing, "missing_blocks": blocks,
                "omitted_legacy_mappings": 0, "omitted_invalid_mappings": 0, "omitted_unencoded_glyphs": 0})
            report["merge"]["licenses"].append({"part_index": index, "label": label, "copyright": "MIT License",
                "license": "MIT", "license_url": None, "modification": "allowed", "reserved_names": [],
                "provenance": "exact standard identifier/title: MIT"})

        def check(value, font=native):
            return gate.verify_report(value, inputs, ["cjk", "western"], expected, [], Counter(), self.policy,
                                      "Mixed", "prop", "prop", "half", font, output)

        self.assertEqual(check(report), [])
        for mutation in (lambda r: r["merge"]["parts"][0].update(taken_codepoints=2),
                         lambda r: r["merge"]["parts"][1]["missing_ranges"].pop(),
                         lambda r: r["merge"]["licenses"][0].update(copyright="lost notice"),
                         lambda r: r["outputs"][0]["report"]["entries"].pop(),
                         lambda r: r["outputs"][0].update(bytes=0)):
            altered = deepcopy(report)
            mutation(altered)
            self.assertTrue(check(altered))
        altered = deepcopy(native)
        altered["comments"].pop()
        self.assertTrue(check(report, altered))
        for key in ("FAMILY_NAME", "STYLE_NAME", "FACE_NAME", "POSTSCRIPT_NAME"):
            altered = deepcopy(native)
            next(row for row in altered["properties"] if row[0] == key)[1] = "Wrong"
            self.assertTrue(check(report, altered), key)

    def test_strict_rejection_requires_expected_exit_notice_and_preserved_output(self):
        directory = gate.fresh_directory(self.root / "strict-rejection")
        sentinel = directory / "merged.bdf"
        sentinel.write_bytes(b"existing-output-sentinel\n")
        error = {"error": {"exit_code": 3, "message": "strict loss"},
                 "report": {"entries": [{"code": "bdf.write.notice_line_breaks"}]}}
        (directory / "stderr").write_bytes(gate.json_bytes(error))

        def check(code=3):
            return gate.rejection_outcome(code, directory, sentinel, 3, diagnostic="bdf.write.notice_line_breaks")["pass"]

        self.assertTrue(check())
        self.assertFalse(check(1))
        self.assertFalse(check(-11))
        error["report"]["entries"].clear()
        (directory / "stderr").write_bytes(gate.json_bytes(error))
        self.assertFalse(check())
        error["report"]["entries"] = [{"code": "bdf.write.notice_line_breaks"}]
        (directory / "stderr").write_bytes(gate.json_bytes(error))
        sentinel.write_bytes(b"clobbered")
        self.assertFalse(check())
        sentinel.unlink()
        self.assertFalse(check())

    def test_evidence_refuses_existing_destinations_and_retains_failed_process(self):
        path = self.root / "proof.json"
        gate.save(path, {"first": True})
        with self.assertRaises(FileExistsError):
            gate.save(path, {"first": False})
        self.assertEqual(gate.load(path), {"first": True})
        with self.assertRaises(FileExistsError):
            gate.fresh_directory(self.root)
        with self.assertRaisesRegex(ValueError, "inside input"):
            gate.fresh_directory(self.root / "child", [self.root])
        with self.assertRaisesRegex(ValueError, "protected"):
            gate.fresh_directory(BLOCKS.parent / "not-created")
        directory = gate.fresh_directory(self.root / "failed-process")
        import sys
        status = gate.capture([sys.executable, "-B", "-c", "import sys; print('original failure'); sys.exit(7)"], directory)
        self.assertEqual(status, 7)
        self.assertEqual(gate.load(directory / "process.json")["returncode"], 7)
        self.assertIn(b"original failure", (directory / "stdout").read_bytes())


if __name__ == "__main__":
    unittest.main()
