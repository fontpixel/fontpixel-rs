#!/usr/bin/env python3
"""Original MIT W5.3 independent format-oracle comparisons; retains raw artifacts.

Requires Python >=3.12, monobit 0.54 and optional official FIGlet 2.2.5.
No oracle implementation is copied into the native codecs.
"""
from __future__ import annotations
import argparse, hashlib, importlib.util, json, pathlib, subprocess, sys
import monobit
ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('w53_seeds',ROOT/'fuzz/seed_w53.py')
seed=importlib.util.module_from_spec(spec);spec.loader.exec_module(seed)

def run(adapter,*args):
    p=subprocess.run([str(adapter),*map(str,args)],check=True,capture_output=True,text=True)
    return p.stdout

def rust_model(adapter,fmt,path):
    data=json.loads(run(adapter,'dump',fmt,path))[0]
    out={}
    for g in data['glyphs']:
        stride=(g['width']+7)//8
        points=sorted((x+g['x'],g['height']-1-y+g['y'])for y in range(g['height'])for x in range(g['width'])if g['bytes'][y*stride+x//8]&(128>>(x%8)))
        for cp in g['codes']:out[cp]={'advance':g['advance'],'points':points,'rect':[g['width'],g['height'],g['x'],g['y']]}
    return out

def oracle_model(font,fmt):
    out={}
    for g in font.glyphs:
        codes=[int(g.codepoint)]if g.codepoint else [ord(g.char.value)]if len(g.char.value)==1 else []
        points=sorted((x+g.left_bearing,g.height-1-y+g.shift_up)for y,row in enumerate(g.as_matrix(inklevels=(0,1)))for x,p in enumerate(row)if p)
        for code in codes:out[code]={'advance':g.advance_width,'points':points,'rect':[g.width,g.height,g.left_bearing,g.shift_up]}
    return out

def comparable(model,fmt):
    # FIGlet negative tags are translation-table glyphs which monobit intentionally drops.
    return {k:{**v,'rect':v['rect'] if v['rect'][0] and v['rect'][1] else None} for k,v in model.items()if fmt!='figlet' or k<0x80000000}

def main():
    p=argparse.ArgumentParser();p.add_argument('--adapter',type=pathlib.Path,required=True);p.add_argument('--artifacts',type=pathlib.Path,required=True);p.add_argument('--figlet',type=pathlib.Path);p.add_argument('--chkfont',type=pathlib.Path);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args()
    a.adapter=a.adapter.resolve();a.artifacts.mkdir(parents=True,exist_ok=True)
    results=[]
    for fmt,fixtures in [('vfont',{'little':seed.vfont(),'big':seed.vfont('>'),'full256':seed.vfont_full()}),('fzx',{'proportional':seed.fzx(),'full224':seed.fzx_full()}),('figlet',{'canonical':seed.figlet().replace(b'0 0 3\n',b'0 0 2\n').replace(b'-02 map\n#@\n @@\n',b''),'full358':seed.figlet_full()})]:
        for label,data in fixtures.items():
            source=a.artifacts/f'{fmt}-{label}.source';source.write_bytes(data)
            rust=a.artifacts/f'{fmt}-{label}.rust';run(a.adapter,'write',fmt,source,rust)
            oracle=monobit.load(source,format=fmt)[0]
            ours=rust_model(a.adapter,fmt,source);theirs=oracle_model(oracle,fmt)
            assert comparable(ours,fmt)==comparable(theirs,fmt),(fmt,label,'source decode',[(k,ours.get(k),theirs.get(k))for k in ours.keys()|theirs.keys()if comparable(ours,fmt).get(k)!=comparable(theirs,fmt).get(k)][:1])
            back=oracle_model(monobit.load(rust,format=fmt)[0],fmt)
            target=rust_model(a.adapter,fmt,rust)
            assert comparable(target,fmt)==comparable(back,fmt),(fmt,label,'rust write',set(target)^set(back))
            outgoing=a.artifacts/f'{fmt}-{label}.monobit'
            monobit.save(oracle,outgoing,format=fmt,overwrite=True,**({'endianness':'big' if label=='big' else 'little'}if fmt=='vfont' else {}))
            raw=outgoing.read_bytes()
            independent=oracle_model(monobit.load(outgoing,format=fmt)[0],fmt)
            parsed=rust_model(a.adapter,fmt,outgoing)
            assert comparable(parsed,fmt)==comparable(independent,fmt),(fmt,label,'monobit write',set(parsed)^set(independent))
            results.append({'format':fmt,'fixture':label,'bidirectional':True,'exact_nonempty_rectangles_pixels_codes_advances':True,'glyphs':len(target),'source_sha256':hashlib.sha256(data).hexdigest(),'rust_sha256':hashlib.sha256(rust.read_bytes()).hexdigest(),'monobit_raw_sha256':hashlib.sha256(raw).hexdigest()})
    official={}
    if a.figlet:
        font=a.artifacts/'official.flf'
        font.write_bytes((a.artifacts/'figlet-canonical.rust').read_bytes())
        proc=subprocess.run([str(a.figlet.resolve()),'-f',str(font.resolve()),'-A','-l','-W','A'],capture_output=True,check=True)
        (a.artifacts/'figlet-official-A.stdout').write_bytes(proc.stdout)
        assert proc.stdout.splitlines()==[b'## ',b' ##'],proc.stdout
        official={'render_A_exact':True,'stdout_sha256':hashlib.sha256(proc.stdout).hexdigest(),'executable_sha256':hashlib.sha256(a.figlet.read_bytes()).hexdigest()}
        signed=a.artifacts/'signed.flf';signed.write_bytes(seed.figlet())
        control=a.artifacts/'signed.flc';control.write_bytes(b'flc2a\n90 -2\n')
        signed_out=a.artifacts/'signed-rust.flf';run(a.adapter,'write','figlet',signed,signed_out)
        for label,font in [('source',signed),('rust',signed_out)]:
            proc=subprocess.run([str(a.figlet.resolve()),'-f',str(font.resolve()),'-A','-l','-W','-C',str(control.resolve()),'Z'],capture_output=True,check=True)
            (a.artifacts/f'figlet-negative-{label}.stdout').write_bytes(proc.stdout)
            assert proc.stdout.splitlines()==[b'#',b' '],proc.stdout
        official['signed_code_source_and_native_output_exact']=True
        standard=a.figlet.parent/'fonts/standard.flf'
        if standard.exists():
            decoded=json.loads(run(a.adapter,'dump','figlet',standard))
            official['real_standard_read_glyphs']=len(decoded[0]['glyphs'])
            official['real_standard_sha256']=hashlib.sha256(standard.read_bytes()).hexdigest()
            official['real_standard_license']='Font header permits modification if modifier name is placed on a comment line; external read-only source, not redistributed'


    if a.chkfont:
        proc=subprocess.run([str(a.chkfont.resolve()),str((a.artifacts/'official.flf').resolve())],capture_output=True,check=True)
        (a.artifacts/'chkfont.stdout').write_bytes(proc.stdout);(a.artifacts/'chkfont.stderr').write_bytes(proc.stderr)
        official['chkfont_exit']=proc.returncode
        assert b'ERROR-' not in proc.stdout+proc.stderr,proc.stdout+proc.stderr
        official['chkfont_errors']=0
        official['chkfont_warnings']=['non-increasing code tag for valid missing-character code 0','historical checker warns about valid code tags above 65535']
    report={'python':sys.version,'monobit':'0.54','fixture_license':'Original project MIT fixtures generated by fuzz/seed_w53.py','primary_sources':[
        {'url':'https://man.freebsd.org/cgi/man.cgi?apropos=0&manpath=SunOS+4.1.3&query=vfont&sektion=5','sha256':'fbd58a37c77bc62ef534ecad8f12681150eb72505a87ea86b143e43be3d0d4d2'},
        {'url':'https://raw.githubusercontent.com/cmatsuoka/figlet/master/figfont.txt','sha256':'4d6b5571e887c8f95dea7a7823841e0c7915893fc2808b762bb2a78c52d1d3e0'},
        {'url':'https://raw.githubusercontent.com/z88dk/z88dk/master/libsrc/font/fzx/README.md','sha256':'b826503f685717d674555c962e1d49567eb1bb72d9b9c36f143b79a187957972'}],
        'cases':results,'official_figlet':official,'artifact_directory':str(a.artifacts.resolve()),'oracle_limits':['monobit 0.54 rejects negative FIGlet tags (Props assignment TypeError) and traditional octal tags (ValueError); its oracle fixture omits that record, while native tests and the official FIGlet executable cover signed tags','monobit 0.54 treats FZX tracking as signed; normal-range tracking is correctly inherited into glyph advances; zero-area raster dimensions are normalized only for comparison','FIGlet hardblank projects to ink per PLAN; monobit projects it to blank; tested separately with explicit loss diagnostics']}
    a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'cases':len(results),'official_figlet':official,'status':'pass'}))
if __name__=='__main__':main()
