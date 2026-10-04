#!/usr/bin/env python3
"""Complete BDF/PCF records for catalogue validation, using copied bdfparser.

The original application parser intentionally projects away BDF headers,
SWIDTH/vertical metadata and PCF unencoded records. It cannot prove preservation
of those fields. This adapter does not alter either reference implementation.
"""
from collections import Counter
import gzip
import json
from pathlib import Path
import struct


def differences(left, right):
    """Keep absence distinct from an explicit null/empty/zero value."""
    return {key: {'native_present': key in left, 'python_present': key in right,
                  'native': left.get(key), 'python': right.get(key)}
            for key in sorted(left.keys() | right.keys())
            if key not in left or key not in right or left[key] != right[key]}


def multiset_difference(left, right):
    def counted(records):
        return Counter(json.dumps(record, sort_keys=True, ensure_ascii=True) for record in records)
    a, b = counted(left), counted(right)
    return {label: [{'count': count, 'record': json.loads(record)}
                    for record, count in sorted(values.items())]
            for label, values in [('native_only', a-b), ('python_only', b-a)]}


def bdf_physical(data):
    """Retain every physical record, duplicate mapping and unknown glyph field."""
    text = data.decode('utf-8', errors='surrogateescape')
    headers, properties, glyphs = [], [], []
    current = None
    in_properties = False
    in_bitmap = False
    declared = None
    for line in text.splitlines():
        if not line.strip():
            continue
        key, _, value = line.partition(' ')
        value = value.strip()
        if current is not None:
            if key == 'ENDCHAR':
                glyphs.append(current); current = None; in_bitmap = False
            elif key == 'BITMAP':
                in_bitmap = True
            elif in_bitmap:
                # Hex case and byte padding bits do not change physical ink;
                # full decoded records below independently compare dimensions.
                current['rows'].append(value.upper())
            else:
                tokens = value.split()
                if key in ['ENCODING','SWIDTH','DWIDTH','SWIDTH1','DWIDTH1','VVECTOR','BBX']:
                    tokens = [int(token) for token in tokens]
                current['fields'].append([key, tokens])
        elif key == 'STARTCHAR':
            current = {'name': value, 'fields': [], 'rows': []}
        elif key == 'STARTPROPERTIES':
            in_properties = True
        elif key == 'ENDPROPERTIES':
            in_properties = False
        elif in_properties:
            properties.append([key, value])
        elif key == 'CHARS':
            declared = int(value)
        elif key != 'ENDFONT':
            headers.append([key, value])
    if current is not None or in_properties:
        raise ValueError('unterminated BDF record/property list')
    if declared != len(glyphs):
        raise ValueError(('BDF physical CHARS count', declared, len(glyphs)))
    return {'declared_glyphs': declared, 'physical_glyphs': glyphs,
            'raw_header_records': headers, 'raw_property_records': properties}


