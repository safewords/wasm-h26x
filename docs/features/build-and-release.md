# Build, tests and release

> **Status:** PARTIAL — the build, the tests and both workflows are in place, CI passed on `4c72771`, and `pkg/` holds CI's build of it; v0.1.0 is tagged from there (npm publishing waits on credentials: see release.yml), the committed `pkg/` is replaced by hand so it can trail `main` by a commit, and `scripts/build.sh` does not run on a stock macOS.
> **Owners (code):** `scripts/build.sh`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `package.json`, `Cargo.toml`, `rust-toolchain.toml`, `tests/`, `test/`, `scripts/`
> **Related docs:** [variants-and-threading.md](./variants-and-threading.md), [`../performance.md`](../performance.md), [`../rivet.md`](../rivet.md)
> **Last verified against code:** 2026-11-15 @ `bb70073`

## Overview

`scripts/build.sh` compiles the four variants and runs wasm-bindgen and
wasm-opt into `pkg/`; between the two, the threads variants go through
`tools/wasm-copy-guard` (a native tool with its own `Cargo.lock`, built by
the script), which works around a WebKit bug
([variants-and-threading.md](./variants-and-threading.md)). CI proves the
decoder on every push to `main`; a `v*` tag builds the tag, tests it, and
publishes `@safewords/wasm-h26x` to npm and to a GitHub Release. `pkg/` is
committed so that a git dependency carries the wasm; it is CI's build,
committed by hand. All of it is wasm-av1's, renamed (`616be49`). Nothing
here deploys a service, and as of `bb70073` there is no tag, no release
and no consumer.

```mermaid
flowchart LR
  push[push to main] --> ci[ci.yml: fmt, clippy, cargo test,<br/>wasm tests, build.sh, node tests,<br/>npm publish --dry-run]
  tag[tag v*] --> rel[release.yml: build.sh, tests]
  rel --> gh[GitHub Release<br/>.tgz, pkg .zip, SHA256SUMS]
  rel --> npm[npm @safewords/wasm-h26x<br/>provenance]
  ci -. artifact pkg/, committed by hand .-> pkgdir[pkg/ in main]
  rivet[(rivet main<br/>rivet-h26x, rivet-container)] -. Cargo.lock .-> ci
```

## How it works today

### Interfaces

| Kind | Name | Notes |
|---|---|---|
| Script | `scripts/build.sh [--dev]` | `VARIANTS="simd simd-threads"` builds a subset. `FEATURES` unset builds the default features (`container`); set, even to nothing, it builds exactly the features it names, so `FEATURES= scripts/build.sh` leaves the demuxers out (`scripts/build.sh:36-46`). Checks wasm-bindgen-cli equals the `Cargo.lock` version (`50-56`); writes `pkg/manifest.json` with the version, `"core": "rivet-h26x"`, the rivet-h26x and rivet-container revisions `Cargo.lock` pins, the features and each variant's size (`131-148`) |
| npm | `build`, `test`, `test:node`, `test:browser`, `bench`, `bench:browser`, `serve` | `package.json:56-64` |
| Workflow | `ci.yml` | `push` to `main`, `workflow_dispatch` (`.github/workflows/ci.yml:12-19`); `docs/**` and `**/*.md` are paths-ignored, so a docs-only push runs nothing |
| Workflow | `release.yml` | `push` of `v*` tags, or `workflow_dispatch` with a tag (`.github/workflows/release.yml:25-32`) |
| Secret | `NPM_TOKEN` (repository secret, optional) | Otherwise npm trusted publishing (OIDC); with neither the npm step warns and the release still succeeds (`release.yml:127-151`) |
| Hook | `.githooks/pre-commit` | `cargo fmt --check` and `cargo clippy -D warnings`, when `core.hooksPath` is `.githooks` |

Toolchain: Rust `1.100.0` with `rust-src` (`rust-toolchain.toml`),
wasm-bindgen-cli `0.2.130`, binaryen `version_123`, Node 22, mold for
native links (`ci.yml:21-50`). The threads variants rebuild std with
`-Zbuild-std` under `RUSTC_BOOTSTRAP=1` (`scripts/build.sh:68-75`).

Dependencies: `h26x` and `container` are both taken from rivet `main`
(`Cargo.toml:24`, `31`) at the revision `Cargo.lock` pins (`f5d350e`,
which carries rivet-h26x `3ec8b66` with the `h26x::platform` hook the
threads builds need); `cargo update -p rivet-h26x -p rivet-container`
moves both. Until `a4c193c` a `[patch]` took rivet-h26x from its
`wasm/threads` branch, which has since been merged.

### Flow: tests

