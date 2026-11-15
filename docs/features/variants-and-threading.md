# wasm variants, detection and threading

> **Status:** PARTIAL — the four variants decode every fixture exactly in Node, and play in Chromium, Firefox and WebKit, the threads builds on real Workers; a thread Worker that never starts hangs the decoder and every later reset of it, and the threads hook is on an unmerged rivet-h26x branch.
> **Owners (code):** `js/detect.js`, `js/loader.js`, `src/threads.rs`, `js/thread-worker.js`, `scripts/build.sh`, `tools/wasm-copy-guard`, `Cargo.toml`
> **Related docs:** [js-api-and-worker.md](./js-api-and-worker.md), [build-and-release.md](./build-and-release.md), [`../performance.md`](../performance.md), [`../integration.md`](../integration.md)
> **Last verified against code:** 2026-11-15 @ `616be49`

## Overview

There are four builds of the same crate under `pkg/`, and the loader picks
one at run time. WebAssembly has no feature query, so detection validates
tiny modules that use exactly the instruction in question. The threads
builds run rivet-h26x's worker pool on Web Workers sharing one
`WebAssembly.Memory`: several pictures at once (frame threading, both
codecs), and an H.265 picture's wavefront rows, tiles and slice segments
in parallel. That needs a cross-origin-isolated page (COOP + COEP) and the
decoder itself in a Worker, because the decoding thread blocks on
`memory.atomic.wait` while it waits for the others.

The JS side (detection, loader, the spawn relay through the page, the
thread Worker script) is wasm-av1's, renamed. The Rust side differs:
rivet-av1 had one long-lived pool per instance; rivet-h26x makes a pool per
decoder and lets it go when the decoder is dropped, so `src/threads.rs`
keeps the Workers and gives them pool workers to run, one after another.

What the variants buy in speed is measured separately: see
[`../performance.md`](../performance.md).

| Variant | Rust flags (`scripts/build.sh:113-121`) | Engines |
|---|---|---|
| `baseline` | rustc's default wasm32 features | Chrome 96, Firefox 89, Safari 15 |
| `simd` | `+simd128`: rivet-h26x's `v128` kernels and the SIMD RGBA path | Chrome 91, Firefox 89, Safari 16.4 |
| `threads` | `+atomics,+bulk-memory,+mutable-globals`, shared imported memory (max 1 GiB), `-Zbuild-std`; then `tools/wasm-copy-guard` on the `.wasm` | + cross-origin isolation (Safari 15.2) |
| `simd-threads` | both | + cross-origin isolation |

## How it works today

### Interfaces

| Kind | Name | Notes |
|---|---|---|
| JS | `detectSimd()`, `detectBaseline()`, `detectThreads()` | `js/detect.js:13`, `36`, `62-81`; `detectThreads` also requires `SharedArrayBuffer`, `crossOriginIsolated` and `Worker` |
| JS | `chooseVariant({threads})` | SIMD first, then baseline; threaded only if asked and possible; `null` if nothing runs (`js/detect.js:91-96`) |
| JS | `loadWasmH26x({variant, threads, baseUrl, fetch})` | Imports `pkg/<variant>/wasm_h26x.js` by URL; one cached runtime per `variant@baseUrl` (`js/loader.js:85-126`) |
| JS | `spawnThreadWorker(msg)`, `THREAD_STACK_SIZE` (2 MiB) | `js/loader.js:30`, `59-64` |
| Global | `globalThis.__wasmH26xSpawnThread(module, memory, ptr)` | Installed by the loader in whichever context instantiated a threads build (`js/loader.js:35-51`) |
| wasm export | `__wasm_h26x_thread_entry(ptr)` | Runs one Worker's loop (`src/threads.rs:88-94`) |
| wasm export | `threadsSupported()`, `threadWorkersSpawned()` | True in the two threads variants (`THREADS_SUPPORTED`, `src/lib.rs:56-57`); Workers asked for so far in this instance (`src/wasm.rs:36-46`) |

### Flow: a thread starting

