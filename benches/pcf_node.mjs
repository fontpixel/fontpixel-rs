// Uses the read-only reference bundle described in benches/bdf_node.mjs.
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {performance} from 'node:perf_hooks';
import {createHash} from 'node:crypto';
const {Font,setWarningHandler}=await import(pathToFileURL(process.argv[3]));setWarningHandler(null);
const bytes=new Uint8Array(readFileSync(process.argv[2]));const font=new Font().load_bytes(bytes);const wire=font.topcf();
const sample=fn=>{let result=[];for(let i=0;i<5;i++){global.gc?.();let start=performance.now();let value=fn();result.push(performance.now()-start);value=null;}return result;};
console.log(JSON.stringify({path:process.argv[2],node:process.version,glyphs:font.length,input_bytes:bytes.length,pcf_bytes:wire.length,sha256:createHash('sha256').update(wire).digest('hex'),bdf_parse_ms:sample(()=>new Font().load_bytes(bytes)),pcf_write_ms:sample(()=>font.topcf()),pcf_read_ms:sample(()=>new Font().load_bytes(wire)),bdf_to_pcf_ms:sample(()=>new Font().load_bytes(bytes).topcf())}));
