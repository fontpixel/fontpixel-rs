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
    return sorted((r.platformID,r.platEncID,r.langID,r.nameID,r.toUnicode(),r.string.hex()) for r in font['name'].names)


def plain(value):
    """Stable complete field inventory; never drop an unrecognized object."""
    if value is None or isinstance(value,(bool,int,float,str)):return value
    if isinstance(value,bytes):return {'bytes_hex':value.hex()}
    if isinstance(value,(list,tuple)):return [plain(item) for item in value]
    if isinstance(value,dict):return {str(key):plain(item) for key,item in sorted(value.items(),key=lambda pair:str(pair[0]))}
    if hasattr(value,'__dict__'):return {key:plain(item) for key,item in sorted(vars(value).items())}
    raise TypeError('unhandled fontTools metadata type: '+type(value).__name__)


def cmap_records(font):
    best=font.getBestCmap()
    records=[]
    for table in font['cmap'].tables:
        cmap=getattr(table,'cmap',{})
        if table.isUnicode() and table.format!=14:
            for cp,name in cmap.items():
                assert best.get(cp)==name,('inconsistent Unicode cmap subtable',table.platformID,table.platEncID,table.format,cp,name,best.get(cp))
        records.append({'platform':table.platformID,'encoding':table.platEncID,
                        'format':table.format,'language':getattr(table,'language',None),
                        'mapping':{str(cp):name for cp,name in sorted(cmap.items())},
                        'variation_sequences':plain(getattr(table,'uvsDict',None))})
    return records


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
    inventories={label:{tag:plain(vars(font[tag])) if tag in font else None for tag in FIELDS}
                 for label,font in [('native',native),('python',reference)]}
    for tag in FIELDS:
        nn,rn=inventories['native'][tag],inventories['python'][tag]
        if nn is None or rn is None:
            if nn!=rn:differences[tag]={'native':nn,'python':rn}
            continue
        for field in sorted(nn.keys() | rn.keys()):
            if field not in nn or field not in rn or nn[field]!=rn[field]:
                differences[tag+'.'+field]={'native_present':field in nn,'python_present':field in rn,'native':nn.get(field),'python':rn.get(field)}
    nn,rn=name_records(native),name_records(reference)
    if nn!=rn: differences['name_records']={'native':nn,'python':rn}
    cmaps={label:cmap_records(font) for label,font in [('native',native),('python',reference)]}
    if cmaps['native']!=cmaps['python']:differences['cmap_records']=cmaps
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
    if extra['native']!=extra['python']:differences['unencoded_glyph_identity_and_metrics']=extra
    return {'explicit_names_match':True,'explicit_common_metric_profile_matches':True,
            'complete_table_metric_inventory':inventories,
            'complete_cmap_inventory':cmaps,
            'encoded_common_advances_checked':len(nc),'unencoded_metric_inventory':extra,'policy_differences':differences,
            'policy_differences_are_accepted':False}


def bitmap_properties(task, native, reference):
    checked=[]
    # Application names are only overridden for export_name. Every other
    # BDF/PCF family/style field must still agree with the original producer.
    for key in ['FAMILY_NAME','FULL_NAME','FACE_NAME','WEIGHT_NAME','SLANT','SETWIDTH_NAME','ADD_STYLE_NAME','FONT_VERSION','FOUNDRY','SPACING']:
        assert (key in native.props)==(key in reference.props),(task['file'],'property presence',key)
        assert native.props.get(key)==reference.props.get(key),(task['file'],'bitmap names/style',key,native.props.get(key),reference.props.get(key))
        checked.append(key)
    metrics={}
    for field in ['pixel_size','ascent','descent','bbox']:
        n,r=getattr(native,field),getattr(reference,field)
        assert n==r,(task['file'],'bitmap global metric',field,n,r)
        metrics[field]=n
    if task.get('rename'):
        # opf.parse_bdf does not retain its FONT header. The independent raw
        # bdfparser profile checks that header; never count None==None here.
        for key in ['FAMILY_NAME','FULL_NAME','FACE_NAME']:
            assert native.props.get(key)==reference.props.get(key),(task['file'],'export_name',key,native.props.get(key),reference.props.get(key))
            checked.append(key)
        assert native.props.get('FAMILY_NAME')==task['rename']['export_name'],(task['file'],'export family')
    notices=sorted(key for key in native.props.keys() | reference.props.keys()
                   if any(word in key.upper() for word in ['COPYRIGHT','NOTICE','LICENSE','LICENCE']))
    for key in notices: assert native.props.get(key)==reference.props.get(key),(task['file'],'notice',key)
    return {'name_style_properties_checked':checked,'notice_properties_checked':notices,
            'global_metrics':metrics,'font_header_checked_by':'independent complete bitmap profile'}