```mermaid
sequenceDiagram
  participant P as Page (WorkerDecoder)
  participant W as Decode Worker
  participant R as wasm: core + Slots (in W)
  participant T as Thread Worker
  P->>W: init {threads: n}
  W->>W: loadWasmH26x({threads: true}) picks simd-threads
  W->>R: new H26xDecoder(.., n): threads::install() once
  W->>R: first unit: rivet-h26x makes its pool (H.264 n workers, H.265 2n + n/4)
  R->>R: Platform::spawn(body) for each: Slots::give queues it
  alt a Worker is waiting, or still starting
    R->>R: notify it; no new Worker
  else none
    R->>W: __wasmH26xSpawnThread(module, memory, ptr)
    W->>P: postMessage spawnThread {glue, module, memory, ptr, stackSize}
    P->>T: new Worker(pkg/thread-worker.js), postMessage {module, memory, ptr}
    T->>T: import glue, init on the shared memory
    T->>R: __wasm_h26x_thread_entry(ptr): Slots::run, for ever
  end
  loop each body
    T->>T: take a body, run rivet-h26x's worker loop until its pool lets go
  end
  Note over W,R: decoder dropped (flush, new source): the pool waits for each body to return, the Workers go back to waiting
```

The request goes up to the page rather than the decode Worker creating its
own child, because the decode Worker is about to block and Chromium only
starts a nested Worker once its parent returns to the event loop, and
Safari before 16.4 has no nested Workers (`js/loader.js:10-20`). On a page
that instantiates a threads build directly the loader spawns the Worker
itself (`js/loader.js:45-46`).

The wasm side is `src/threads.rs`. rivet-h26x starts its pool workers with
`std::thread` natively; on `wasm32-unknown-unknown` std has neither threads
nor a clock, so rivet-h26x's `h26x::platform` hook lets an embedder supply
both (rivet-h26x `src/platform.rs`, on its `wasm/threads` branch).
`install()` (`src/threads.rs:186-194`), called when a decoder opens with
more than one thread (`src/core.rs:114-117`), registers `spawn_body` and
`performance.now()` once per instance. From then on:

- `Slots::give` (`src/threads.rs:147-167`) queues each worker body
  rivet-h26x asks for. If a Worker is waiting, or has been asked for and is
  still starting, it is woken and takes it; otherwise one more Worker is
  requested. So the number of Workers is about the most pool workers ever
  alive at once, and a second decoder of the same size starts none.
- `Slots::run` (`src/threads.rs:128-145`), the loop each Worker runs for
  ever: take a body, run it until its pool lets it go, take the next.
- `spawn` returns as soon as the Worker is *requested*. Nothing waits for
  it to start: rivet-h26x's pool queues its jobs, and a worker takes them
  when it comes up, so the first pictures of a stream decode on fewer
  threads than asked for rather than later.
- When a decoder is dropped (a flush, a new source, `free`), its pool shuts
  down and, since it cannot join a thread it did not start, waits until
  each of its workers has returned from its body (rivet-h26x
  `src/threading.rs:336-356`). The Worker is then free for the next body.

Mutexes and condvars are std's, which on wasm with atomics are
`memory.atomic.wait` / `notify`; no dependency supplies them.

**How many Workers.** An H.264 decoder of `n` threads has `n` pool
workers; an H.265 one has `2n` for the pictures' rows and `max(n / 4, 1)`
for the loop filters (`test/decode.test.mjs` asserts this). On 2026-11-15,
decoding every fixture in Node, the threads builds asked for 5 Workers at
2 threads, 9 at 4 and 18 at 8, and none more on a second pass over the
same fixtures. Each Worker is an instance of the module with a 2 MiB stack
taken from the shared heap (`js/loader.js:30`): 36 MiB of stacks at 8
threads, kept for the life of the instance.

**Frames in flight.** With more than one thread the core's frames come
out a few access units after their unit went in, and the decoding happens
while the decode Worker is idle. Nothing wakes that Worker when a picture
is done, so `js/worker.js` looks again every 4 ms while `unitsInFlight`
is not 0 and it has no input (`js/worker.js:37-44`, `82-87`).
`unitsInFlight` counts pictures waiting in the reorder buffer too
([decoder-core.md](./decoder-core.md)), so on a live stream that is
waiting for input the Worker keeps polling until more comes or the stream
ends.

Thread count: `H26xPlayer` resolves `threads: 'auto'` to
`hardwareConcurrency` capped at 8 (`js/player.js:20-26`).

### Configuration

- Serve `pkg/` as static files; pass `baseUrl` if it is not next to `js/`
  (`js/loader.js:102`).
- The page must send `Cross-Origin-Opener-Policy: same-origin` and
  `Cross-Origin-Embedder-Policy: require-corp` for any threads variant
  ([`../integration.md`](../integration.md) covers serving).
- `opts.fetch` lets a caller add credentials to the `.wasm` request.

### Edge cases & gotchas