| Command | Proves |
|---|---|
| `cargo test --release` (also `--no-default-features`, without `container`) | Every elementary-stream fixture decodes frame by frame to the MD5s of rivet-h26x's own `h26xdec`, on 1, 2 and 4 threads, with pts in presentation order; push ≡ elementary stream; flush rewinds; the ring; MP4/fMP4/MKV/TS ≡ elementary stream; CMAF segments, H.264 and H.265 (an open GOP across the boundary, its RASL pictures decoded), and an H.264 rendition switch (`tests/decode.rs`, `tests/container.rs`, and the unit tests in `src/`) |
| `cargo test --target wasm32-unknown-unknown --test wasm_convert` (± simd128) | RGBA dispatch equals scalar on 55 combinations inside wasm |
| `node --test test/decode.test.mjs` | The built `.wasm`, all four variants: every fixture's frames equal `h26xdec` in baseline and SIMD; the `threads` and `simd-threads` builds on 1, 2, 4 and 8 threads, each wasm thread a `worker_threads` Worker, with the number of Workers asked for (`test/threads-node.mjs`, one process per run so a decoder left waiting fails instead of hanging); SIMD and baseline RGBA byte-identical; push mode; errors; VUI frame rate; containers and segments, H.265 segments with open GOPs among them; a rung switch through the JS `Decoder`; `sniff` and the HLS CODECS helpers |
| `node --test test/copy-guard.test.mjs` (builds `tools/wasm-copy-guard` with cargo) | The pass the threads variants get behaves as memmove/memset on both of its paths |
| `node test/browser.mjs` (Playwright, not in CI) | Chromium, Firefox, WebKit × variants × renderers × main thread/Worker × formats, HLS, rung switches and thread counts, a two-layer H.265 stream, on a cross-origin-isolated page: 33 combinations |

Run on 2026-11-15 on macOS aarch64: `cargo test --release` passes (17 unit,
7 decode and 3 container tests); Node 25/25 against a local build of all
four variants, and 25/25 against CI's build committed in `bfd7846`;
`test/browser.mjs` 32/32 in each of Chromium, Firefox and WebKit
(Playwright 1.62). CI (run 43753157735) passed both jobs on `2e79fa0`.
`30688b9` then added `testdata/cmaf-hevc` and its two tests (the Rust
`cmaf_segments_decode_continuously` and a Node test): ffmpeg's HLS muxer
writes one composition time twice at that fixture's open-GOP boundary, so
there the pts are checked only for order (`tests/container.rs:124-133`).

Run again on 2026-11-15 on macOS aarch64 at `42acb04` (rivet `f5d350e`):
`cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`;
`cargo test --release` 17 unit + 7 decode + 3 container,
`--no-default-features` 17 + 7; `wasm_convert` in wasm 2/2, baseline and
simd128; `scripts/build.sh` (all four variants, wasm-bindgen 0.2.130,
binaryen `version_123`); Node 26/26; `test/browser.mjs` 33/33 in each of
Chromium, Firefox and WebKit (Playwright 1.62.1). `build.sh` ran as
`zsh --emulate ksh scripts/build.sh` with a `sed` shim that rewrites `p}`
to `p;}` for `/usr/bin/sed` and GNU `stat` (`gstat`) as `stat` first on
`PATH`.

`scripts/make-fixtures.sh` regenerates `testdata/` with ffmpeg (libx264,
libx265) and takes each fixture's per-frame MD5 from rivet-h26x's
`h26xdec` example, built at the revision `Cargo.lock` pins (`H26XDEC`);
`scripts/layered-fixture.mjs` makes the two-layer H.265 fixture from the
Main 10 one, whose MD5s it must equal. On 2026-11-15 h26xdec built at
rivet-h26x `3ec8b66` gave every committed fixture's `.md5` byte for byte.
`scripts/fetch-samples.sh` downloads Big Buck Bunny clips into
`testdata/samples/` (not committed) for the demo and the benchmarks.

### Flow: release

`release.yml` checks the tag equals the `package.json` and `Cargo.toml`
versions (`55-63`), builds, runs `cargo test` and the Node tests, `npm
pack`s, zips `pkg/`, writes `SHA256SUMS` and release notes with the
rivet-h26x and rivet revisions, creates or updates the GitHub Release, and
publishes to npm unless that version already exists. To release: bump
both versions (0.1.0 today), tag `vX.Y.Z`, push the tag. The first npm
publish needs `NPM_TOKEN` or a publish by hand, since trusted publishing
needs the package to exist (`release.yml:13-19`).

### Edge cases & gotchas

- **The committed `pkg/` can trail `main` by a commit.** As in wasm-av1,
  it is replaced only from CI's build (the pinned `wasm-opt`), never from a
  local one, by hand, after the source commit that changes the wasm. It is
  CI's build of `4c72771` (run 43795375571, `wasm-opt -O3`, rivet
  `f5d350e`, rivet-h26x `3ec8b66`), committed as `3b0ab49`. Before it,
  `ff68fe8` held CI's build of `66366a2` (run 43793316495), and
  `bfd7846` held CI's build of `2e79fa0` (run 43753157735, rivet-h26x
  `a26393e`, rivet `88a4940`). Between a source commit and
  the `pkg/` commit that follows it, consumers of the release tarball are
  unaffected (release.yml builds the tag itself) and consumers of
  `github:safewords/wasm-h26x#main` get the previous wasm with newer JS.
  CI's "does the committed `pkg/` match this build?" step is a warning
  only (`ci.yml:98-103`): builds on different machines differ in the
  paths embedded in panic strings.
