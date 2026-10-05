#!/usr/bin/env python3
"""Independent sfnt validity proofs; these never waive semantic differences."""
import hashlib
from pathlib import Path
import struct

MASK=0xffffffff
SFNT_CHECKSUM=0xb1b0afba


def checksum(data):
    """Sum big-endian words; virtual zero padding applies to the final word."""
    view=memoryview(data);whole=len(view)//4*4
    value=sum(word[0] for word in struct.iter_unpack('>I',view[:whole]))
    if whole!=len(view):value+=int.from_bytes(bytes(view[whole:]).ljust(4,b'\0'),'big')
    return value&MASK


def raw_sfnt_validity(data):
    """Verify every physical table and the complete standalone sfnt checksum.

    This accepts raw sfnt only. WOFF/WOFF2 do not carry this directory; silently
    serializing them through another library would replace the bytes whose
    integrity is being checked. Their same-sfnt source proof is separate.
    """
    if len(data)<12 or data[:4] not in [b'\0\1\0\0',b'OTTO',b'true']:
        raise ValueError('raw standalone sfnt required for checksum proof')
    count=struct.unpack_from('>H',data,4)[0];directory_end=12+16*count
    if not count or directory_end>len(data):raise ValueError('invalid sfnt table directory count')
    tags=set();intervals=[];records=[];adjustment=None
    for index in range(count):
        tag,stored,offset,length=struct.unpack_from('>4sIII',data,12+16*index)
        if tag in tags:raise ValueError('duplicate sfnt table tag')
        tags.add(tag)
        if offset%4 or offset<directory_end or offset>len(data) or length>len(data)-offset:
            raise ValueError(('out-of-bounds/unaligned sfnt table',tag,offset,length))
        table=memoryview(data)[offset:offset+length]
        if tag==b'head':
            if length<54:raise ValueError('truncated head table')
            adjustment=struct.unpack_from('>I',table,8)[0]
            # head's own directory checksum is defined with this field zero.
            computed=(checksum(table)-adjustment)&MASK
        else:computed=checksum(table)
        if computed!=stored:raise ValueError(('sfnt table checksum mismatch',tag,stored,computed))
        records.append({'tag':tag.decode('ascii'),'offset':offset,'length':length,
                        'stored_checksum':stored,'recomputed_checksum':computed})
        if length:intervals.append((offset,offset+length,tag))
    for left,right in zip(sorted(intervals),sorted(intervals)[1:]):
        if left[1]>right[0]:raise ValueError(('overlapping sfnt tables',left[2],right[2]))
    if adjustment is None:raise ValueError('missing head table')
    whole=checksum(data)
    recomputed=(SFNT_CHECKSUM-((whole-adjustment)&MASK))&MASK
    if whole!=SFNT_CHECKSUM or recomputed!=adjustment:
        raise ValueError(('invalid whole sfnt checksum adjustment',whole,adjustment,recomputed))
    return {'scope':'original raw sfnt bytes; every table and whole file independently recomputed',
            'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'tables':records,
            'whole_file_checksum':whole,'stored_checkSumAdjustment':adjustment,
            'recomputed_checkSumAdjustment':recomputed,'checksums_valid':True,'directory_bounds_valid':True}


def sfnt_file_validity(path):
    result=raw_sfnt_validity(Path(path).read_bytes());result['path']=str(path)
    return result
