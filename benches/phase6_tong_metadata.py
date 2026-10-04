#!/usr/bin/env python3
"""Independent complete Tong bitmap metadata, without implicit policy waivers.

The caller still runs the existing source/cmap/pixel/advance checks. This adds
all physical PCF records and all OTB GIDs, raw names and global/strike metrics.
Every difference is unresolved until a concrete production fix or explicitly
supported semantic rule exists; an inventory alone never establishes parity.
"""
import hashlib
import json
from pathlib import Path
import sys
from phase6_catalogue_bitmap import bdf_physical, compare_profiles, differences, profile
from phase6_catalogue_metadata import FIELDS, cmap_records, name_records, plain


def hash_value(value):
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()


def source_inventory(path):
    physical=bdf_physical(Path(path).read_bytes())
    codes=[]
    for glyph in physical['physical_glyphs']:
        encoded=[value for key,value in glyph['fields'] if key=='ENCODING']
        assert len(encoded)==1 and len(encoded[0])==1,('Tong source ENCODING',glyph['name'],encoded)
        cp=encoded[0][0]
        assert 0<=cp<=0x10ffff and not 0xd800<=cp<=0xdfff,('Tong source must have scalar encoding',cp)
        codes.append(cp)
    assert len(codes)==len(set(codes)), 'duplicate Tong source scalar would be lost in a glyph dictionary'
    return {'physical_count':len(codes),'complete_physical_records_sha256':hash_value(physical),
            'headers':physical['raw_header_records'],'properties':physical['raw_property_records']}


def copied_bdfparser(work):
    path=(Path(work)/'reference/bdfparser').resolve()
    protected=Path(__file__).resolve().parents[2]/'reference'
    if path.is_relative_to(protected.resolve()):raise ValueError('use copied bdfparser reference')
    sys.dont_write_bytecode=True
    sys.path.insert(0,str(path/'src'))
    from bdfparser import Font
    origin=Path(sys.modules[Font.__module__].__file__).resolve()
    assert origin.is_relative_to(path), ('unexpected bdfparser consumer',origin,path)
    return Font


def glyph_raster(face,gid,index):
    import freetype
    face.select_size(index)
    face.load_glyph(gid,freetype.FT_LOAD_RENDER|freetype.FT_LOAD_TARGET_MONO|freetype.FT_LOAD_NO_HINTING)
    slot=face.glyph;bitmap=slot.bitmap;data=bytes(bitmap.buffer)
    if bitmap.width and bitmap.rows:
        assert bitmap.pixel_mode==freetype.FT_PIXEL_MODE_MONO,('non-monochrome Tong bitmap',bitmap.pixel_mode)
    ink=[]
    for y in range(bitmap.rows):
        row=y if bitmap.pitch>=0 else bitmap.rows-1-y
        for x in range(bitmap.width):
            if data[row*abs(bitmap.pitch)+x//8] & (0x80>>(x%8)):
                ink.append((slot.bitmap_left+x,slot.bitmap_top-y-1))
    metrics=[int(getattr(slot.metrics,key)) for key in
             ['width','height','horiBearingX','horiBearingY','horiAdvance','vertBearingX','vertBearingY','vertAdvance']]
    return {'ink_and_advance_sha256':hash_value([int(slot.advance.x),int(slot.advance.y),ink]),
            'bitmap_metrics':[bitmap.width,bitmap.rows,slot.bitmap_left,slot.bitmap_top],
            'glyph_metrics':metrics}


def sfnt_inventory(path):
    from fontTools.ttLib import TTFont
    import freetype
    font=TTFont(path)
    try:
        font.ensureDecompiled();order=font.getGlyphOrder();face=freetype.Face(str(path))
        assert len(order)==len(set(order))==font['maxp'].numGlyphs, 'physical sfnt glyph inventory'
        assert 'EBLC' in font and 'EBDT' in font, 'Tong OTB lacks monochrome bitmap tables'
        assert len(font['EBLC'].strikes)==len(font['EBDT'].strikeData)==len(face.available_sizes), 'physical strike count'
        strikes=[]
        for index,(strike,bitmaps,size) in enumerate(zip(font['EBLC'].strikes,font['EBDT'].strikeData,face.available_sizes,strict=True)):
            assert set(bitmaps)<=set(order), 'bitmap records outside physical glyph order'
            actual=strike.bitmapSizeTable
            assert (actual.ppemX,actual.ppemY)==(round(size.x_ppem/64),round(size.y_ppem/64)), 'FreeType strike association'
            records=[]
            for gid,name in enumerate(order):
                record={'gid':gid,'name':name,'hmtx':list(font['hmtx'].metrics[name]),'bitmap_present':name in bitmaps}
                if name in bitmaps:
                    bitmap=bitmaps[name];bitmap.ensureDecompiled()
                    record['stored_metrics']=plain(vars(bitmap.metrics)) if hasattr(bitmap,'metrics') else None
                    record['components']=plain([vars(component) for component in getattr(bitmap,'componentArray',[])])
                    record['raster']=glyph_raster(face,gid,index)
                # A missing physical bitmap is explicit, including .notdef;
                # do not substitute a consumer fallback glyph and call it equal.
                records.append(record)
            strikes.append({'size_fields':plain(vars(actual)),
                            'index_subtables':[plain(vars(sub)) for sub in strike.indexSubTables],
                            'all_physical_glyphs':records})
        raw_head=font.reader['head']
        return {'tables':sorted(map(str,font.reader.keys())),
                'global_tables':{tag:plain(vars(font[tag])) if tag in font else None for tag in FIELDS},
                'name_records':name_records(font),'cmap_records':cmap_records(font),
                'cmap_gids':{str(cp):font.getGlyphID(name) for cp,name in font.getBestCmap().items()},
                'raw_head_timestamps':{'created':int.from_bytes(raw_head[20:28],'big'),'modified':int.from_bytes(raw_head[28:36],'big')},
                'all_physical_strikes':strikes}
    finally:font.close()


def bitmap_metadata(source,native,reference,kind,work):
    source_result=source_inventory(source)
    if kind=='pcf':
        consumer=copied_bdfparser(work)
        a,b=profile(native,consumer),profile(reference,consumer)
        diff=compare_profiles(a,b)
    elif kind=='otb':
        a,b=sfnt_inventory(native),sfnt_inventory(reference)
        diff=differences(a,b)
    else:raise ValueError(kind)
    return {'source_physical_validation':source_result,
            'complete_native_inventory':a,'complete_reference_inventory':b,
            'unresolved_differences':diff,'differences_accepted':False}
