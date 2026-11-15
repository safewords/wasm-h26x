// A threads build driven from Node, for test/decode.test.mjs.
//
//   node test/threads-node.mjs <threads|simd-threads> <thread count>
//
// Decodes every elementary-stream fixture in testdata/ and prints one line
// of JSON: the thread count the decoder applied, how many thread Workers the
// wasm asked for, and each fixture's per-frame MD5s — twice over, see below.
//
// In a browser a wasm thread is a Web Worker running pkg/thread-worker.js,
// and the decoder has to live in a Worker of its own because it blocks. Node
// lets its main thread block, so here the decoder runs on the main thread and
// each wasm thread is a `worker_threads` Worker running this same file: the
// Node stand-in for thread-worker.js (same module, same shared memory, same
// `__wasm_h26x_thread_entry`). It is a process of its own because a decoder
// that waits for a thread forever cannot be timed out from inside; the test
// that spawns it can.

import { Worker, isMainThread, workerData } from 'node:worker_threads';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

if (!isMainThread) {
  // One wasm thread: it takes rivet-h26x's pool workers, one after another,
  // for as long as the instance lives.
  const { glue, module, memory, ptr, stackSize } = workerData;
  const mod = await import(glue);
  await mod.default({ module_or_path: module, memory, thread_stack_size: stackSize });
  mod.__wasm_h26x_thread_entry(ptr);
} else {
  const [variant, threadsArg] = process.argv.slice(2);
  const threads = Number(threadsArg);
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const dir = path.join(root, 'pkg', variant);
  const glue = pathToFileURL(path.join(dir, 'wasm_h26x.js')).href;
  const mod = await import(glue);
  const wasm = await mod.default({ module_or_path: readFileSync(path.join(dir, 'wasm_h26x_bg.wasm')) });

  let spawned = 0;
  globalThis.__wasmH26xSpawnThread = (module, memory, ptr) => {
    spawned++;
    const w = new Worker(fileURLToPath(import.meta.url), { workerData: { glue, module, memory, ptr, stackSize: 2 * 1024 * 1024 } });
    w.on('error', (e) => {
      console.error('wasm thread died:', e);
      process.exit(3);
    });
  };

  const testdata = path.join(root, 'testdata');
  const fixtures = readdirSync(testdata).filter((f) => f.endsWith('.264') || f.endsWith('.265')).sort();
  let applied = 0;
  const decodeAll = () => {
    const out = {};
    for (const f of fixtures) {
      const dec = new mod.H26xDecoder(4, threads);
      applied = dec.threads();
      dec.setSourceAnnexB(readFileSync(path.join(testdata, f)));
      const md5s = [];
      while (!dec.finished()) {
        dec.run();
        while (dec.nextFrame()) {
          // A copy: the memory is shared, and a hash may not read a SharedArrayBuffer.
          const view = new Uint8Array(wasm.memory.buffer, dec.framePtr(), dec.frameLen()).slice();
          md5s.push(createHash('md5').update(view).digest('hex'));
        }
      }
      dec.free();
      out[f.replace(/\.26[45]$/, '')] = md5s;
    }
    return out;
  };

  // First with the Workers only just asked for: the decoder must not wait
  // for a thread that is not up yet. Then again once they are waiting for
  // work, which is the pool doing the decoder's parallel jobs.
  const cold = decodeAll();
  const spawnedCold = spawned;
  await new Promise((r) => setTimeout(r, 500));
  const warm = decodeAll();
  console.log(JSON.stringify({
    variant, simd: mod.simdEnabled(), threadsSupported: mod.threadsSupported(), applied,
    spawnedCold, spawned, spawnedByWasm: mod.threadWorkersSpawned(), cold, warm,
  }));
  // The Workers wait for work for ever.
  process.exit(0);
}
