// Same resident font, inputs, warmups and five single calls as render_timing.rs.
import {readFileSync} from 'node:fs';import {pathToFileURL} from 'node:url';import {performance} from 'node:perf_hooks';
const {Font,setWarningHandler}=await import(pathToFileURL(process.argv[3]));setWarningHandler(null);
const f=new Font().load_bytes(new Uint8Array(readFileSync(process.argv[2]))),text='永'.repeat(2000),opts={linelimit:80*f.headers.fbbx,mode:0};
const screen=f.draw(text,opts),all=f.drawall();
const sample=fn=>Array.from({length:5},()=>{const start=performance.now();let result=fn();const elapsed=performance.now()-start;result=null;return elapsed;});
console.log(JSON.stringify({node:process.version,path:process.argv[2],glyphs:f.length,screen_characters:2000,screen_width:screen.width(),screen_height:screen.height(),drawall_width:all.width(),drawall_height:all.height(),screen_ms:sample(()=>f.draw(text,opts)),drawall_ms:sample(()=>f.drawall())}));