- **A thread Worker that never starts hangs the decoder, and every reset
  of it.** If `pkg/thread-worker.js` fails to import or instantiate, it
  logs to the console and closes (`js/thread-worker.js:19-25`); if
  `__wasmH26xSpawnThread` is missing or throws, the body stays queued for
  a Worker that never comes (`src/threads.rs:159-166`). rivet-h26x's
  platform contract says the decoder then waits for ever, and dropping the
  decoder waits for that body to return, so a `flush`, a new source or
  `free` in the decode Worker blocks it for good. A Worker that traps
  inside a body (a panic is an abort) does the same. Nothing surfaces any
  of it as an `error` message.
- **The threads hook is not in rivet `main`.** `Cargo.toml:55-63` patches
  rivet-h26x to `safewords/rivet-h26x-codecs` at `a26393e` on its
  `wasm/threads` branch, for both this crate and rivet-container. Until
  that branch is merged, a `cargo update` of rivet does not update the
  decoder.
- **WebKit: `memory.copy` and `memory.fill` trap on memory another Worker
  grew.** The JavaScriptCore bug wasm-av1 found on 2026-11-12 applies to
  any threads build: an instance in one Worker keeps the old size of a
  shared memory another Worker grew, and its `memory.copy`/`memory.fill`
  trap on the grown pages. `scripts/build.sh` runs the same
  `tools/wasm-copy-guard` on the two threads variants after wasm-bindgen
  (`scripts/build.sh:81-86`): every `memory.copy` and `memory.fill` becomes
  a call to a helper that falls back to loads and stores beyond the
  instance's own `memory.size`. `test/browser.mjs` reports whether each
  engine has the bug and plays the threads variants in all three;
  `test/copy-guard.test.mjs` tests the pass. wasm-av1's
  `docs/features/variants-and-threading.md` has the whole account.
- **Thread Workers end with the decoder.** A Worker's loop never returns,
  and the page creates it, so terminating the decode Worker does not end
  it. `WorkerDecoder` keeps the Workers it starts and `close()` terminates
  them (`js/worker-client.js:254-261`, `threadWorkerCount`).
- **The fixtures here are small**, so the tests prove that the threads
  builds decode exactly on a pool of real threads, both before the Workers
  are up and once they are waiting for work (`test/threads-node.mjs`), not
  that they are faster.
- **Two decoders on one instance** share the Workers: each asks for its own
  pool workers, and `Slots` hands them to whichever Workers are free, so
  two 8-thread H.265 decoders at once take up to 36 Workers.
- **Memory** is bounded by the 1 GiB a threads build may grow to
  (`scripts/build.sh:62`); what rivet-h26x holds for frame threading was
  not measured. An allocation the heap cannot meet aborts the instance.
- A threads runtime instantiated on the main thread can only use 1 thread:
  the decoder would block the main thread. `H26xPlayer` without
  `worker: true` never asks for a threads build (`js/player.js:103-104`).
- The loader caches runtimes forever (`js/loader.js:24`); every decoder in
  a context shares one wasm instance and one memory, which grows and never
  shrinks, and the Workers live as long as the instance.
- Unlike wasm-av1's, `test/browser.mjs` has no live-push case (units pushed
  on several threads with no end of stream), so the re-poll path is only
  exercised by the Worker playing files and segments.

## History & references

- `1a7e686` (2026-11-15): `src/threads.rs`, `Slots` over rivet-h26x's
  `h26x::platform` (rivet-h26x `a26393e`: "a pool waits, as it is dropped,
  for the workers the embedder started").
- `55e3c2b` (2026-11-15): the JS side, wasm-av1's renamed;
  `test/threads-node.mjs` and the threads combinations of
  `test/browser.mjs`.
- `test/threads-node.mjs`, run by `test/decode.test.mjs`: both threads
  variants on 1, 2, 4 and 8 threads in Node, each wasm thread a
  `worker_threads` Worker, every fixture frame by frame before the Workers
  are up and after. Run on 2026-11-15 against a local build: Node 25/25;
  `test/browser.mjs` 32/32 combinations in each of Chromium, Firefox and
  WebKit (Playwright 1.62).
- wasm-av1's `docs/features/variants-and-threading.md`: the spawn relay,
  the WebKit bug and the copy guard, in full.

## Proposed / Ideas (backlog)

| Idea | Motivation | Sketch | Status |
|---|---|---|---|
| Report a dead thread | A hang becomes an error | `thread-worker.js` posts to the page; `WorkerDecoder` raises `onerror` and closes | Proposed |
| Merge rivet-h26x `wasm/threads` | Drop the `[patch]`; `cargo update` reaches the decoder | Merge upstream, then delete the section and `cargo update -p rivet-h26x` | Proposed |
| Live-push browser case | Exercise the re-poll path | 48 units pushed on four threads, no `eos`, 48 frames expected less the reorder depth | Proposed |
