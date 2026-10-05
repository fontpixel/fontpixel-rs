#!/usr/bin/env python3
"""Validate all 173 catalogue fonts and eight ZIPs using independent consumers.

Every physical BDF/PCF record and every sfnt glyph is inspected. Explicit app
names, cmap, common advances, outlines and encoded raster mismatches fail.
Complete metadata/default/helper differences are retained and remain unresolved;
an inventory or a generic policy description never turns them into a pass.
"""
import argparse
import gzip
import hashlib
import importlib.metadata
import io
import json
from pathlib import Path
import sys
import tempfile
import traceback
import warnings
import zipfile
from phase6_catalogue_bitmap import profile as bitmap_profile, compare_profiles, differences
from phase6_catalogue_metadata import POLICIES, bitmap_properties, sfnt_metadata, plain
from phase6_support import digest
from phase6_sfnt_validity import sfnt_file_validity
from phase6_validate import outlines, web_equivalent


def pixels(g):
    stride=(g.bbw+7)//8; data=bytearray(g.rows)
    if g.bbw%8:
        mask=(0xff << (8-g.bbw%8)) & 0xff
        for row in range(g.bbh):data[row*stride+stride-1]&=mask
    return bytes(data).hex()


def glyphs(font):
    return {g.cp:(g.name,g.bbw,g.bbh,g.bbx,g.bby,g.dwidth,pixels(g)) for g in font.glyphs}


def alternates(font):
    return [(g.name,g.bbw,g.bbh,g.bbx,g.bby,g.dwidth,pixels(g)) for g in font.unencoded_glyphs]


def hash_value(value):
    return hashlib.sha256(json.dumps(value,separators=(',',':'),ensure_ascii=True).encode()).hexdigest()


def face(path,font):
    import freetype
    if font.flavor:
        flavor=font.flavor;font.flavor=None;data=io.BytesIO()
        try:font.save(data)
        finally:font.flavor=flavor
        return freetype.Face(io.BytesIO(data.getvalue()))
    return freetype.Face(str(path))