- **Sizes** of that build (`pkg/manifest.json`; gzip is CI's `gzip -9`):
  baseline 2 481 784 bytes (796 595 gzipped), simd 2 783 035 (892 567),
  threads 2 502 061 (805 370), simd-threads 2 805 460 (902 058). The
  `2e79fa0` build was 2 395 577 (766 444), 2 692 513 (857 794),
  2 413 460 (775 485), 2 713 008 (872 018): +86 to +92 KB per variant,
  +30 KB gzipped, from rivet-h26x's decoder (see below).
- **rivet-h26x's encoders are in the crate but not in the `.wasm`.**
  Nothing here calls them, so LTO drops them: on 2026-11-15 the baseline
  `.wasm` before wasm-bindgen had no function from rivet-h26x's `encode`
  module, and the 92 KB of code it gained from rivet-h26x `a26393e` to
  `3ec8b66` is the decoder (`hevc::ctu` +34 KB, `h264::decoder` +12 KB,
  `hevc::decoder` +11 KB, the SPS/PPS/slice parsers +12 KB, `hevc::scc`
  +1 KB, and the rest in `core`/`alloc` generics). Check the same way after
  a bump: the release profile keeps function names until wasm-bindgen
  strips them.
- `scripts/build.sh` does not run on a stock macOS (checked 2026-11-15 on
  macOS with `/usr/bin/sed`, `/bin/bash` 3.2.57 and `/usr/bin/stat`): BSD
  sed rejects the `{n;s/…/p}` and `{n;n;s/…/p}` expressions ("bad flag in
  substitute command: '}'", `scripts/build.sh:48`, `:50`); bash 3.2 refuses
  the empty arrays under `set -u` ("unbound variable", `:74-75`); and
  `:144` (with `release.yml:112`) uses GNU `stat -c %s`. CI runs it on
  Linux, where all three are fine. wasm-av1's
  `docs/features/build-and-release.md` records how it was run on macOS
  (`zsh --emulate ksh` and two shims).
- **Without `wasm-opt` the `.wasm` is larger.** `build.sh` skips it with a
  note; compare sizes only between builds made the same way (each build
  writes its sizes to `pkg/manifest.json`).
- The threads variants pass `-C target-feature=+atomics`, which is not a
  stable target feature; on the same toolchain wasm-av1 records rustc
  warning that it "will become a hard error in a future release"
  (rust-lang/rust#162235). Read that issue before the next toolchain bump.
- `.cargo/config.toml` sets a 7-day dependency cooldown
  (`global-min-publish-age`): `cargo update` and `cargo add` skip crates.io
  versions younger than that. It does not apply to the git dependencies or
  to builds from `Cargo.lock`.
- `Cargo.toml:47-48` denies cargo's `unused_dependencies` lint, so any
  cargo command that compiles the crate fails on an unused dependency.
- The browser matrix needs Playwright (`PLAYWRIGHT_DIR`) and does not run
  in CI.
- A new Rust version cold-starts sccache: the first CI build after a bump
  is 2-4x slower.

## History & references

- `1a7e686`, `55e3c2b`, `616be49` (2026-11-15): the crate, the JS API with
  its tests, and the workflows and package as `@safewords/wasm-h26x` 0.1.0,
  each taken from wasm-av1 and adapted.
- `bfd7846` (2026-11-15): `pkg/` committed, CI's build of `2e79fa0`.
- `ff68fe8` (2026-11-15): `pkg/` replaced with CI's build of `66366a2`
  (run 43793316495); Node 26/26 and `test/browser.mjs` 33/33 in each
  engine against it, and CI on `ff68fe8` (run 43794226963) green, its
  build matching the committed `pkg/` exactly.
- `3b0ab49` (2026-11-15): `pkg/` replaced with CI's build of `4c72771`
  (run 43795375571, unequal luma and chroma depths widened); Node 26/26
  against it. `test/browser.mjs` was not re-run: the change touches only
  streams with unequal depths, which no browser combination plays.
- `30688b9` (2026-11-15): `testdata/cmaf-hevc`, H.265 CMAF with open GOPs.
- `a4c193c` (2026-11-15): the `[patch]` deleted; rivet-h26x and
  rivet-container from rivet `main` `f5d350e` (rivet-h26x `3ec8b66`).
- `42acb04` (2026-11-15): `scripts/layered-fixture.mjs` and
  `testdata/hevc-layered-main10-178x100`.
- wasm-av1's `docs/features/build-and-release.md`: the history of the
  toolchain, the cooldown and the copy guard.

## Proposed / Ideas (backlog)

| Idea | Motivation | Sketch | Status |
|---|---|---|---|
| Rebuild `pkg/` with each source change | `main` consumers get what the source says | CI artifact committed by the release prep, or stop committing `pkg/` | Proposed |
| Portable `build.sh` | Builds on macOS | `wc -c < file` for `stat`; `p;}` in the sed expressions; `${arr[@]+"${arr[@]}"}` for the empty arrays | Proposed |
| Browser matrix in CI | A browser regression shows up without someone running it | Playwright with its browsers in the `wasm` job | Proposed |
