#!/usr/bin/env python3
"""Explicit application-intent checks and independently decoded policy inventory."""
FIELDS = {
    'head': ['unitsPerEm','macStyle','lowestRecPPEM','flags','fontDirectionHint','created','modified','xMin','yMin','xMax','yMax'],
    'hhea': ['ascent','descent','lineGap','advanceWidthMax','minLeftSideBearing','minRightSideBearing','xMaxExtent','numberOfHMetrics'],
    'OS/2': ['version','xAvgCharWidth','usWeightClass','usWidthClass','fsType','fsSelection','sTypoAscender','sTypoDescender','sTypoLineGap','usWinAscent','usWinDescent','ulUnicodeRange1','ulUnicodeRange2','ulUnicodeRange3','ulUnicodeRange4','ulCodePageRange1','ulCodePageRange2','usMaxContext','sxHeight','sCapHeight','usDefaultChar','usBreakChar','achVendID'],
    'post': ['formatType','italicAngle','underlinePosition','underlineThickness','isFixedPitch'],
    'maxp': ['numGlyphs'],
}
POLICIES = {
    'scope': 'Explicit intent matches; remaining differences are enumerated, not blanket sfnt equality.',
    'names': 'Application family/style/full/PostScript/copyright/version must match. Native vector Version prefix, unique-ID spelling, typographic IDs/platform record coverage remain recorded.',
    'bitmap_metrics': 'UPM1000/largest physical strike/ties-even. Native retains source bearings, source or zero-synthetic .notdef, source line gap/underline/spacing and bitmap-derived extents.',
    'glyph_identity': 'Native retains physical identities/unencoded glyphs. Original bitmap sfnt uses encoded union plus half-em synthetic .notdef. Extra native glyph metrics are inventoried; their mere presence is not a full pixel proof.',
    'tables': 'Original bitmap sfnt has empty glyf/loca and post3; native OTB omits dummy outlines and retains names. hmtx compaction and strike order are serialization differences.',
    'defaults': 'fontTools4.66.1 fsType4/fsSelection0/usBreakChar32 and head/OS2 vendor/version defaults are not source author licensing intent; adapter does not copy them.',
    'timestamp': 'Original fixes 3660681600 (2020-01-01 in1904 epoch), native fixes raw zero; decoded and raw timestamps are both recorded.',
    'source': 'Supplied opf/ttfexport.py, vectorize.py and installed FontBuilder defaults; exact versions and source hashes must accompany the final manifest.',
}


def name_values(font, ident):
    return {r.toUnicode() for r in font['name'].names if r.nameID == ident}


def name_records(font):
    return sorted((r.platformID,r.platEncID,r.langID,r.nameID,r.toUnicode()) for r in font['name'].names)


def expected_names(task, font, label):
    for ident, field in {0:'copyright',1:'family',2:'style',4:'full_name',6:'postscript_name'}.items():
        assert name_values(font,ident) == {task[field]}, (task['file'],label,ident,name_values(font,ident),task[field])
    assert {s.removeprefix('Version ') for s in name_values(font,5)} == {task['font_version']}, (task['file'],label,'version')
    for ident,field in [(16,'family'),(17,'style')]:
        if name_values(font,ident): assert name_values(font,ident) == {task[field]}, (task['file'],label,ident)


def sfnt_metadata(task, native, reference):
    expected_names(task,native,'native'); expected_names(task,reference,'reference')
    assert name_values(reference,3) == {f"{task['family']}-{task['style']}-{task['font_version']}"}, (task['file'],'reference unique ID')
    differences = {}
    for tag,fields in FIELDS.items():
        for field in fields:
            n=getattr(native[tag],field,None) if tag in native else None
            r=getattr(reference[tag],field,None) if tag in reference else None
            if n!=r: differences[tag+'.'+field]={'native':n,'python':r}
    nn,rn=name_records(native),name_records(reference)
    if nn!=rn: differences['name_records']={'native':nn,'python':rn}
    nt,rt=sorted(map(str,native.reader.keys())),sorted(map(str,reference.reader.keys()))
    if nt!=rt: differences['tables']={'native':nt,'python':rt}
    stamps={label:{'created':int.from_bytes(font.reader['head'][20:28],'big'),'modified':int.from_bytes(font.reader['head'][28:36],'big')}
            for label,font in [('native',native),('python',reference)]}
    if stamps['native']!=stamps['python']: differences['raw_head_timestamps']=stamps
    nc,rc=native.getBestCmap(),reference.getBestCmap(); assert set(nc)==set(rc),(task['file'],'cmap')
    advances=[];bearings=[]
    for cp in sorted(nc):
        n,r=native['hmtx'].metrics[nc[cp]],reference['hmtx'].metrics[rc[cp]]
        if n[0]!=r[0]: advances.append({'cp':cp,'native':n[0],'python':r[0]})
        if n[1]!=r[1]: bearings.append({'cp':cp,'native':n[1],'python':r[1]})
    assert not advances,(task['file'],'encoded common advances',advances)
    if bearings: differences['encoded_common_bearings']=bearings
    if task['kind']=='ttf':
        assert native['head'].unitsPerEm==reference['head'].unitsPerEm==1000,(task['file'],'bitmap UPM')
        for tag,fields in [('hhea',['ascent','descent']),('OS/2',['sTypoAscender','sTypoDescender','usWinAscent','usWinDescent'])]:
            for field in fields: assert getattr(native[tag],field)==getattr(reference[tag],field),(task['file'],tag,field)
    else:
        expected=(1000//task['pixel_size'])*task['pixel_size']
        assert native['head'].unitsPerEm==reference['head'].unitsPerEm==expected,(task['file'],'vector UPM')
    extra={}
    for label,font,cmap in [('native',native,nc),('python',reference,rc)]:
        mapped=set(cmap.values())
        extra[label]=[{'name':name,'advance':font['hmtx'].metrics[name][0],'bearing':font['hmtx'].metrics[name][1]}
                      for name in font.getGlyphOrder() if name not in mapped]
    return {'explicit_names_match':True,'explicit_common_metric_profile_matches':True,
            'encoded_common_advances_checked':len(nc),'unencoded_metric_inventory':extra,'policy_differences':differences}


def bitmap_properties(task, native, reference):
    checked=[]
    if task.get('rename'):
        for key in ['FONT','FAMILY_NAME','FULL_NAME','FACE_NAME']:
            assert native.props.get(key)==reference.props.get(key),(task['file'],'export_name',key,native.props.get(key),reference.props.get(key))
            checked.append(key)
        assert native.props.get('FAMILY_NAME')==task['rename']['export_name'],(task['file'],'export family')
    notices=sorted(key for key in native.props.keys() | reference.props.keys()
                   if any(word in key.upper() for word in ['COPYRIGHT','NOTICE','LICENSE','LICENCE']))
    for key in notices: assert native.props.get(key)==reference.props.get(key),(task['file'],'notice',key)
    return {'export_name_properties_checked':checked,'notice_properties_checked':notices}
