//! What the native tests share: the fixtures and their reference MD5s.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use md5::{Digest, Md5};
use wasm_h26x::{Decoder, Frame, PixelLayout};

pub fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

/// One elementary-stream fixture and what rivet-h26x's native `h26xdec`
/// decodes it to (`scripts/make-fixtures.sh`).
pub struct Fixture {
    pub name: String,
    pub data: Vec<u8>,
    /// One per output frame, in output order: `(width, height, md5)`.
    pub frames: Vec<(usize, usize, String)>,
}

impl Fixture {
    pub fn layout(&self) -> PixelLayout {
        if self.name.contains("gray") {
            PixelLayout::I400
        } else if self.name.contains("444") {
            PixelLayout::I444
        } else if self.name.contains("422") {
            PixelLayout::I422
        } else {
            PixelLayout::I420
        }
    }

    pub fn bit_depth(&self) -> u8 {
        if self.name.contains("10-") {
            10
        } else {
            8
        }
    }
}

pub fn fixture(name: &str) -> Fixture {
    let dir = testdata();
    let ext = if name.starts_with("hevc") {
        "265"
    } else {
        "264"
    };
    let data = std::fs::read(dir.join(format!("{name}.{ext}"))).unwrap();
    let md5 = std::fs::read_to_string(dir.join(format!("{name}.md5"))).unwrap();
    let frames = md5
        .lines()
        .filter(|l| l.contains(','))
        .map(|l| {
            // index,poc,decode_index,WxH,md5
            let f: Vec<&str> = l.split(',').collect();
            let (w, h) = f[3].split_once('x').unwrap();
            (w.parse().unwrap(), h.parse().unwrap(), f[4].to_string())
        })
        .collect();
    Fixture {
        name: name.to_string(),
        data,
        frames,
    }
}

/// Every elementary-stream fixture, by name.
pub fn fixtures() -> Vec<Fixture> {
    let mut names: Vec<String> = std::fs::read_dir(testdata())
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().into_string().unwrap();
            n.strip_suffix(".md5").map(str::to_string)
        })
        .collect();
    names.sort();
    names.iter().map(|n| fixture(n)).collect()
}

pub fn md5_hex(data: &[u8]) -> String {
    Md5::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What came out of a decode: per frame `(width, height, md5)`, and pts.
#[derive(Debug, Default)]
pub struct Decoded {
    pub frames: Vec<(usize, usize, String)>,
    pub pts: Vec<Option<i64>>,
    pub first: Option<Frame>,
}

impl Decoded {
    pub fn take(&mut self, f: &Frame) {
        if self.first.is_none() {
            self.first = Some(f.clone());
        }
        self.frames.push((f.width, f.height, md5_hex(&f.data)));
        self.pts.push(f.pts);
    }
}

/// Run a decoder whose source is set until it is finished.
pub fn drain(dec: &mut Decoder) -> Decoded {
    let mut out = Decoded::default();
    let mut guard = 0;
    while !dec.finished() {
        if let Err(e) = dec.run() {
            panic!("decode error: {e}");
        }
        while let Some(f) = dec.next_frame() {
            out.take(f);
        }
        guard += 1;
        assert!(guard < 100_000, "decoder never finished");
    }
    out
}