def pcf_physical(data, font):
    """Index physical slots, including aliases/unencoded, without dict collapse."""
    count = struct.unpack_from('<I', data, 4)[0]
    if count > (len(data)-8)//16:
        raise ValueError('invalid PCF directory')
    tables = {}
    directory = []
    for index in range(count):
        tag, fmt, size, offset = struct.unpack_from('<4I', data, 8+16*index)
        if tag in tables or offset > len(data):
            raise ValueError('duplicate/out-of-bounds PCF table')
        available=len(data)-offset
        if size>available:
            # X11/bdftopcf advertises 100 bytes for accelerators but leaves the
            # final payload at 48 bytes (72 with ink bounds), without padding.
            # This is an explicit wire rule, not permission to truncate metrics
            # or encodings. The independent reader validates that payload too.
            actual=72 if fmt & 0x100 else 48
            if tag not in [2,256] or size!=100 or available!=actual:
                raise ValueError('out-of-bounds PCF table')
        tables[tag] = (fmt, memoryview(data)[offset:offset+size])
        directory.append({'type':tag,'format':fmt,'declared_bytes':size,'available_bytes':min(size,available)})
    def table(tag):
        fmt, raw = tables[tag]
        if len(raw) < 4 or struct.unpack_from('<I', raw)[0] != fmt:
            raise ValueError('PCF directory/table format mismatch')
        return fmt, raw, '>' if fmt & 4 else '<'
    fmt, metrics, endian = table(4)
    physical = struct.unpack_from(endian+('H' if fmt & 0x100 else 'I'), metrics, 4)[0]
    _, encodings, endian = table(32)
    low, high, low_byte, high_byte, default = struct.unpack_from(endian+'5H', encodings, 4)
    if low > high or low_byte > high_byte:
        raise ValueError('invalid PCF encoding range')
    codes = [[] for _ in range(physical)]
    entries = (high-low+1)*(high_byte-low_byte+1)
    if entries > (len(encodings)-14)//2:
        raise ValueError('truncated PCF encoding grid')
    for index in range(entries):
        gid = struct.unpack_from(endian+'H', encodings, 14+2*index)[0]
        if gid == 0xffff:continue
        if gid >= physical:raise ValueError('PCF encoding references absent glyph')
        cp = ((low_byte+index//(high-low+1)) << 8) | (low+index%(high-low+1))
        codes[gid].append(cp)
    unencoded = iter(font.unencoded)
    glyphs = []
    for mapped in codes:
        if mapped:
            records = [list(font.glyphs[cp]) for cp in mapped]
            for meta in records:meta[1] = -1
            # A missing PCF glyph-name table permits the decoder to derive
            # different names for aliases. Keep every alias record explicitly.
            meta = records
        else:
            item = next(unencoded, None)
            if item is None:raise ValueError('independent parser omitted an unencoded PCF record')
            meta = [item['meta']]
        glyphs.append({'codes':mapped,'meta':meta})
    if next(unencoded, None) is not None:
        raise ValueError('independent PCF unencoded count exceeds physical slots')
    return {'physical_count':physical,'physical_glyphs':glyphs,
            'default_character':default,'table_directory':directory}


def profile(path, font_class):
    data = Path(path).read_bytes()
    if data.startswith(b'\x1f\x8b'):data = gzip.decompress(data)
    font = font_class(data)
    physical = pcf_physical(data, font) if data.startswith(b'\x01fcp') else bdf_physical(data)
    return {'headers':font.headers,'properties':font.props,
            'property_spelling':font._propnames,
            'encoded_records':font.glyphs,'unencoded_records':font.unencoded,
            'physical':physical}


def compare_profiles(native, reference):
    result = {}
    for field in ['headers','properties','property_spelling']:
        diff = differences(native[field], reference[field])
        if diff:result[field] = diff
    for field in ['encoded_records','unencoded_records']:
        a, b = native[field], reference[field]
        if isinstance(a, dict):
            # JSON keys are strings in proof artifacts; codepoint identity remains exact.
            diff = differences({str(k):v for k,v in a.items()}, {str(k):v for k,v in b.items()})
        else:
            diff = multiset_difference(a,b)
            if not any(diff.values()):diff = {}
        if diff:result[field] = diff
    diff = multiset_difference(native['physical']['physical_glyphs'], reference['physical']['physical_glyphs'])
    if any(diff.values()):result['physical_glyph_records'] = diff
    # Directory offsets, compressed metric encoding and BDF textual whitespace
    # are separately inventoried rather than confused with decoded semantics.
    for key in ['default_character','physical_count','declared_glyphs']:
        if native['physical'].get(key) != reference['physical'].get(key):
            result[key] = {'native':native['physical'].get(key),'python':reference['physical'].get(key)}
    for key in ['raw_header_records','raw_property_records']:
        if key in native['physical'] or key in reference['physical']:
            diff=multiset_difference(native['physical'].get(key,[]),reference['physical'].get(key,[]))
            if any(diff.values()):result[key]=diff
    return result
