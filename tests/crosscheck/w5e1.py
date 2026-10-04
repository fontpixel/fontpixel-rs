#!/usr/bin/env python3
"""Native E1 device-font crosscheck; tools are consumers, never library dependencies.

Original rasters come from w5e1_timing. Bits'N'Picas is pinned, Rockbox uses its
unmodified official loader through an original local adapter, and Panic pdc /
Simulator consume native source and compiled PFT files. No vendor glyphs needed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import time

JAR_SHA = '22685a52bc1a7f26680f80197548e74dc61a9f58eb083d6f700220a48a1165a6'

def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()

def kbits(path):
    b=path.read_bytes(); assert b[:8]==b'KBnPbits'; version,*metrics=struct.unpack_from('>7i',b,8); assert version==1
    p=36; glyphs=[]
    while True:
        tag=b[p:p+4]; p+=4
        if tag==b'fin.':
            assert p==len(b); return {'glyphs':glyphs,'metrics':metrics}
        if tag==b'name':
            version,ident,n=struct.unpack_from('>iiH',b,p); assert version==1; p+=10+n
        elif tag==b'char':
            version,cp,advance,x,top,h=struct.unpack_from('>6i',b,p); assert version==1;p+=24;rows=[]
            for _ in range(h):
                w,=struct.unpack_from('>i',b,p);p+=4;rows.append(b[p:p+w]);p+=w
            w=max(map(len,rows),default=0)
            glyphs.append(dict(cp=cp,width=w,height=h,x=x,y=top-h,advance=advance,pixels=list(b''.join(row.ljust(w,b'\0') for row in rows))))
        else: raise AssertionError(tag)

def canonical(g):
    # Compare all nonzero coverage samples in absolute baseline coordinates,
    # while keeping advance. Native editors may crop transparent rectangles.
    w,h=g['width'],g['height'];ink=tuple((g['x']+x,g['y']+h-1-y,g['pixels'][y*w+x]) for y in range(h) for x in range(w) if g['pixels'][y*w+x])
    return g['cp'],g['advance'],ink

def mapped(sig):
    out={}
    for g in sig['glyphs']:
        value=canonical(g)
        if g['cp'] in out: assert out[g['cp']]==value
        out[g['cp']]=value
    return out

def bdf(path,w=9,h=7):
    s=f'STARTFONT 2.1\nFONT W5E1-Original\nSIZE {h} 72 72\nFONTBOUNDINGBOX {w} {h} 0 0\nSTARTPROPERTIES 5\nCHARSET_REGISTRY "ISO10646"\nCHARSET_ENCODING "1"\nFONT_ASCENT {h}\nFONT_DESCENT 0\nFAMILY_NAME "Original E1"\nENDPROPERTIES\nCHARS 3\n'
    for i in range(3):
        rows=[]
        for y in range(h):
            row=bytearray((w+7)//8)
            for x in range(w):
                if (x*3+y*5+i)%7<2: row[x//8]|=128>>(x%8)
            rows.append(row.hex().upper())
        s+=f'STARTCHAR uni{65+i:04X}\nENCODING {65+i}\nSWIDTH 1000 0\nDWIDTH {w} 0\nBBX {w} {h} 0 0\nBITMAP\n'+'\n'.join(rows)+'\nENDCHAR\n'
    path.write_text(s+'ENDFONT\n')

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--jar',type=Path,required=True)
    ap.add_argument('--rockbox-reader',type=Path,required=True);ap.add_argument('--sdk',type=Path,required=True)
    ap.add_argument('--output-dir',type=Path,required=True);ap.add_argument('--display',default=':97')
    a=ap.parse_args();out=a.output_dir.resolve();out.mkdir(parents=True,exist_ok=True)
    binary=a.binary.resolve();jar=a.jar.resolve();sdk=a.sdk.resolve();assert sha(jar)==JAR_SHA
    commands=[];checks=[]
    def run(*args):
        p=subprocess.run(list(map(str,args)),capture_output=True)
        commands.append({'argv':list(map(str,args)),'exit':p.returncode,'stdout_sha256':hashlib.sha256(p.stdout).hexdigest(),'stderr':p.stderr.decode(errors='replace')})
        assert p.returncode==0,(args,p.stderr.decode(errors='replace'));return p.stdout
    def inspect(fmt,path):return json.loads(run(binary,'inspect',fmt,path))
    def java(fmt,src,dest):
        run('java','-Djava.awt.headless=true','-jar',jar,'convertbitmap','-f',fmt,'-o',dest,src);assert dest.exists(),f'Java returned zero without producing {dest}'
    run(binary,'emit',out)
    for fmt,name in [('rb11','original.rb11'),('rb12','original.rb12'),('u8m','original.u8m'),('cybiko','original.cyf'),('sbf','original.sbf'),('hmzk','original.hmzk'),('playdate','original-playdate.fnt'),('split','original-split.fnt')]:
        native=out/name;expected=json.loads((out/(name+'.expected.json')).read_text());decoded=out/(name+'.java.kbits');java('kbits',native,decoded)
        actual=kbits(decoded)
        if fmt=='u8m':
            default=[g for g in actual['glyphs'] if g['cp']==0];assert len(default)==1 and canonical(default[0])==(0,0,())
            actual['glyphs']=[g for g in actual['glyphs'] if g['cp']!=0]
        assert mapped(actual)==mapped(expected),(fmt,'external reader mismatch')
        assert actual['metrics'][2:4]==[expected['ascent'],expected['descent']],(fmt,'line metrics mismatch')
        rust=inspect(fmt,native);assert mapped(rust)==mapped(expected),(fmt,'native Rust reader mismatch')
        checks.append(dict(format=fmt,direction='Rust writer -> BitsNPicas native reader',glyphs=len(mapped(expected)),pixels=sum(len(g['pixels']) for g in expected['glyphs']),sha256=sha(native)))
        source=out/(fmt+'.bdf');bdf(source,16 if fmt=='hmzk' else 9,16 if fmt=='hmzk' else 7)
        reverse=out/('java-'+name);java('playdate-separate' if fmt=='split' else fmt,source,reverse)
        reread=out/('java-'+name+'.kbits');java('kbits',reverse,reread)
        reverse_sig=kbits(reread)
        if fmt=='u8m':
            default=[g for g in reverse_sig['glyphs'] if g['cp']==0];assert len(default)==1 and canonical(default[0])==(0,0,())
            reverse_sig['glyphs']=[g for g in reverse_sig['glyphs'] if g['cp']!=0]
        assert mapped(inspect(fmt,reverse))==mapped(reverse_sig),(fmt,'external producer mismatch')
        checks.append(dict(format=fmt,direction='BitsNPicas native writer -> Rust reader',glyphs=len(mapped(kbits(reread))),sha256=sha(reverse)))
    # U8/M importer convention exposes glyph zero as U+0000; inspect its
    # underlying native API as well, including every unencoded glyph and alias.
    java_src=Path(__file__).resolve().parents[2]/'reports/w5.e1/oracles/U8Oracle.java'
    run('javac','-cp',jar,'-d',out,java_src)
    for filename in ['original.u8m','java-original.u8m']:
        text=run('java','-cp',str(out)+os.pathsep+str(jar),'U8Oracle',out/filename).decode()
        (out/(filename+'.api.tsv')).write_text(text)
        rust=inspect('u8m',out/filename)
        slots={g['glyph_id']:g for g in rust['glyphs']+rust['unencoded_glyphs']}
        mapping={(g['domain'],g['cp']):g['glyph_id'] for g in rust['glyphs']}
        observed={};glyph_count=0
        for line in text.splitlines():
            p=line.split('\t')
            if p[0]=='FONT':assert list(map(int,p[4:6]))==[rust['ascent'],rust['descent']]
            elif p[0]=='MAP':observed[(p[1],int(p[2]))]=int(p[3])
            elif p[0]=='GLYPH':
                i,w,h,x,y,adv=map(int,p[1:7]);g=slots[i];assert (w,h,x,y,adv)==tuple(g[k] for k in ['width','height','x','y','advance']);assert list(bytes.fromhex(p[7]))==g['pixels'];glyph_count+=1
        assert mapping==observed and glyph_count==len(slots)
        checks.append(dict(format='u8m',direction='author native U8MFile API validates all glyphs and mapping domains',fixture=filename,glyphs=glyph_count,mappings=len(observed)))
    for name in ['original.rb12','original-gray.rb12','original-large.rb12']:
        actual=json.loads(run(a.rockbox_reader,out/name));expected=json.loads((out/(name+'.expected.json')).read_text());assert actual['ascent']==expected['ascent'] and actual['height']==expected['line_height']
        assert len(actual['glyphs'])==len(expected['glyphs'])
        for left,right in zip(actual['glyphs'],expected['glyphs']):assert left['cp']==right['cp'] and left['width']==right['advance'] and left['pixels']==right['pixels']
        checks.append(dict(format='rockbox',direction='Rust writer -> official unmodified Rockbox runtime loader',fixture=name,glyphs=len(actual['glyphs']),pixels=sum(len(g['pixels']) for g in actual['glyphs'])))
    # Original Lua adapter uses public runtime APIs to measure each glyph, pixel,
    # and pair. pdc copies the native PFT byte-for-byte into the game bundle.
    game=out/'runtime-source';game.mkdir(exist_ok=True)
    (game/'native.pft').write_bytes((out/'original.pft').read_bytes())
    (game/'embedded.fnt').write_bytes((out/'original-playdate.fnt').read_bytes())
    (game/'split.fnt').write_bytes((out/'original-split.fnt').read_bytes())
    for p in out.glob('original-split-table-*.png'):(game/p.name.replace('original-split','split')).write_bytes(p.read_bytes())
    bundle='org.fontpixel.w5e1oracle'
    (game/'pdxinfo').write_text(f'name=Original font interoperability test\nauthor=fontpixel\nbundleID={bundle}\nversion=1.0\nbuildNumber=1\n')
    (game/'main.lua').write_text('''local output={}
for _,name in ipairs({"native","embedded","split"}) do
  local f,err=playdate.graphics.font.new(name)
  assert(f,err)
  table.insert(output,string.format("FONT\\t%s\\t%d\\t%d",name,f:getHeight(),f:getTracking()))
  for _,cp in ipairs({65,66,67,256,131072}) do
    local s=utf8.char(cp)
    local image=f:getGlyph(s)
    assert(image,"missing glyph")
    local w,h=image:getSize()
    local row={}
    for y=0,h-1 do for x=0,w-1 do table.insert(row,image:sample(x,y)==playdate.graphics.kColorBlack and "1" or "0") end end
    table.insert(output,string.format("GLYPH\\t%s\\t%d\\t%d\\t%d\\t%d\\t%s",name,cp,w,h,f:getTextWidth(s..s)-f:getTextWidth(s),table.concat(row)))
  end
  for _,pair in ipairs({{65,66},{65,256},{131072,65}}) do
    local x,y=utf8.char(pair[1]),utf8.char(pair[2])
    table.insert(output,string.format("KERN\\t%s\\t%d\\t%d\\t%d",name,pair[1],pair[2],f:getTextWidth(x..y)-f:getTextWidth(x)-f:getTextWidth(y)))
  end
end
local file,err=playdate.file.open("result.tsv",playdate.file.kFileWrite)
assert(file,err)
file:write(table.concat(output,"\\n").."\\n")
file:close()
function playdate.update() end
''')
    pdx=out/'runtime.pdx';run(sdk/'bin/pdc','-sdkpath',sdk,'-u',game,pdx)
    assert sha(pdx/'native.pft')==sha(game/'native.pft')
    result=sdk/'Disk/Data'/bundle/'result.tsv';result.unlink(missing_ok=True)
    config=out/'simulator-config'/'Playdate Simulator';config.mkdir(parents=True,exist_ok=True)
    (config/'Playdate Simulator.ini').write_text('ShowPerfWarning=0\nShowElist=0\n')
    env=dict(os.environ,PLAYDATE_SDK_PATH=str(sdk),DISPLAY=a.display,GDK_BACKEND='x11',NO_AT_BRIDGE='1',GIO_USE_VFS='local',GTK_USE_PORTAL='0',DBUS_SESSION_BUS_ADDRESS='unix:path='+str(out/'unused-session-bus'),XDG_CONFIG_HOME=str(config.parent),XDG_DATA_HOME=str(out/'simulator-data'))
    log=out/'simulator.log'
    with log.open('wb') as capture:
        proc=subprocess.Popen([str(sdk/'bin/PlaydateSimulator'),str(pdx)],env=env,stdout=capture,stderr=subprocess.STDOUT)
        deadline=time.monotonic()+20
        while not result.exists() and proc.poll() is None and time.monotonic()<deadline:time.sleep(.1)
        found=result.exists()
        proc.terminate()
        try:code=proc.wait(timeout=3)
        except subprocess.TimeoutExpired:proc.kill();code=proc.wait()
    commands.append({'argv':[str(sdk/'bin/PlaydateSimulator'),str(pdx)],'exit':code,'controlled_stop':True,'result_observed':found,'log_sha256':sha(log)})
    assert found,log.read_text(errors='replace')
    text=result.read_text();(out/'runtime-result.tsv').write_text(text)
    expected=json.loads((out/'original.pft.expected.json').read_text());glyphs={g['cp']:g for g in expected['glyphs']};observed=0
    for line in text.splitlines():
        p=line.split('\t')
        if p[0]=='FONT':assert list(map(int,p[2:]))==[7,2]
        elif p[0]=='GLYPH':
            cp,w,h,advance=map(int,p[2:6]);g=glyphs[cp];assert(w,h,advance)==(g['width'],g['height'],g['advance']);assert [255 if c=='1' else 0 for c in p[6]]==g['pixels'];observed+=1
        elif p[0]=='KERN':assert int(p[4])=={(65,66):-2,(65,256):-3,(131072,65):1}[tuple(map(int,p[2:4]))]
    assert observed==15
    checks.append(dict(format='playdate',direction='Rust FNT embedded/split/PFT -> Panic SDK 3.1.2 compiler + Simulator runtime',glyphs=15,pixels=15*9*7,kerning_pairs=9))
    report={'checks':checks,'commands':commands,'tools':{'helper_sha256':sha(binary),'jar_sha256':sha(jar),'rockbox_reader_sha256':sha(a.rockbox_reader),'pdc_sha256':sha(sdk/'bin/pdc'),'simulator_sha256':sha(sdk/'bin/PlaydateSimulator')},'fixtures':{p.name:sha(p) for p in out.iterdir() if p.is_file()}}
    (out/'crosscheck.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'checks':len(checks),'native_runtime_glyphs':observed,'output':str(out/'crosscheck.json')}))
if __name__=='__main__':main()
