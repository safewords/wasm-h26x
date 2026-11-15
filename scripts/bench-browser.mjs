// Benchmark the variants in real browsers (Playwright), including the threads
// builds, which need a cross-origin-isolated page — the in-process server
// here sends COOP/COEP — and the decoder's worker threads as Web Workers, which
// Node has no way to run.
//
//   node scripts/bench-browser.mjs [--files h264-720p.264,hevc-1080p.265] [--frames 300]
//                                  [--threads 1,2,4,8] [--browsers chromium,firefox,webkit]
//                                  [--variants baseline,simd] [--runs 3] [--json out.json]
//
// Decodes inside a Worker (scripts/bench-worker.js) with the frames consumed
// there, so the ms/frame is decode alone, by the wall clock of the whole
// decode — of a second pass over the clip, once the thread Workers are up
// and the JIT has tiered up (the first pass is printed as "cold"). Clips from scripts/fetch-samples.sh under testdata/samples/.
// Prints a table per browser and checks that every configuration produced
// the same frame hash.

import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { extname, join, normalize, resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (name, def) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : def; };
const files = opt('--files', 'h264-720p.264,h264-1080p.264,hevc-720p.265,hevc-1080p.265').split(',');
const maxFrames = Number(opt('--frames', 300));
const threadCounts = opt('--threads', '1,2,4,8').split(',').map(Number);
const browsers = opt('--browsers', 'chromium,firefox,webkit').split(',');
const singleVariants = opt('--variants', 'baseline,simd').split(',');
// Each configuration is decoded `runs` times (a fresh Worker and wasm
// instance each time) and the median reported, with the spread.
const runs = Number(opt('--runs', 3));
const jsonOut = opt('--json', null);
const rows = [];

// A Playwright install: PLAYWRIGHT_DIR, else this package's node_modules.
const pwDir = process.env.PLAYWRIGHT_DIR
  ?? [join(root, 'node_modules', 'playwright')]
    .find((p) => { try { statSync(p); return true; } catch { return false; } })
  ?? 'playwright';
const pw = await import(pathToFileURL(join(pwDir, 'index.mjs')).href);

const types = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.wasm': 'application/wasm', '.264': 'application/octet-stream', '.265': 'application/octet-stream', '.json': 'application/json' };
const server = createServer((req, res) => {
  let file = normalize(join(root, decodeURIComponent(new URL(req.url, 'http://x').pathname)));
  try {
    if (statSync(file).isDirectory()) file = join(file, 'index.html');
    res.writeHead(200, {
      'Content-Type': types[extname(file)] || 'application/octet-stream', 'Content-Length': statSync(file).size,
      'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp',
    });
    createReadStream(file).pipe(res);
  } catch { res.writeHead(404); res.end(); }
});
await new Promise((r) => server.listen(0, r));
const base = `http://localhost:${server.address().port}`;

// (variant, threads) configurations: the single-threaded builds, then the
// threads builds on each count. The first is the reference hash.
const configs = singleVariants.map((variant) => ({ variant, threads: 1 }));
for (const t of threadCounts) {
  if (t > 1) {
    configs.push({ variant: 'threads', threads: t });
    configs.push({ variant: 'simd-threads', threads: t });
  }
}

for (const name of browsers) {
  let browser;
  try {
    browser = await pw[name].launch();
  } catch (e) {
    console.log(`${name}: not available (${e.message.split('\n')[0]})`);
    continue;
  }
  const page = await browser.newPage();
  await page.goto(`${base}/demo/`);
  const hc = await page.evaluate(() => navigator.hardwareConcurrency);
  console.log(`\n${name} (hardwareConcurrency ${hc}, crossOriginIsolated ${await page.evaluate(() => crossOriginIsolated)})`);
  console.log('clip'.padEnd(18) + 'variant'.padEnd(14) + 'threads  ms/frame   fps (range)   cold fps  worst run  hash');
  for (const file of files) {
    let refHash = null;
    for (const cfg of configs) {
      const all = [];
      for (let k = 0; k < runs; k++) {
      const r = await page.evaluate(async ({ base, file, cfg, maxFrames }) => {
        const { spawnThreadWorker } = await import(`${base}/js/loader.js`);
        const es = await (await fetch(`${base}/testdata/samples/${file}`)).arrayBuffer();
        return new Promise((resolve) => {
          const w = new Worker(`${base}/scripts/bench-worker.js`, { type: 'module' });
          const timer = setTimeout(() => resolve({ type: 'error', message: 'timeout' }), 600000);
          // The thread Workers this run's instance asked for: the page made
          // them, so the page ends them (a blocked one holds the instance's
          // whole shared memory otherwise, run after run).
          const threads = [];
          w.onmessage = ({ data }) => {
            if (data.type === 'spawnThread') { threads.push(spawnThreadWorker(data)); return; }
            clearTimeout(timer);
            w.terminate();
            for (const t of threads) t.terminate();
            resolve(data);
          };
          w.onerror = (e) => { clearTimeout(timer); resolve({ type: 'error', message: e.message }); };
          w.postMessage({ variant: cfg.variant, baseUrl: `${base}/pkg/`, threads: cfg.threads, es, maxFrames }, [es]);
        });
      }, { base, file, cfg, maxFrames });
      all.push(r);
      if (r.type === 'error') break;
      }
      const errs = all.filter((x) => x.type === 'error');
      const ok = all.filter((x) => x.type !== 'error').sort((a, b) => a.msPerFrame - b.msPerFrame);
      const r = errs.length ? errs[0] : { ...ok[Math.floor(ok.length / 2)], spread: [ok[0].msPerFrame, ok[ok.length - 1].msPerFrame], hashes: new Set(ok.map((x) => x.hash)).size };
      if (r.type === 'error') {
        console.log(`${file.padEnd(18)}${cfg.variant.padEnd(14)}${String(cfg.threads).padStart(7)}  ERROR ${r.message.split('\n')[0]}`);
        continue;
      }
      if (refHash === null) refHash = r.hash;
      const same = r.hash === refHash && r.hashes === 1 ? 'same' : 'DIFFERENT';
      rows.push({ browser: name, file, variant: r.variant, threads: r.threads, frames: r.frames, msPerFrame: r.msPerFrame, coldFps: 1000 / r.coldMsPerFrame, fps: 1000 / r.msPerFrame, fpsRange: r.spread.map((m) => 1000 / m).reverse(), worstRunMs: r.worstRunMs, same: same === 'same' });
      console.log(`${file.padEnd(18)}${r.variant.padEnd(14)}${String(r.threads).padStart(7)}  ${r.msPerFrame.toFixed(2).padStart(8)}  ${(1000 / r.msPerFrame).toFixed(0).padStart(4)} (${(1000 / r.spread[1]).toFixed(0)}–${(1000 / r.spread[0]).toFixed(0)})  ${(1000 / r.coldMsPerFrame).toFixed(0).padStart(5)}  ${r.worstRunMs.toFixed(1).padStart(8)}ms  ${same}${r.stats.decodeErrors ? `  errors ${r.stats.decodeErrors}` : ''}`);
    }
  }
  await browser.close();
}
server.close();
if (jsonOut) (await import('node:fs')).writeFileSync(jsonOut, JSON.stringify(rows, null, 1));
