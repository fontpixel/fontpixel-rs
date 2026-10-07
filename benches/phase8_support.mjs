// Shared, loopback-only harness support. Node tooling is installed outside the repository.
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
export const WORK = path.resolve(process.env.FONTPIXEL_P8_WORK ?? path.join(ROOT, '../local-work/p8-redo'));
export const TOOLS = path.resolve(process.env.FONTPIXEL_JS_TOOLS ?? path.join(ROOT, '../local-work/p8-tools'));
export const PKG = path.resolve(process.env.FONTPIXEL_WASM_PKG ?? path.join(process.env.CARGO_TARGET_DIR ?? path.join(ROOT, 'target'), 'wasm-pkg/full'));
export const requireTool = createRequire(path.join(TOOLS, 'package.json'));
export const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
export const git = (...args) => execFileSync('git', args, { cwd: ROOT, encoding: 'utf8' }).trim();
export const metadata = () => ({ date: new Date().toISOString(), commit: git('rev-parse', 'HEAD'), dirty: !!git('status', '--porcelain', '--untracked-files=no'), node: process.version, platform: `${os.platform()} ${os.release()} ${os.arch()}`, cpu: os.cpus()[0].model, cpus: os.cpus().length, load: os.loadavg(), package: JSON.parse(readFileSync(path.join(PKG, 'build-info.json'))) });
export function save(file, value) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, JSON.stringify(value, null, 2) + '\n');
}
export async function browserFor(engine) {
  const playwright = requireTool('playwright');
  const options = { headless: true };
  if (engine === 'webkit' && process.env.FONTPIXEL_WEBKIT_EXECUTABLE) options.executablePath = process.env.FONTPIXEL_WEBKIT_EXECUTABLE;
  return playwright[engine].launch(options);
}

/** Only explicitly mounted directories are served; no network or filesystem writes. */
export async function serve(mounts) {
  const errors = [];
  const server = createServer(async (req, res) => {
    try {
      const pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
      if (pathname !== '/plain') {
        res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
        res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
      }
      res.setHeader('Cache-Control', 'no-store');
      if (['/', '/plain'].includes(pathname)) {
        res.setHeader('Content-Type', 'text/html');
        res.end('<!doctype html><meta charset="utf-8"><title>fontpixel phase 8 harness</title>');
        return;
      }
      if (pathname === '/favicon.ico') { res.writeHead(204); res.end(); return; }
      for (const [prefix, directory] of Object.entries(mounts)) {
        if (!pathname.startsWith(prefix)) continue;
        const root = path.resolve(directory);
        const file = path.resolve(root, pathname.slice(prefix.length));
        if (!file.startsWith(root + path.sep) || !(await stat(file)).isFile()) break;
        res.setHeader('Content-Type', ({ '.wasm': 'application/wasm', '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json' })[path.extname(file)] ?? 'application/octet-stream');
        res.end(await readFile(file));
        return;
      }
      throw new Error(`unmounted resource ${pathname}`);
    } catch (error) {
      errors.push(String(error));
      res.writeHead(404); res.end('not found');
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { url: `http://127.0.0.1:${server.address().port}`, errors, close: () => new Promise((resolve, reject) => server.close((err) => err ? reject(err) : resolve())) };
}
