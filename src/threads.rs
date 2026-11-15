//! Threads for the wasm build: the decoder core's worker threads as Web
//! Workers.
//!
//! Only compiled with the `atomics` target feature (the `threads` variants
//! in `pkg/`). A wasm thread is another instance of the same module on the
//! same shared memory, running in a Worker; Rust cannot create one, so the
//! core asks the embedder and this module is that embedder: it boxes the
//! thread body, hands its address to
//! `globalThis.__wasmH26xSpawnThread(module, memory, ptr)` — installed by
//! `js/loader.js` in whichever context instantiated the module — and the
//! Worker that JS ends up creating (`pkg/thread-worker.js`, created by the
//! page at the decode Worker's request: a Worker created by a Worker that
//! then blocks never starts in Chromium) instantiates the module on the
//! shared memory and calls [`thread_entry`] with the address, which runs the
//! body. This is wasm-av1's mechanism, unchanged.
//!
//! What differs is the decoder's side of it. rivet-h26x runs its parallel
//! work on a FIFO pool of worker threads that each decoder makes for itself
//! (an H.264 decoder of `n` threads has `n` workers; an H.265 one `2n` for
//! the pictures' rows and `n / 4` for the loop filters) and that it lets go
//! when it is dropped. Natively those are `std::thread`s. Here
//! [`h26x::platform`] is installed: rivet-h26x asks [`spawn_body`] for each
//! worker, and reads its clock from `performance.now()` ([`now_ms`]), since
//! std has neither on `wasm32-unknown-unknown`.
//!
//! A Worker is far too slow to start for every decoder (a seek makes a new
//! one), so the Workers are kept: each runs [`Slots`]'s loop, which takes a
//! worker body, runs it until its pool lets it go, and waits for the next.
//! A body is given to a Worker that is waiting (or still starting), and a
//! new Worker is asked for only when there is none — so the number of
//! Workers is about the most pool workers that were ever alive at once, and
//! a second decoder of the same size starts none. (rivet-h26x's pool waits,
//! as it is dropped, for its workers to leave; a Worker is back in the loop
//! a moment after that, so a decoder made in that moment may ask for one
//! Worker more.)
//!
//! `spawn` returns as soon as the Worker is *requested*; it starts when its
//! script has loaded. Nothing blocks on that: rivet-h26x's pool queues its
//! jobs, and a worker takes them when it comes up.
//!
//! Requirements on the page: `crossOriginIsolated` (COOP + COEP headers), or
//! there is no `SharedArrayBuffer` and the threads variant cannot even be
//! instantiated — `js/detect.js` checks before choosing it. And the decoder
//! must itself run in a Worker: with more than one thread the core blocks
//! the calling thread on condvars (`memory.atomic.wait`), which browsers
//! forbid on the main thread.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, Once};

use h26x::platform::{Platform, ThreadBody};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// `globalThis.__wasmH26xSpawnThread(module, memory, ptr)` — see js/loader.js.
    #[wasm_bindgen(js_namespace = globalThis, js_name = "__wasmH26xSpawnThread", catch)]
    fn js_spawn_thread(module: JsValue, memory: JsValue, ptr: usize) -> Result<JsValue, JsValue>;

    /// `performance.now()`, in whichever context the calling thread runs.
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;

    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(msg: &str);
}

/// What a Worker runs: [`Slots::run`], for as long as the instance lives.
type WorkerMain = Box<dyn FnOnce() + Send + 'static>;

fn spawn_worker(main: WorkerMain) -> Result<(), ()> {
    // Double-boxed: a `Box<dyn …>` is a fat pointer, the address handed to
    // JS must be thin.
    let ptr = Box::into_raw(Box::new(main)) as usize;
    match js_spawn_thread(wasm_bindgen::module(), wasm_bindgen::memory(), ptr) {
        Ok(_) => Ok(()),
        Err(_) => {
            // SAFETY: the Worker was not created, nobody else has `ptr`.
            drop(unsafe { Box::from_raw(ptr as *mut WorkerMain) });
            Err(())
        }
    }
}

/// Entry point of a thread Worker: run the function whose address
/// `spawn_worker` gave JS. Called exactly once per address, from
/// `pkg/thread-worker.js`.
#[wasm_bindgen(js_name = "__wasm_h26x_thread_entry")]
pub fn thread_entry(ptr: usize) {
    // SAFETY: `ptr` came from `Box::into_raw` in `spawn_worker` and is
    // consumed here, once.
    let main: Box<WorkerMain> = unsafe { Box::from_raw(ptr as *mut WorkerMain) };
    main();
}

/// The Workers, each running one of rivet-h26x's pool workers at a time.
struct Slots {
    state: Mutex<SlotState>,
    /// Workers wait here for a body.
    work: Condvar,
}

struct SlotState {
    /// Bodies no Worker has taken yet.
    queue: VecDeque<ThreadBody>,
    /// Workers waiting for a body.
    idle: usize,
    /// Workers running a body.
    running: usize,
    /// Workers asked for so far. Those neither idle nor running have not
    /// started yet; each will take a body when it does.
    spawned: usize,
}

static SLOTS: Slots = Slots {
    state: Mutex::new(SlotState {
        queue: VecDeque::new(),
        idle: 0,
        running: 0,
        spawned: 0,
    }),
    work: Condvar::new(),
};

impl Slots {
    /// A Worker's life: take a body, run it to its end (its pool let it
    /// go), take the next. Never returns.
    fn run(&'static self) {
        loop {
            let body = {
                let mut st = self.state.lock().unwrap();
                loop {
                    if let Some(b) = st.queue.pop_front() {
                        st.running += 1;
                        break b;
                    }
                    st.idle += 1;
                    st = self.work.wait(st).unwrap();
                    st.idle -= 1;
                }
            };
            body();
            self.state.lock().unwrap().running -= 1;
        }
    }

    fn give(&'static self, body: ThreadBody) {
        let mut st = self.state.lock().unwrap();
        st.queue.push_back(body);
        let starting = st.spawned - st.idle - st.running;
        if st.idle + starting >= st.queue.len() {
            // A waiting Worker takes it, or one that is still starting.
            drop(st);
            self.work.notify_one();
            return;
        }
        st.spawned += 1;
        drop(st);
        if spawn_worker(Box::new(|| SLOTS.run())).is_err() {
            // The body stays queued for the next Worker that is free; until
            // then its pool decodes with the workers it has.
            self.state.lock().unwrap().spawned -= 1;
            console_error(
                "wasm-h26x: could not start a decoder thread (no __wasmH26xSpawnThread?)",
            );
        }
    }
}

/// [`Platform::spawn`]: a pool worker for rivet-h26x.
pub fn spawn_body(body: ThreadBody) {
    SLOTS.give(body);
}

/// [`Platform::now_ms`].
pub fn now_ms() -> f64 {
    performance_now()
}

/// How many Workers have been asked for so far.
pub fn workers_spawned() -> usize {
    SLOTS.state.lock().unwrap().spawned
}

/// Make rivet-h26x start its worker threads here (once per instance).
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        h26x::platform::install(Platform {
            spawn: spawn_body,
            now_ms,
        });
    });
}
