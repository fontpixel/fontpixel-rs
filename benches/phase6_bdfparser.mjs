// Reconstructed full-input v2. Build only the copied original JS source bundle.
import {readFileSync, writeFileSync, mkdirSync, readdirSync, realpathSync} from 'node:fs';
import {resolve, relative, join} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
if (process.argv.length !== 5) throw new Error('phase6_bdfparser.mjs COPIED_BUNDLE INPUT OUTPUT');
if (typeof global.gc !== 'function') throw new Error('run Node with --expose-gc');
const [, , modulePath, source, output] = process.argv;
const protectedRoot = resolve(fileURLToPath(new URL('../../reference/', import.meta.url)));
if (!relative(protectedRoot, realpathSync(modulePath)).startsWith('..')) throw new Error('use a writable copied reference bundle');
const {Font, Bitmap, setWarningHandler} = await import(pathToFileURL(resolve(modulePath)));
const diagnostics = []; setWarningHandler(message => diagnostics.push(message));
mkdirSync(output, {recursive:true});
if (readdirSync(output).length) throw new Error('output directory must be empty');
const data = new Uint8Array(readFileSync(source));
const metrics = {};
function once(name, operation) {
  global.gc(); let warmup = operation(); warmup = null; global.gc();
  const start = process.hrtime.bigint(); const value = operation();
  metrics[name] = Number(process.hrtime.bigint() - start); return value;
}
function raster(name, bitmap) {
  const width=bitmap.width(), height=bitmap.height(), stride=Math.ceil(width/8);
  const packed=Buffer.alloc(12+stride*height); packed.write('P6R1'); packed.writeUInt32BE(width,4); packed.writeUInt32BE(height,8);
  for(let y=0;y<height;y++) for(let x=0;x<width;x++) {
    const p=bitmap.bindata[y][x];
    if(p==='1') packed[12+y*stride+Math.floor(x/8)] |= 0x80 >> (x%8);
    else if(p!=='0') throw new Error('unexpected nonbinary raster label');
  }
  writeFileSync(join(output,name),packed);
}
const font=once('parse',()=>new Font().load_bytes(data));
writeFileSync(join(output,'compat.bdf'),once('write_bdf',()=>new TextEncoder().encode(font.tobdf())));
writeFileSync(join(output,'compat.pcf'),once('write_pcf',()=>font.topcf()));
writeFileSync(join(output,'compat.ttf'),once('write_ttf',()=>font.tottf()));
const text='永'.repeat(2000);
let bitmap=once('render_screen',()=>font.draw(text,{mode:0,linelimit:80*font.headers.fbbx}));
raster('screen.raster',bitmap);bitmap=null;
bitmap=once('drawall_mode1',()=>font.drawall({mode:1,linelimit:512}));raster('drawall.raster',bitmap);bitmap=null;
const small=new Bitmap(Array.from({length:64},(_,y)=>Array.from({length:64},(_,x)=>(x*13+y*7)%11<4?'1':'0').join('')));
bitmap=once('crop',()=>small.clone().crop(31,31,3,5));raster('crop.raster',bitmap);bitmap=null;
bitmap=once('concat',()=>small.clone().concat(small,{direction:1,align:1,offset:0}));raster('concat.raster',bitmap);bitmap=null;
writeFileSync(join(output,'warnings.report.json'),JSON.stringify(diagnostics)+'\n');
console.log(JSON.stringify({schema:2,profile:'reconstructed-full-input-v2',engine:'node',node:process.version,
  glyphs:font.length,input_bytes:data.length,metrics_ns:metrics,warmup_calls:1,measured_calls:1,atlas_mode:1,
  gc:'forced before/after warmup outside timer; automatic in-call GC included',
  small_bitmap:'64x64,(13*x+7*y)%11<4; crop31x31@(3,5); concat right/end/gap0; immutable input via timed clone'}));
