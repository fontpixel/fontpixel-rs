#!/usr/bin/env python3
"""MIT original C1 fixtures versus independent MIT monobit 0.54 reader/writer."""
import argparse,hashlib,importlib.util,json,pathlib,subprocess,sys
import monobit
ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('c1seed',ROOT/'fuzz/seed_w5c1.py');seed=importlib.util.module_from_spec(spec);spec.loader.exec_module(seed)
def run(adapter,*args):return subprocess.run([str(adapter),*map(str,args)],check=True,capture_output=True,text=True).stdout
def rust(adapter,fmt,path):
    sets=json.loads(run(adapter,'dump',fmt,path));out=[]
    for f in sets:
        glyphs={}
        for g in f['glyphs']:
            row=(g['width']+7)//8
            points=sorted((x+g['x'],g['height']-1-y+g['y'])for y in range(g['height'])for x in range(g['width'])if g['bytes'][y*row+x//8]&(128>>(x%8)))
            for code in g['codes']:glyphs[code]={'rect':[g['width'],g['height'],g['x'],g['y']]if g['width'] and g['height'] else None,'points':points,'advance':g['advance'],'scalable':g['scalable_advance']}
        out.append(glyphs)
    return out
def external(font,fmt):
    out={}
    for g in font.glyphs:
        # PXL loader emits absent all-zero directory entries as empty placeholders.
        if fmt=='pxl' and not g.width and not g.height and not g.scalable_width:continue
        cp=int(g.codepoint)
        points=sorted((x+g.left_bearing,g.height-1-y+g.shift_up)for y,row in enumerate(g.as_matrix(inklevels=(0,1)))for x,p in enumerate(row)if p)
        out[cp]={'rect':[g.width,g.height,g.left_bearing,g.shift_up]if g.width and g.height else None,'points':points,'advance':g.advance_width}
    return out
def comparable(r,fmt):
    return {k:{kk:vv for kk,vv in v.items() if kk!='scalable' and (fmt!='pxl' or kk!='advance')}for k,v in r.items()}
def verify(adapter,fmt,path,kwargs):
    ours=rust(adapter,fmt,path);fonts=monobit.load(path,format=fmt,**kwargs);assert len(ours)==len(fonts),(fmt,'strike_count')
    for a,b in zip(ours,fonts):
        x,y=comparable(a,fmt),comparable(external(b,fmt),fmt)
        assert x==y,(fmt,path.name,[(k,x.get(k),y.get(k))for k in x.keys()|y.keys()if x.get(k)!=y.get(k)][:1])
        if fmt=='pxl':
            # PXL oracle exposes fractional pixel scalable_width, not device advance.
            for g in b.glyphs:
                if int(g.codepoint) in a:
                    assert a[int(g.codepoint)]['advance']==round(g.scalable_width),(path.name,g.scalable_width,a[int(g.codepoint)])
                    assert a[int(g.codepoint)]['scalable']==500
    return sum(map(len,ours))
def main():
    p=argparse.ArgumentParser();p.add_argument('--adapter',type=pathlib.Path,required=True);p.add_argument('--artifacts',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args();a.adapter=a.adapter.resolve();a.artifacts.mkdir(parents=True,exist_ok=True);cases=[]
    for fmt,label,data in seed.cases():
        d=a.artifacts/(fmt+'-'+label);d.mkdir(exist_ok=True);source=d/'source.nlq' if fmt=='daisy' else d/'source.bin';source.write_bytes(data)
        if fmt=='daisy' and label=='mag':(d/'source.nl2').write_bytes(seed.daisy('mag',part=1))
        version=label.removesuffix('-dense')
        kwargs={'version':'pcw' if label=='pcw127' else version}if fmt=='tasprint' else {}
        if fmt=='tasprint' and label=='pcw127':
            # Official 127-slot variant differs from monobit 0.54's fixed 128-slot expectation.
            padded=d/'oracle-padded.bin';padded.write_bytes(data+bytes(32));oracle=external(monobit.load(padded,format=fmt,version='pcw')[0],fmt);oracle.pop(128);ours=rust(a.adapter,fmt,source)[0];assert comparable(ours,fmt)==comparable(oracle,fmt);count=len(ours)
        else:count=verify(a.adapter,fmt,source,kwargs)
        out=d/('native.nlq' if fmt=='daisy' else 'native.bin');run(a.adapter,'write',fmt,source,out)
        if fmt=='tasprint' and label=='pcw127':assert out.read_bytes()==data
        else:verify(a.adapter,fmt,out,kwargs)
        result={'format':fmt,'fixture':label,'glyphs':count,'native_read_and_write_external_reader':True,'source_sha256':hashlib.sha256(data).hexdigest(),'native_sha256':hashlib.sha256(out.read_bytes()).hexdigest()}
        if fmt=='tasprint' and label!='pcw127':
            sourcefont=monobit.load(source,format=fmt,**kwargs)[0];saved=d/'monobit.bin';monobit.save(sourcefont,saved,format=fmt,version=version,raw=True,overwrite=True);verify(a.adapter,fmt,saved,kwargs);result['external_writer_native_reader']=True;result['monobit_sha256']=hashlib.sha256(saved.read_bytes()).hexdigest()
        cases.append(result)
    from fontTools.tfmLib import TFM
    import fontTools
    d=a.artifacts/'pxl-tfm';d.mkdir(exist_ok=True);src=d/'source.pxl';src.write_bytes(seed.pxl());(d/'font.tfm').write_bytes(seed.tfm())
    native=d/'native.pxl';run(a.adapter,'tfm','pxl',src,native)
    tfm=TFM(str(d/'font.tfm'));assert tfm.designsize==10 and tfm.chars[65]['width']==0.5
    native_model=rust(a.adapter,'pxl',native)[0];assert native_model[65]['advance']==14 and native_model[65]['scalable']==500
    # Monobit's independent TFM application also consumes the emitted pair.
    oracle=monobit.load(native,format='pxl',tfm='font.tfm')[0]
    assert round(oracle.get_glyph(codepoint=65).scalable_width)==14
    tfm_case={'fonttools':fontTools.__version__,'fonttools_design_width_exact':True,'monobit_applies_native_tfm':True,'source_tfm_sha256':hashlib.sha256(seed.tfm()).hexdigest(),'native_tfm_sha256':hashlib.sha256((d/'font.tfm').read_bytes()).hexdigest()}
    real=ROOT.parent/'local-work/w5c1-sources/polyprint-002.fnt'
    real_case={}
    if real.exists():
        count=verify(a.adapter,'polyprint',real,{})
        target=a.artifacts/'polyprint-002.native';run(a.adapter,'write','polyprint',real,target);verify(a.adapter,'polyprint',target,{})
        real_case={'url':'https://www.seasip.info/Unix/PSF/Amstrad/PolyPrint/002.fnt','sha256':hashlib.sha256(real.read_bytes()).hexdigest(),'glyphs':count,'read_write_pixels_metrics_exact':True,'license':'Not established; external read-only oracle asset, not redistributed'}
    official={}
    texbin=ROOT.parent/'local-work/texlive-oracle/usr/bin'
    if (texbin/'tftopl').exists():
        tool=texbin/'tftopl';pl=d/'native.pl'
        proc=subprocess.run([str(tool),str(d/'font.tfm'),str(pl)],capture_output=True,check=True)
        (d/'tftopl.stdout').write_bytes(proc.stdout);(d/'tftopl.stderr').write_bytes(proc.stderr)
        text=pl.read_text();assert '(DESIGNSIZE R 10.0)' in text and '(CHARACTER C A' in text and '(CHARWD R 0.5)' in text
        regenerated=d/'pltotf.tfm';proc2=subprocess.run([str(texbin/'pltotf'),str(pl),str(regenerated)],capture_output=True,check=True)
        (d/'pltotf.stdout').write_bytes(proc2.stdout);(d/'pltotf.stderr').write_bytes(proc2.stderr)
        offdir=d/'official';offdir.mkdir(exist_ok=True);(offdir/'font.tfm').write_bytes(regenerated.read_bytes());(offdir/'font.pxl').write_bytes(native.read_bytes())
        assert rust(a.adapter,'pxl',offdir/'font.pxl')[0][65]['advance']==14
        version=subprocess.run([str(tool),'--version'],capture_output=True,check=True).stdout.decode().splitlines()[0]
        official={'version':version,'tftopl_exit':proc.returncode,'pltotf_exit':proc2.returncode,'native_reader_official_tfm':True,'native_tfm_design_width_exact':True,'executable_sha256':hashlib.sha256(tool.read_bytes()).hexdigest(),'pl_sha256':hashlib.sha256(pl.read_bytes()).hexdigest(),'regenerated_tfm_sha256':hashlib.sha256(regenerated.read_bytes()).hexdigest(),'stderr_note':'Local extracted TeX Live warns that texmf.cnf is absent; tools return0 and report no format corrections'}
    report={'official_tex':official,'real_polyprint':real_case,'tfm':tfm_case,'python':sys.version,'monobit':'0.54','fixture_license':'Original project MIT byte assemblers in fuzz/seed_w5c1.py','cases':cases,'limits':['PXL oracle advance_width defaults to bitmap width; native device advance is independently checked against rounded oracle scalable_width, per-1000 scalable advance against exact fixture fixed point','monobit PCW reader hardcodes128 slots; documented127-slot fixture compared against its128-slot padded result minus code128','Printshop proprietary offset/kerning semantics remain unknown; only independently observed contiguous-raster profile is asserted']}
    a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'cases':len(cases),'status':'pass'}))
if __name__=='__main__':main()
