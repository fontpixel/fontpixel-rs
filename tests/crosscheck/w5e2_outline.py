#!/usr/bin/env python3
"""Native consumers: FreeType 2.13.2 and author's sfnconv. Original artwork only."""
import argparse, hashlib, importlib.util, json, re, subprocess
from pathlib import Path
import freetype
ROOT=Path(__file__).resolve().parents[2]
def run(args):
    r=subprocess.run([str(v) for v in args],capture_output=True,text=True)
    if r.returncode:raise RuntimeError(f'{args}: exit {r.returncode}\n{r.stdout}\n{r.stderr}')
    return r.stdout

def author_ascii(path):
    text=path.read_text();base=int(re.search(r'^\$baseline (\d+)',text,re.M)[1]);out=[]
    for m in re.finditer(r'^===U\+([0-9A-F]+)===w(\d+)=h(\d+)=x(\d+)=y(\d+)=o(\d+)=(?:"([^"]*)")?===.*\n([^=]*?)(?=^===|^# End #)',text,re.M):
        cp,w,h,x,y,o=(int(m[1],16),*[int(m[i]) for i in range(2,7)])
        rows=[s for s in m[8].splitlines() if s.strip()];pixels=[]
        for row in rows:
            if set(row)<=set('.X'):pixels += [255 if c=='X' else 0 for c in row[:w]]
            else:pixels += [0 if c=='--------' else int(c[:2],16) for c in row.split()]
        assert len(pixels)==w*h,(path,cp,w,h,rows)
        out.append({'codes':[cp],'width':w,'height':h,'x':-o,'y':base-h,'advance':[x,-y],'sequences':[[ord(c) for c in m[7]]] if 0xf000<=cp<=0xf8ff and m[7] else [],'pixels':pixels})
    return out

