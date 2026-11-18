//! Native harness — upstream `test.c`, grown up (wasm-av1's `decode_ivf`).
//!
//! ```text
//! cargo run --release --example decode -- FILE.264|.265          # decode, print a line per frame + summary
//! cargo run --release --example decode -- FILE.mp4|.mkv|.ts      # a container, demuxed by rivet
//! cargo run --release --example decode -- FILE --codec hevc      # say the codec of an elementary stream
//! cargo run --release --example decode -- FILE --threads 4       # decoder threads
//! cargo run --release --example decode -- FILE --ppm 30          # write frame0030.ppm (the RGBA conversion, as RGB)
//! cargo run --release --example decode -- FILE --quiet           # summary only (timing runs)
//! ```
//!
//! A frame line is `index,pts,WxH,md5` with the MD5 of the packed planes —
//! the same bytes rivet-h26x's `h26xdec` hashes, which is how
//! `testdata/*.md5` were made, so the two can be compared with `cut`.

use std::fs;
use std::io::Write;
use std::time::Instant;

use md5::{Digest, Md5};
use wasm_h26x::{convert, Codec, Config, Decoder};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: decode FILE [--codec h264|hevc] [--threads N] [--ppm N] [--quiet]");
        std::process::exit(2);
    };
    let mut codec = None;
    let mut ppm: Option<u64> = None;
    let mut quiet = false;
    let mut config = Config::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--codec" => {
                codec = Some(Codec::from_label(&args[i + 1]).expect("h264 or hevc"));
                i += 1;
            }
            "--threads" => {
                config.threads = args[i + 1].parse().unwrap();
                i += 1;
            }
            "--ppm" => {
                ppm = Some(args[i + 1].parse().unwrap());
                i += 1;
            }
            "--quiet" => quiet = true,
            other => panic!("unknown argument {other}"),
        }
        i += 1;
    }

    let data = fs::read(path).unwrap();
    let mut dec = Decoder::new(config).unwrap();
    let is_annexb = data.starts_with(&[0, 0, 1]) || data.starts_with(&[0, 0, 0, 1]);
    if is_annexb {
        dec.set_source_annexb(data, codec).unwrap();
    } else {
        #[cfg(feature = "container")]
        dec.set_source_container(data).unwrap();
        #[cfg(not(feature = "container"))]
        panic!("not an elementary stream, and this build has no container support");
    }

    let t0 = Instant::now();
    let mut n = 0u64;
    let mut all = Md5::new();
    let mut stdout = std::io::stdout().lock();
    while !dec.finished() {
        if let Err(e) = dec.run() {
            eprintln!("decode error: {e}");
        }
        while let Some(f) = dec.next_frame() {
            all.update(&*f.data);
            if !quiet {
                let pts = f.pts.map_or("-".into(), |p| p.to_string());
                let md5 = hex(&Md5::digest(&*f.data));
                writeln!(stdout, "{n},{pts},{}x{},{md5}", f.width, f.height).unwrap();
            }
            if ppm == Some(n) {
                let mut rgba = vec![0u8; f.rgba_len()];
                convert::yuv_to_rgba(f, &mut rgba);
                let mut out = format!("P6\n{} {}\n255\n", f.width, f.height).into_bytes();
                for px in rgba.chunks(4) {
                    out.extend_from_slice(&px[..3]);
                }
                let name = format!("frame{n:04}.ppm");
                fs::write(&name, out).unwrap();
                eprintln!("wrote {name}");
            }
            n += 1;
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    let s = dec.stats();
    eprintln!(
        "{n} frames ({}, {} thread{}), {:.1} fps, {} errors, md5 of all {}",
        dec.codec().map_or("?", Codec::name),
        wasm_h26x::decoder::effective_threads(dec.config()),
        if dec.config().threads > 1 { "s" } else { "" },
        n as f64 / secs,
        s.decode_errors,
        hex(&all.finalize())
    );
}

/// Lower-case hex: md-5 0.11's output is not `LowerHex`.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
