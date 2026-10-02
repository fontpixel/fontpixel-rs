#!/usr/bin/env python3
"""Generate deterministic SPEC-v3 rendering oracle cases; never write references.
Synthetic fixtures are committed. --corpus adds all supplied BDF samples to the
runtime comparison without redistributing those fonts in the generated fixture.
"""
import argparse, itertools, json, random, sys, warnings
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT.parent/'reference/bdfparser/src'))
from bdfparser import Font

def bdf(rng,n):
    w,h=rng.randrange(0,10),rng.randrange(0,11)
    cell=f'{w} {h} {rng.randrange(-4,3)} {rng.randrange(-5,3)}'
    globals_=''
    if n%3==0: globals_+='DWIDTH '+str(rng.randrange(-8,13))+' 0\n'
    if n%4==0: globals_+='DWIDTH1 0 '+str(rng.randrange(-10,14))+'\n'
    out=f'STARTFONT 2.2\nFONT render-synthetic\nSIZE 10 72 72\nFONTBOUNDINGBOX {cell}\n'+globals_+'STARTPROPERTIES 2\nCHARSET_REGISTRY "ISO10646"\nCHARSET_ENCODING "1"\nENDPROPERTIES\nCHARS 5\n'
    for cp in [67,65,66,32,65]:
        gw,gh=rng.randrange(0,12),rng.randrange(0,12)
        out+=f'STARTCHAR u{cp}\nENCODING {cp}\n'
        if rng.randrange(4):out+=f'DWIDTH {rng.randrange(-16,17)} {rng.randrange(-8,9)}\n'
        if rng.randrange(3):out+=f'DWIDTH1 {rng.choice([0,0,0,3])} {rng.randrange(-16,17)}\n'
        out+=f'BBX {gw} {gh} {rng.randrange(-7,7)} {rng.randrange(-7,7)}\nBITMAP\n'
        for _ in range(gh):out+=f'{rng.randrange(1<<gw)<<(8-gw%8 if gw%8 else 0):0{(gw+7)//8*2}X}\n' if gw else '\n'
        out+='ENDCHAR\n'
    return out+'ENDFONT\n'

def expected(font,case):
    opts=dict(linelimit=case.get('limit',512),mode=case.get('mode',1),direction=case.get('direction','lrtb'),usecurrentglyphspacing=case.get('current',False))
    if 'missing' in case:opts['missing']=font.glyphbycp(case['missing'])
    try:
        if 'all' in case:
            ranges=case.get('ranges')
            r=None if ranges is None else [tuple(x) for x in ranges]
            value=font.drawall(order=case['all'],r=r,**opts)
        elif 'text' in case:value=font.draw(case['text'],**opts)
        else:value=font.drawcps(case['cps'],**opts)
        return {'rows':value.bindata,'width':value.width(),'height':value.height()}
    except Exception as e:return {'error':type(e).__name__,'message':str(e)}

def generate(corpus=False):
    rng=random.Random(0xF04D14)
    fonts=[];cases=[]
    def add_font(text,name):
        f=Font().load_string(text);index=len(fonts);fonts.append({'name':name,'bdf':text})
        return index,f
    def add(index,font,**case):
        case={'font':index,**case};case['expected']=expected(font,case);cases.append(case)
    for n in range(24):
        index,font=add_font(bdf(rng,n),f'random-{n}')
        for a,b,mode,current in itertools.product(['lr','rl','tb','bt'],['lr','rl','tb','bt'],[0,1],[False,True]):
            add(index,font,cps=[65,66,None,67,65,999,32,65],direction=a+b,mode=mode,current=current,limit=rng.choice([0,4,12,20,512]))
        for order in [0,1,2,-1]:
            add(index,font,all=order,mode=0,limit=30,ranges=[[32,32],[65,67]])
        add(index,font,text='',direction='tb')
        add(index,font,cps=[999,65,999,66],missing=67,limit=50)
    # Test-font's historical examples, available at runtime only to avoid copying
    # its larger third-party BDF. Captures explicit supplied test vectors and drawall.
    if corpus:
        paths=sorted((ROOT.parent/'reference/bdfparser/tests/fonts').glob('*.bdf'))+sorted((ROOT.parent/'samples').rglob('*.bdf'))
        for path in paths:
            try:index,font=add_font(path.read_text(),str(path.relative_to(ROOT.parent)))
            except Exception:continue
            cps=list(font.itercps());subset=cps[:7]+cps[len(cps)//2:len(cps)//2+7]+cps[-7:]
            for d,m in itertools.product(['lrtb','rltb','lrbt','rlbt','tbrl','tblr','btrl','btlr'],[0,1]):
                add(index,font,cps=subset,mode=m,direction=d,limit=128)
            add(index,font,all=1,mode=0,limit=700)
            for text in ['Bdf Hi','مرحبا','a的','Bé H好Δi的']:
                add(index,font,text=text,limit=80)
    return {'fonts':fonts,'cases':cases}

def main():
    p=argparse.ArgumentParser();p.add_argument('--output',type=Path,default=ROOT/'crates/fontpixel/tests/data/render_reference.json');p.add_argument('--corpus',action='store_true');p.add_argument('--check',action='store_true');a=p.parse_args()
    warnings.simplefilter('ignore')
    data=generate(a.corpus);text=json.dumps(data,separators=(',',':'),ensure_ascii=False)+'\n'
    if a.check:
        if not a.output.exists() or a.output.read_text()!=text:raise SystemExit(f'generated rendering oracle differs: {a.output}')
    else:a.output.write_text(text)
    print(f'{len(data["fonts"])} fonts, {len(data["cases"])} cases -> {a.output}')
if __name__=='__main__':main()