def ft_glyphs(path,face_index=0,strike=0,model_font=None):
    face=freetype.Face(str(path),face_index);face.select_size(strike);out=[]
    if model_font is not None:
        size=face.available_sizes[strike];assert size.x_ppem//64==size.y_ppem//64==model_font["ppem"]
        assert face.family_name.decode("latin1")==model_font["family"]
    # FT_Get_Next_Char omits the sole mapping on this driver; direct indexes are authoritative.
    for cp in range(65536):
        idx=face.get_char_index(cp)
        if not idx:continue
        face.load_glyph(idx,freetype.FT_LOAD_DEFAULT);g=face.glyph;b=g.bitmap
        pixels=[255 if b.buffer[y*b.pitch+x//8]&(128>>(x%8)) else 0 for y in range(b.rows) for x in range(b.width)]
        out.append({'codes':[cp],'width':b.width,'height':b.rows,'x':g.bitmap_left,'y':g.bitmap_top-b.rows,'advance':[g.advance.x//64,g.advance.y//64],'sequences':[],'pixels':pixels})
    return out

def main():
    p=argparse.ArgumentParser();p.add_argument('--adapter',type=Path,required=True);p.add_argument('--sfnconv',type=Path,required=True);p.add_argument('--work',type=Path,required=True);a=p.parse_args();a.work.mkdir(parents=True,exist_ok=True)
    spec=importlib.util.spec_from_file_location('original',ROOT/'fuzz/seed_w5e2_outline.py');original=importlib.util.module_from_spec(spec);spec.loader.exec_module(original)
    run(['python',ROOT/'fuzz/seed_w5e2_outline.py','--out',a.work/'seeds'])
    dump=lambda fmt,path:json.loads(run([a.adapter,'dump',fmt,path]));cases=[]
    expected={'codes':[65],'width':9,'height':5,'x':-2,'y':-1,'advance':[11,0],'sequences':[],'pixels':[255 if c=='X' else 0 for row in original.ROWS for c in row]}
    for src in sorted((a.work/'seeds').glob('pfr-*.pfr')):
        if 'fractional' in src.name:continue
        model=dump('pfr',src);assert not model['read_loss'];assert all(f['glyphs']==[expected] for f in model['fonts']),src
        if '-ft-' in src.name:
            for si,_ in enumerate(model['fonts']):assert ft_glyphs(src,strike=si,model_font=model["fonts"][si])==[expected],src
        for variant in ['ftraw','ftrle4','ftrle8']:
            dest=a.work/(src.stem+'-'+variant+'.pfr');info=json.loads(run([a.adapter,'rewrite','pfr',src,dest,variant]));back=dump('pfr',dest)
            assert len(back['fonts'])==len(model['fonts'])
            for fi,f in enumerate(model['fonts']):assert ft_glyphs(dest,face_index=fi,model_font=f)==f['glyphs'],(dest,fi)
            cases.append({'id':dest.name,'consumer':'FreeType','read_loss':info['read_loss'],'write_loss':info['write_loss']})
    fractional=dump('pfr',a.work/'seeds/pfr-fractional.pfr');assert fractional['read_loss'];g=fractional['fonts'][0]['glyphs'][0];assert (g['x'],g['y'],g['advance'])==(-2,-1,[8,0]);cases.append({'id':'fractional-retained-loss','consumer':'DAVIC original fields'})
    for name in ['ssfn-original.sfn','ssfn-original.asc','ssfn-gray.asc','ssfn-ligature.asc']:
        source=a.work/'seeds'/name;before=dump('ssfn',source);assert not before['read_loss']
        # Author native writer -> fontpixel reader. Converter adds an NBSP and shifts underline for ASCII.
        external=a.work/(name+'-author.sfn');run([a.sfnconv,'-U','-o',source,external]);theirs=dump('ssfn',external)
        assert {k:theirs['fonts'][0][k] for k in ['family','ppem','ascent','descent']}=={k:before['fonts'][0][k] for k in ['family','ppem','ascent','descent']}
        nonextra=[g for g in theirs['fonts'][0]['glyphs'] if g['codes']!=[160]]
        if name=='ssfn-gray.asc':
            gold=a.work/'author-gray-export.asc';run([a.sfnconv,'-A','-o',external,gold]);assert author_ascii(gold)==theirs['fonts'][0]['glyphs']
            expected_author=json.loads(json.dumps(before['fonts'][0]['glyphs']));expected_author[0]['pixels']=[139 if v else 0 for v in expected_author[0]['pixels']]
            assert nonextra==expected_author,('author alpha coalescing changed',nonextra)
        else:assert nonextra==before['fonts'][0]['glyphs'],(name,theirs)
        extra=[g for g in theirs['fonts'][0]['glyphs'] if g['codes']==[160]];assert len(extra)==(0 if name=='ssfn-ligature.asc' else 1) and all(g['pixels']==[] and g['advance']==[10,0] for g in extra)
        for variant in ['binary','ascii']:
            native=a.work/(name+'-native-'+variant);info=json.loads(run([a.adapter,'rewrite','ssfn',source,native,variant]));assert not info['write_loss'],info
            ascii_out=a.work/(name+'-consumer-'+variant+'.asc');run([a.sfnconv,'-A','-o',native,ascii_out]);observed=author_ascii(ascii_out)
            extra=[g for g in observed if g['codes']==[160]];assert len(extra)==(0 if name=='ssfn-ligature.asc' else 1) and all(g['pixels']==[] and g['advance']==[10,0] for g in extra)
            observed=[g for g in observed if g['codes']!=[160]]
            if name=='ssfn-gray.asc' and variant=='ascii':
                assert author_ascii(native)==before['fonts'][0]['glyphs']
                assert observed==expected_author,('author ASCII alpha coalescing',observed)
            else:assert observed==before['fonts'][0]['glyphs'],(native,observed,before)
            cases.append({'id':native.name,'consumer':'sfnconv ASCII exporter','extra_nbsp_asserted':True})
    result={'cases':cases,'case_count':len(cases),'freetype_version':freetype.version(),'sfnconv_sha256':hashlib.sha256(a.sfnconv.read_bytes()).hexdigest(),'adapter_sha256':hashlib.sha256(a.adapter.read_bytes()).hexdigest()}
    (a.work/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'case_count':len(cases),'result':str(a.work/'result.json')}))
if __name__=='__main__':main()
