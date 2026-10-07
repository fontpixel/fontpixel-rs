import {chromium,firefox,webkit} from '/home/chen/githubprojects/fontpixel/fontpixel.com/site/node_modules/@playwright/test/index.mjs';
import {readFile,writeFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {gzipSync} from 'node:zlib';
import os from 'node:os';
const root='/home/chen/githubprojects/fontpixel',pkg=root+'/local-work/target-fontpixel-rs/wasm-pkg/web',site='http://localhost:4399';
const info=JSON.parse(await readFile(pkg+'/build-info.json','utf8'));
if(info.build_mode!=='release'||info.wasm_opt==='off'||info.source_dirty)throw new Error('Require clean release artifacts');
const js=await readFile(root+'/fontpixel.com/site/public/wasm/fontpixel.js');
const result={date:new Date().toISOString(),node:process.version,cpu:os.cpus()[0].model,build:info,jsBytes:js.length,jsGzipBytes:gzipSync(js,{level:9}).length,nodeInit:[],chromiumInit:[],generation:[],method:'Five fresh Node/Chromium processes; input bytes preloaded, warm OS cache; JS import, wasm compile and instantiate separated. Browser JS import includes loopback fetch. Generation uses the production UI and Worker, cache cleared; workerMs excludes initialization, wallMs includes initialization, cache write and result delivery (not all deferred sample paints). Shared host load recorded.'};
const out=root+'/local-work/p9/evidence/measurements.json';
const save=()=>writeFile(out,JSON.stringify(result,null,2)+'\n');
for(let repeat=0;repeat<5;repeat++){
  const load=os.loadavg();
  const node=JSON.parse(execFileSync(process.execPath,[root+'/fontpixel-rs/benches/phase8_init_node.mjs',pkg,root+'/fontpixel-rs/tests/data/bdf/3x5-block.bdf'],{encoding:'utf8'}));
  result.nodeInit.push({...node,repeat,load});
  const browser=await chromium.launch();
  try{
    result.chromiumVersion=browser.version();const page=await browser.newPage();await page.goto(site+'/robots.txt');
    const measured=await page.evaluate(async()=>{
      const bytes=await(await fetch('/wasm/fontpixel_wasm_bg.wasm')).arrayBuffer();
      const t0=performance.now(),url='/wasm/fontpixel.js',fp=await import(url),t1=performance.now();
      const module=await WebAssembly.compile(bytes),t2=performance.now();await fp.init(module);const t3=performance.now();
      return {importMs:t1-t0,compileMs:t2-t1,instantiateMs:t3-t2,totalMs:t3-t0};
    });
    result.chromiumInit.push({...measured,repeat,load:os.loadavg()});
  }finally{await browser.close();}
  await save();
}
for(const [name,engine,executablePath] of [['firefox',firefox,'/home/chen/.cache/ms-playwright/firefox-1543/firefox/firefox'],['webkit',webkit,root+'/local-work/p8-tools/webkit-local.sh']]){
  const browser=await engine.launch({executablePath});
  result[name+'Version']=browser.version();await browser.close();
}
const browser=await chromium.launch();
try{
  for(const slug of ['spleen','cozette','poxiao-pixel'])for(const format of ['bdf','woff2'])for(let repeat=0;repeat<5;repeat++){
    const context=await browser.newContext(),page=await context.newPage();
    try{
      await page.addInitScript(()=>{const Native=window.Worker;window.Worker=class extends Native{constructor(url,options){super(url,options);this.addEventListener('message',e=>{if(e.data.type==='result')window.__measureResult=e.data.result;if(e.data.type==='error')window.__measureError=e.data.error;});}};});
      await page.goto(`${site}/font-tool/?font=${slug}&format=${format}`);await page.locator('fieldset.source').waitFor();
      await page.getByRole('button',{name:'Clear generated-font cache',exact:true}).click();await page.getByTestId('tool-status').filter({hasText:'cleared'}).waitFor();
      const start=performance.now();await page.getByRole('button',{name:'Generate preview',exact:true}).click();
      await page.waitForFunction(()=>window.__measureResult||window.__measureError,{},{timeout:180000});
      const detail=await page.evaluate(()=>window.__measureError?{error:window.__measureError}:({workerMs:window.__measureResult.ms,glyphs:window.__measureResult.preview.glyphs,cache:window.__measureResult.cache,bytes:Object.fromEntries(Object.entries(window.__measureResult.files).map(([k,v])=>[k,v.length]))}));
      result.generation.push({slug,format,repeat,wallMs:performance.now()-start,load:os.loadavg(),...detail});console.log(slug,format,repeat,detail.workerMs??detail.error);
      await save();
      if(detail.error)throw new Error(JSON.stringify(detail.error));
      if(detail.cache==='hit')throw new Error('Generation benchmark hit cache');
    }finally{await context.close();}
    await save();
  }
}finally{await browser.close();await save();}