def raster_glyph(ft,gid,ppem,strike_index=None):
    import freetype
    if strike_index is None:ft.set_pixel_sizes(0,ppem)
    else:ft.select_size(strike_index)
    ft.load_glyph(gid,freetype.FT_LOAD_RENDER | freetype.FT_LOAD_TARGET_MONO | freetype.FT_LOAD_NO_HINTING)
    slot=ft.glyph;bitmap=slot.bitmap;data=bytes(bitmap.buffer)
    if bitmap.width and bitmap.rows:
        assert bitmap.pixel_mode==freetype.FT_PIXEL_MODE_MONO,('unexpected FreeType raster mode',bitmap.pixel_mode)
    ink=[]
    for y in range(bitmap.rows):
        row=y if bitmap.pitch>=0 else bitmap.rows-1-y
        for x in range(bitmap.width):
            if data[row*abs(bitmap.pitch)+x//8] & (0x80>>(x%8)):
                ink.append((slot.bitmap_left+x,slot.bitmap_top-y-1))
    signature=(int(slot.advance.x),int(slot.advance.y),ink)
    return {'pixels_and_advance_sha256':hash_value(signature),'ink_pixels':len(ink),
            'advance':[int(slot.advance.x),int(slot.advance.y)],
            'bitmap_metrics':{'left':slot.bitmap_left,'top':slot.bitmap_top,'width':bitmap.width,'rows':bitmap.rows},
            'glyph_metrics':{key:int(getattr(slot.metrics,key)) for key in ['width','height','horiBearingX','horiBearingY','horiAdvance','vertBearingX','vertBearingY','vertAdvance']}}


def strike_inventory(font):
    order=set(font.getGlyphOrder());strikes=[];seen=set()
    assert len(font['EBLC'].strikes)==len(font['EBDT'].strikeData),'bitmap strike arrays differ'
    for index,strike in enumerate(font['EBLC'].strikes):
        size=strike.bitmapSizeTable;key=(size.ppemX,size.ppemY,size.bitDepth)
        # Do not silently collapse a second strike at the same size into a dict.
        assert key not in seen,('duplicate physical strike requires explicit comparison',key)
        seen.add(key);names=set(font['EBDT'].strikeData[index]);assert names<=order,'bitmap glyph absent from glyph order'
        strikes.append({'key':list(key),'size_fields':plain(vars(size)),
                        'subtables':[{'index_format':sub.indexFormat,'image_format':sub.imageFormat,'names':list(sub.names)} for sub in strike.indexSubTables],
                        'present_glyphs':sorted(names)})
    return strikes


def rendered_glyphs(path,font,task):
    order=font.getGlyphOrder();ft=face(path,font);result={};strikes=None
    if task['kind']=='ttf':
        strikes=strike_inventory(font);fixed={}
        for index,size in enumerate(ft.available_sizes):
            key=(round(size.x_ppem/64),round(size.y_ppem/64))
            assert key not in fixed,('duplicate FreeType physical strike',key)
            fixed[key]=index
        sizes=[(strike['key'][1],fixed[tuple(strike['key'][:2])],set(strike['present_glyphs'])) for strike in strikes]
    else:sizes=[(task['pixel_size'],None,set(order)),(task['pixel_size']*2,None,set(order))]
    for ppem,strike_index,present in sizes:
        records={}
        for gid,name in enumerate(order):
            record={'gid':gid,'hmtx':list(font['hmtx'].metrics[name]),'has_bitmap':name in present}
            if name in present:record.update(raster_glyph(ft,gid,ppem,strike_index))
            records[name]=record
        result[str(ppem)]=records
    return {'strikes':strikes,'all_glyphs_by_ppem':result}


def outline_inventory(font):
    if 'glyf' not in font and 'CFF ' not in font and 'CFF2' not in font:return None
    return {name:hash_value(contours) for name,contours in zip(font.getGlyphOrder(),outlines(font),strict=True)}


def check_sfnt(task,left,right,work):
    from fontTools.ttLib import TTFont
    import ots
    n,r=TTFont(left),TTFont(right)
    try:
        n.ensureDecompiled();r.ensureDecompiled();metadata=sfnt_metadata(task,n,r)
        checksum_proofs={};derived_fields={}
        for label,path,font in [('native',left,n),('python',right,r)]:
            if font.flavor is None:
                proof=sfnt_file_validity(path)
                assert font['head'].checkSumAdjustment==proof['stored_checkSumAdjustment'],(task['file'],label,'decoded/raw checksum adjustment')
                checksum_proofs[label]=proof
            else:
                # A web container has no raw sfnt directory. Do not manufacture
                # a new sfnt, validate that, and claim the original checksum is
                # valid. Its original same-engine source is checked separately.
                source=path.with_suffix('.ttf')
                checksum_proofs[label]={'scope':'web container; source raw sfnt only, embedded adjustment not waived',
                                        'web_sha256':digest(path),'source':sfnt_file_validity(source)}
        if n.flavor is None and r.flavor is None:
            observed=metadata['policy_differences'].pop('head.checkSumAdjustment',None)
            if observed:
                derived_fields['head.checkSumAdjustment']={'observed':observed,
                    'proof':'Each side independently passes every table checksum, directory bounds and the complete raw sfnt checksum; adjustment is recomputed from its own bytes.'}
        nc,rc=n.getBestCmap(),r.getBestCmap();assert set(nc)==set(rc),'complete cmap coverage'
        mapped_names={'native':set(nc.values()),'python':set(rc.values())}
        rendered={label:rendered_glyphs(path,font,task) for label,path,font in [('native',left,n),('python',right,r)]}
        ni,ri=rendered['native'],rendered['python']
        assert set(ni['all_glyphs_by_ppem'])==set(ri['all_glyphs_by_ppem']),'physical strike sizes'
        checks=0;metric_diffs={};unencoded={}
        for ppem,names in ni['all_glyphs_by_ppem'].items():
            other=ri['all_glyphs_by_ppem'][ppem]
            for cp in sorted(nc):
                ng,rg=names[nc[cp]],other[rc[cp]]
                assert ng['has_bitmap']==rg['has_bitmap'],(task['file'],cp,ppem,'physical strike presence')
                if ng['has_bitmap']:
                    assert ng['pixels_and_advance_sha256']==rg['pixels_and_advance_sha256'],(task['file'],cp,ppem,'pixels/advance')
                    for key in ['bitmap_metrics','glyph_metrics']:
                        if ng[key]!=rg[key]:metric_diffs.setdefault(ppem,{}).setdefault(str(cp),{})[key]={'native':ng[key],'python':rg[key]}
                    checks+=1
            unmapped={label:{name:{key:value for key,value in glyph.items() if key!='gid'} for name,glyph in data.items() if name not in mapped_names[label]}
                      for label,data,cmap in [('native',names,nc),('python',other,rc)]}
            if unmapped['native']!=unmapped['python']:unencoded[ppem]=unmapped
        if metric_diffs:metadata['policy_differences']['encoded_raster_metrics']=metric_diffs
        if unencoded:metadata['policy_differences']['all_unencoded_glyph_rasters']=unencoded
        if ni['strikes']!=ri['strikes']:metadata['policy_differences']['complete_physical_strike_metadata']={'native':ni['strikes'],'python':ri['strikes']}
        geometry={'native':outline_inventory(n),'python':outline_inventory(r)}
        if task['kind']!='ttf':
            assert geometry['native'] is not None and geometry['python'] is not None,'vector outlines missing'
            for cp in sorted(nc):
                assert geometry['native'][nc[cp]]==geometry['python'][rc[cp]],(task['file'],cp,'exact decomposed outline, including winding/control points')
            extras={label:{name:value for name,value in values.items() if name not in mapped_names[label]}
                    for label,values,cmap in [('native',geometry['native'],nc),('python',geometry['python'],rc)]}
            if extras['native']!=extras['python']:metadata['policy_differences']['unencoded_outline_geometry']=extras
        elif geometry['python'] is not None:
            empty=hash_value([])
            assert all(value==empty for value in geometry['python'].values()),'reference bitmap TTF unexpectedly contains an outline'
        ots_checks={}
        if task['kind']!='ttf':
            for label,path,font in [('native',left,n),('python',right,r)]:
                with tempfile.TemporaryDirectory(prefix='ots-',dir=work) as temporary:
                    cleaned=Path(temporary)/'font.sfnt';status=ots.sanitize(str(path),str(cleaned),capture_output=True)
                    ots_checks[label]={'exit':status.returncode,'stdout':status.stdout.decode(errors='replace'),'stderr':status.stderr.decode(errors='replace')}
                    assert status.returncode==0,(task['file'],label,'OTS',status.stderr)
                    clean=TTFont(cleaned)
                    try:
                        clean.ensureDecompiled()
                        before_order,after_order=font.getGlyphOrder(),clean.getGlyphOrder()
                        assert len(before_order)==len(after_order),(task['file'],label,'sanitized glyph count')
                        assert {cp:font.getGlyphID(name) for cp,name in font.getBestCmap().items()}=={cp:clean.getGlyphID(name) for cp,name in clean.getBestCmap().items()},(task['file'],label,'sanitized cmap IDs')
                        after=rendered_glyphs(cleaned,clean,task)
                        before=rendered[label]['all_glyphs_by_ppem']
                        for ppem,records in before.items():
                            for before_name,after_name in zip(before_order,after_order,strict=True):
                                assert records[before_name]['pixels_and_advance_sha256']==after['all_glyphs_by_ppem'][ppem][after_name]['pixels_and_advance_sha256'],(task['file'],label,ppem,before_name,'sanitized pixels/advance')
                    finally:clean.close()
        web_checks={}
        if task['kind'].startswith('woff2'):
            for label,path in [('native',left),('python',right)]:
                source=path.with_suffix('.ttf');assert source.is_file(),('missing same-engine uncompressed catalogue vector',source)
                web_checks[label]=web_equivalent(source,path,work)
        return {'metadata':metadata,'independent_checksum_proofs':checksum_proofs,'verified_derived_fields':derived_fields,
                'glyphs':len(nc),'render_checks':checks,
                'native_sfnt_glyphs':n['maxp'].numGlyphs,'python_sfnt_glyphs':r['maxp'].numGlyphs,
                'complete_glyph_raster_inventory':rendered,'complete_outline_inventory':geometry,
                'ots':ots_checks,'same_sfnt_web_validation':web_checks,
                'unresolved_differences':metadata['policy_differences']}
    finally:n.close();r.close()


def check_zip(slug,tasks,native,python):
    left,right=native/(slug+'.zip'),python/(slug+'.zip')
    result={'file':slug+'.zip','native_sha256':digest(left),'python_sha256':digest(right),'members':[]}
    with zipfile.ZipFile(left) as n,zipfile.ZipFile(right) as r:
        assert n.testzip() is None and r.testzip() is None,(slug,'ZIP CRC')
        assert len(n.namelist())==len(set(n.namelist())) and len(r.namelist())==len(set(r.namelist())),(slug,'duplicate ZIP member')
        assert n.namelist()==r.namelist(),(slug,'archive inventory/order')
        for name in n.namelist():
            a,b=n.read(name),r.read(name)
            if not name.endswith('.bdf'):assert a==b,(slug,name,'license/provenance')
            else:
                task=next(t for t in tasks if t['slug']==slug and t['kind']=='bdf' and t['variant']+'.bdf'==name)
                assert a==gzip.decompress((native/task['file']).read_bytes()),(slug,name,'native ZIP BDF identity')
                assert b==gzip.decompress((python/task['file']).read_bytes()),(slug,name,'reference ZIP BDF identity')
            result['members'].append({'name':name,'native_sha256':hashlib.sha256(a).hexdigest(),'python_sha256':hashlib.sha256(b).hexdigest(),'native_bytes':len(a),'python_bytes':len(b)})
    return result


def archive_member_dependencies(slug,tasks,fonts):
    expected={task['file'] for task in tasks if task['slug']==slug and task['kind']=='bdf'}
    actual={item['file']:item for item in fonts if item['file'] in expected}
    assert expected and set(actual)==expected,(slug,'archive font proof inventory')
    return [{'file':name,'status':actual[name]['status'],'proof':actual[name]['proof'],
             'proof_sha256':actual[name]['proof_sha256']} for name in sorted(expected)
            if actual[name]['status']!='pass']


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['reference','plan','native','python','output']:p.add_argument('--'+name,type=Path,required=True)
    a=p.parse_args();a.output.parent.mkdir(parents=True,exist_ok=True)
    protected=Path(__file__).resolve().parents[2]/'reference'
    if a.reference.resolve().is_relative_to(protected.resolve()):raise ValueError('use copied reference code')
    with a.output.open('x') as stream:json.dump({'partial':True,'status':'started'},stream)
    details=Path(str(a.output)+'.details');details.mkdir()
    sys.dont_write_bytecode=True;sys.path.insert(0,str(a.reference/'fontpixel/pipeline'));sys.path.insert(0,str(a.reference/'bdfparser/src'))
    from opf.parsers.bdf import parse_bdf
    from opf.parsers.pcf import parse_pcf
    from bdfparser import Font as IndependentFont
    from fontTools.ttLib.woff2 import WOFF2GlyfTable
    original=WOFF2GlyfTable._decodeComponents
    def no_tail_copy(self,glyph):
        self.compositeStream=memoryview(self.compositeStream);return original(self,glyph)
    WOFF2GlyfTable._decodeComponents=no_tail_copy
    plan=json.loads(a.plan.read_text());assert plan.get('version')==2,'version2 corrected plan required'
    tasks=plan['tasks'];assert len(tasks)==173 and len({t['file'] for t in tasks})==173
    assert all(Path(t['file']).name==t['file'] and t['file'] not in ['.','..'] for t in tasks),'unsafe artifact filename'
    slugs=sorted({t['slug'] for t in tasks});assert len(slugs)==8
    expected={t['file'] for t in tasks}|{slug+'.zip' for slug in slugs}
    for directory in [a.native,a.python]:assert {path.name for path in directory.iterdir() if path.is_file()}==expected,(directory,'complete181')
    inputs={str(Path(path).resolve()):digest(path) for task in tasks for path in task['inputs']}
    result={'partial':True,'status':'running','results':[],'archives':[],'wire_differences':[],'policy_definitions':POLICIES,
            'plan_sha256':digest(a.plan),'source_inputs':inputs,
            'consumer_versions':{name:importlib.metadata.version(name) for name in ['fonttools','freetype-py','opentype-sanitizer','uharfbuzz','brotli']},
            'policy':'Any new metadata/unencoded/helper difference remains unresolved. No generic waiver or review-ledger pass exists.'}
    def save():a.output.write_text(json.dumps(result,indent=2)+'\n')
    save()
    for task in tasks:
        name=task['file'];left,right=a.native/name,a.python/name
        record={'file':name,'kind':task['kind'],'native_sha256':digest(left),'python_sha256':digest(right)}
        if record['native_sha256']!=record['python_sha256']:result['wire_differences'].append(name)
        captured=[]
        try:
            with warnings.catch_warnings(record=True) as captured:
                warnings.simplefilter('always')
                if task['kind'] in ['bdf','pcf']:
                    native_profile,python_profile=bitmap_profile(left,IndependentFont),bitmap_profile(right,IndependentFont)
                    record['complete_bitmap_profiles']={'native':native_profile,'python':python_profile}
                    record['unresolved_differences']=compare_profiles(native_profile,python_profile)
                    parser=parse_bdf if task['kind']=='bdf' else parse_pcf
                    n,r=parser(left,task['slug']),parser(right,task['slug'])
                    record['metadata']=bitmap_properties(task,n,r)
                    assert glyphs(n)==glyphs(r),(name,'all encoded application glyph records')
                    assert alternates(n)==alternates(r),(name,'application unencoded records')
                    record.update(glyphs=len(native_profile['encoded_records']),unencoded_glyphs=len(native_profile['unencoded_records']),all_physical_glyphs_checked=True)
                else:record.update(check_sfnt(task,left,right,details))
            assert digest(left)==record['native_sha256'] and digest(right)==record['python_sha256'],'output changed during validation'
            record['status']='unresolved-differences' if record['unresolved_differences'] else 'pass'
        except Exception as error:
            record.update(status='fail',error_type=type(error).__name__,error=str(error),traceback=traceback.format_exc())
        record['warnings']=[str(item.message) for item in captured]
        proof=details/(name+'.json')
        with proof.open('x') as stream:json.dump(record,stream,indent=2);stream.write('\n')
        result['results'].append({'file':name,'status':record['status'],'proof':str(proof.resolve()),'proof_sha256':digest(proof),
                                  'unresolved_fields':sorted(record.get('unresolved_differences',{}))})
        save();print(name,record['status'],flush=True)
    for slug in slugs:
        try:
            record=check_zip(slug,tasks,a.native,a.python)
            blocked=archive_member_dependencies(slug,tasks,result['results'])
            record.update(packaging_status='pass',status='unresolved-font-members' if blocked else 'pass',
                          blocking_font_members=blocked)
        except Exception as error:record={'file':slug+'.zip','status':'fail','error_type':type(error).__name__,'error':str(error),'traceback':traceback.format_exc()}
        result['archives'].append(record);save()
    assert all(digest(path)==identity for path,identity in inputs.items()),'source input changed during validation'
    failed=sum(record['status']=='fail' for record in result['results']+result['archives'])
    unresolved=sum(record['status']=='unresolved-differences' for record in result['results'])
    unresolved_archives=sum(record['status']=='unresolved-font-members' for record in result['archives'])
    passed=not failed and not unresolved and not unresolved_archives
    result.update(partial=not passed,status='pass' if passed else 'fail',font_files=173,archive_count=8,
                  failed_artifacts=failed,unresolved_metadata_files=unresolved,unresolved_archives=unresolved_archives)
    save();print(json.dumps({'complete':passed,'font_files':173,'archives':8,'failed_artifacts':failed,'unresolved_metadata_files':unresolved,'unresolved_archives':unresolved_archives}))
    return 0 if passed else 1

if __name__=='__main__':raise SystemExit(main())
