#!/usr/bin/env python3
"""Field-by-field comparison of 20 native Tong OTBs against export-rs.

Uses the complete Tong inventory of phase6_tong_metadata (global tables, raw
names, cmap records, every strike/index subtable/GID, raw timestamps) and the
FreeType pixel/advance gate of phase6_validate. Every remaining difference is
listed per field and classified only by the rules below; nothing is waived:

- head.checkSumAdjustment is derived: it is accepted only when every table's
  bytes are identical with that field zeroed, the directories differ at most in
  table data offsets (physical placement order), and each file's value is the
  correct adjustment for its own bytes;
- head.created/modified are producer timestamps: equal when copied explicitly,
  otherwise reported as the export-rs wall-clock policy;
- anything else is an unresolved difference.
"""
import argparse
import hashlib
import json
from pathlib import Path
from phase6_catalogue_bitmap import differences
from phase6_tong_metadata import sfnt_inventory
from phase6_validate import bitmap_pair


def leaves(value, prefix=''):
    if isinstance(value, dict):
        out = {}
        for key, item in value.items(): out.update(leaves(item, f'{prefix}.{key}' if prefix else str(key)))
        return out
    return {prefix: value}


def zero_checksum_adjustment(data):
    data = bytearray(data)
    for i in range(int.from_bytes(data[4:6], 'big')):
        record = 12 + 16 * i
        if data[record:record + 4] == b'head':
            offset = int.from_bytes(data[record + 8:record + 12], 'big')
            data[offset + 8:offset + 12] = b'\0\0\0\0'
    return bytes(data)


def directory(data, timestamps=False):
    records = {}
    for i in range(int.from_bytes(data[4:6], 'big')):
        record = data[12 + 16 * i:28 + 16 * i]
        offset, length = int.from_bytes(record[8:12], 'big'), int.from_bytes(record[12:16], 'big')
        table = bytearray(data[offset:offset + length])
        checksum = record[4:8].hex()
        if record[:4] == b'head':
            table[8:12] = b'\0\0\0\0'
            if timestamps:
                # Producer timestamps are classified separately; the head
                # directory checksum is derived from them.
                table[20:36] = bytes(16); checksum = None
        records[bytes(record[:4])] = {'checksum': checksum, 'offset': offset, 'length': length,
                                      'data': bytes(table)}
    return data[:12], records


def checksum_adjustment_valid(data):
    head = directory(data)[1][b'head']
    zeroed = zero_checksum_adjustment(data) + b'\0' * (-len(data) % 4)
    total = sum(int.from_bytes(zeroed[i:i + 4], 'big') for i in range(0, len(zeroed), 4)) & 0xffffffff
    stored = int.from_bytes(data[head['offset'] + 8:head['offset'] + 12], 'big')
    return stored == (0xb1b0afba - total) & 0xffffffff


