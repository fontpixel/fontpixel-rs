// Runtime-only bundle of the supplied read-only TypeScript source (see bdf_node.mjs).
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
const {Font,setWarningHandler}=await import(pathToFileURL(process.argv[3]));
setWarningHandler(null);
const data=JSON.parse(readFileSync(process.argv[2],'utf8'));
const fonts=data.fonts.map(f=>new Font().load_string(f.bdf));
let good=0,errors=0;
for(const [i,c] of data.cases.entries()){
 const font=fonts[c.font];
 const options={linelimit:c.limit??512,mode:c.mode??1,direction:c.direction??'lrtb',usecurrentglyphspacing:c.current??false};
 if('missing' in c)options.missing=font.glyphbycp(c.missing);
 let actual,error;
 try{
  const b='all' in c?font.drawall({...options,order:c.all,r:c.ranges}):'text' in c?font.draw(c.text,options):font.drawcps(c.cps,options);
  actual={rows:b.bindata,width:b.width(),height:b.height()};
 }catch(e){error=e;}
 if('error' in c.expected){if(!error)throw Error(`case ${i}: expected error`);errors++;}
 else {if(error)throw error;if(!isDeepStrictEqual(actual,c.expected))throw Error(`case ${i}: mismatch ${JSON.stringify({actual,expected:c.expected,options:c})}`);good++;}
}
console.log(`PASS TypeScript: ${good} pixel/dimension outputs, ${errors} errors`);
