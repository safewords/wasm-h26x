//! # wasm-h26x
//!
//! H.264 and H.265 decoding for the browser, in Rust: [rivet-h26x] (rivet's
//! own H.264/AVC and H.265/HEVC decoders, written from the ITU-T
//! Recommendations) compiled to WebAssembly, behind the small streaming API
//! of [wasm-av1] — which is the one [GoogleChromeLabs/wasm-av1] put in front
//! of libaom, ported to Rust.
//!
//! The crate builds four ways:
//!
//! * **native** (`cargo build`, `cargo test`): the same decoder as a Rust
//!   library, used by the CLI harness in `examples/` and the tests, which
//!   compare every frame against rivet-h26x's own native decoder.
//! * **wasm32 baseline**: `cargo build --target wasm32-unknown-unknown` — runs
//!   in every browser with post-MVP wasm (Chrome 96 / Firefox 89 / Safari 15).
//! * **wasm32 SIMD**: the same, with `-C target-feature=+simd128`; rivet-h26x
//!   has `v128` kernels for both codecs' interpolation, weighting, inverse
//!   transforms and residual add (and H.265's SAO), and the RGBA conversion
//!   has an explicit `v128` path. On ARM this is what becomes NEON.
//! * **wasm32 threads** (each of the above with `+atomics`, shared memory and
//!   `-Zbuild-std`): rivet-h26x's worker pool — frame threading, and H.265's
//!   wavefront rows, tiles and slice segments — runs as Web Workers on the
//!   shared memory (see [`threads`]); needs a cross-origin-isolated page.
//!
//! `scripts/build.sh` produces all four; `js/` picks at runtime.
//!
//! | module | upstream | job |
//! |---|---|---|
//! | [`annexb`] | `init_avx`, `blob-api.c` (IVF there) | Annex-B elementary stream over an owned buffer, cut into access units |
//! | [`units`] | — | where an access unit ends, and which pictures in it are output |
//! | [`decoder`] | `AVX_Decoder_*` | run/next_frame/finished ring around the decoder core |
//! | `core` (private) | libaom itself | rivet-h26x behind the calls `decoder` makes |
//! | [`frame`] | `buffer_frame`, `AVX_Video_Frame` | packed planes + geometry + colour |
//! | [`convert`] | `yuv-to-rgb.c` | YUV→RGBA, scalar and SIMD128 |
//! | [`wasm`] | `EXPORTED_FUNCTIONS` | the wasm-bindgen surface |
//! | [`threads`] | — | threads builds only: the core's worker threads as Web Workers |
//!
//! [rivet-h26x]: https://github.com/safewords/rivet-h26x-codecs
//! [wasm-av1]: https://github.com/safewords/wasm-av1
//! [GoogleChromeLabs/wasm-av1]: https://github.com/GoogleChromeLabs/wasm-av1

pub mod annexb;
pub mod convert;
mod core;
pub mod decoder;
pub mod frame;
pub mod units;

#[cfg(all(target_arch = "wasm32", target_feature = "atomics"))]
pub mod threads;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

/// True when this build can run the core's worker threads: native, or wasm
/// with the `atomics` target feature (see [`threads`]).
pub const THREADS_SUPPORTED: bool =
    cfg!(any(not(target_arch = "wasm32"), target_feature = "atomics"));

pub use annexb::{AnnexBError, AnnexBReader};
pub use convert::{simd_enabled, yuv_to_rgba, Coefficients};
pub use core::CoreError;
#[cfg(feature = "container")]
pub use decoder::ContainerInfo;
pub use decoder::{Config, Decoder, Error, RunOutcome, Stats};
pub use frame::{ColorInfo, Frame, FrameData, PixelLayout, PlaneInfo};
pub use units::Codec;

/// Crate version, for the JS side to report.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