def compare(source, native, reference):
    a, b = sfnt_inventory(native), sfnt_inventory(reference)
    diff = differences(a, b)
    globals_a, globals_b = leaves(a['global_tables']), leaves(b['global_tables'])
    global_fields = sorted(k for k in globals_a.keys() | globals_b.keys() if globals_a.get(k) != globals_b.get(k))
    strikes = []
    for left, right in zip(a['all_physical_strikes'], b['all_physical_strikes'], strict=True):
        strikes.append({'size_fields': sorted(k for k, v in leaves(left['size_fields']).items()
                                              if leaves(right['size_fields']).get(k) != v),
                        'index_subtables_equal': left['index_subtables'] == right['index_subtables'],
                        'all_physical_glyphs_equal': left['all_physical_glyphs'] == right['all_physical_glyphs']})
    native_bytes, reference_bytes = native.read_bytes(), reference.read_bytes()
    zeroed = zero_checksum_adjustment(native_bytes) == zero_checksum_adjustment(reference_bytes)
    (header_a, tables_a), (header_b, tables_b) = directory(native_bytes), directory(reference_bytes)
    def same_except_placement(left, right):
        (header_l, tables_l), (header_r, tables_r) = left, right
        return header_l == header_r and tables_l.keys() == tables_r.keys() and all(
            {k: v for k, v in tables_l[tag].items() if k != 'offset'} == {k: v for k, v in tables_r[tag].items() if k != 'offset'}
            for tag in tables_l)
    placement_only = same_except_placement((header_a, tables_a), (header_b, tables_b))
    placement_and_timestamps = same_except_placement(directory(native_bytes, True), directory(reference_bytes, True))
    adjustments_valid = checksum_adjustment_valid(native_bytes) and checksum_adjustment_valid(reference_bytes)
    order = lambda tables: [tag.decode() for tag, _ in sorted(tables.items(), key=lambda item: item[1]['offset'])]
    classified = {}
    for field in global_fields:
        if field == 'head.checkSumAdjustment':
            if zeroed or (placement_only and adjustments_valid):
                classified[field] = ('derived: every table byte-identical with checkSumAdjustment zeroed, '
                                     'directory differs only in table data offsets, both adjustments valid')
            elif placement_and_timestamps and adjustments_valid:
                classified[field] = ('derived: every table byte-identical with checkSumAdjustment and the '
                                     'separately classified head timestamps zeroed, directory differs only in '
                                     'table data offsets and the head checksum, both adjustments valid')
            else:
                classified[field] = 'UNRESOLVED: other bytes differ'
        elif field in ('head.created', 'head.modified'):
            classified[field] = 'producer timestamp (export-rs wall clock); not copied in this run'
        else:
            classified[field] = 'UNRESOLVED'
    for key in diff:
        if key not in ('global_tables', 'raw_head_timestamps'): classified[key] = 'UNRESOLVED'
    if 'raw_head_timestamps' in diff:
        classified['raw_head_timestamps'] = 'producer timestamp (export-rs wall clock); not copied in this run'
    return {'native_sha256': hashlib.sha256(native_bytes).hexdigest(),
            'reference_sha256': hashlib.sha256(reference_bytes).hexdigest(),
            'byte_identical': native_bytes == reference_bytes,
            'byte_identical_except_checksum_adjustment': zeroed,
            'tables_identical_except_placement': placement_only,
            'checksum_adjustments_valid': adjustments_valid,
            'table_data_order': {'native': order(tables_a), 'reference': order(tables_b)},
            'differing_inventory_keys': sorted(diff), 'differing_global_fields': global_fields,
            'name_records_equal': a['name_records'] == b['name_records'],
            'cmap_records_equal': a['cmap_records'] == b['cmap_records'],
            'cmap_gids_equal': a['cmap_gids'] == b['cmap_gids'],
            'tables_equal': a['tables'] == b['tables'], 'strikes': strikes,
            'pixels': bitmap_pair(source, native, reference, 'otb'), 'classification': classified}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for key in ['bdf', 'native', 'reference', 'output']: p.add_argument('--' + key, type=Path, required=True)
    a = p.parse_args()
    results = {}
    for source in sorted(a.bdf.glob('*.bdf')):
        name = source.stem + '.otb'
        results[name] = compare(source, a.native / name, a.reference / name)
    if len(results) != 20: raise ValueError('expected the 20 Tong OTBs')
    counts = {}
    for result in results.values():
        for field in result['differing_global_fields'] + [k for k in result['differing_inventory_keys'] if k != 'global_tables']:
            counts[field] = counts.get(field, 0) + 1
    unresolved = sorted({f'{name}:{field}' for name, r in results.items()
                         for field, rule in r['classification'].items() if rule.startswith('UNRESOLVED')})
    summary = {'files': len(results), 'byte_identical': sum(r['byte_identical'] for r in results.values()),
               'byte_identical_except_checksum_adjustment': sum(r['byte_identical_except_checksum_adjustment'] for r in results.values()),
               'tables_identical_except_placement': sum(r['tables_identical_except_placement'] for r in results.values()),
               'checksum_adjustments_valid': sum(r['checksum_adjustments_valid'] for r in results.values()),
               'pixel_glyphs_checked': sum(r['pixels']['glyphs_checked'] for r in results.values()),
               'differing_field_file_counts': counts, 'unresolved': unresolved}
    a.output.write_text(json.dumps({'summary': summary, 'files': results}, indent=1) + '\n')
    print(json.dumps(summary, indent=1))


if __name__ == '__main__': main()
