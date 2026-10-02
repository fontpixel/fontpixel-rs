// Build the supplied read-only TypeScript source to a temporary module first:
// npm exec --yes --package=esbuild -- esbuild ../reference/bdfparser-js/src/index.ts \
//   --bundle --platform=node --format=esm --outfile=/tmp/fontpixel-bdf-js/bdfparser.mjs
// node --expose-gc benches/bdf_node.mjs INPUT.bdf /tmp/fontpixel-bdf-js/bdfparser.mjs
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {performance} from 'node:perf_hooks';
const {Font,setWarningHandler}=await import(pathToFileURL(process.argv[3]));
setWarningHandler(null);
const bytes=new Uint8Array(readFileSync(process.argv[2]));
const font=new Font().load_bytes(bytes);
const encoder=new TextEncoder();
encoder.encode(font.tobdf());
let reads=[],writes=[],outputBytes=0;
for(let i=0;i<5;i++){
  global.gc?.();
  const start=performance.now();
  let parsed=new Font().load_bytes(bytes);
  reads.push(performance.now()-start);
  if(!parsed.length)throw new Error('empty benchmark font');
  parsed=null;
}
for(let i=0;i<5;i++){
  global.gc?.();
  const start=performance.now();
  let written=encoder.encode(font.tobdf());
  writes.push(performance.now()-start);outputBytes=written.length;
  written=null;
}
console.log(JSON.stringify({path:process.argv[2],node:process.version,input_bytes:bytes.length,glyphs:font.length,output_bytes:outputBytes,read_ms:reads,write_ms:writes}));
