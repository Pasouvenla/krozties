//! The local debug recording: frames written as PNG files into a folder the
//! player chose, each with a JSON sidecar about its window. Off unless the
//! player turns it on, and nothing leaves the machine.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Frame, GameWindow};

/// The size of the grey thumbnail two frames are compared on.
const THUMB_WIDTH: usize = 64;
const THUMB_HEIGHT: usize = 36;
/// Below this mean grey difference, out of 255, two frames are the same.
const SAME: u32 = 2;

pub struct Recorder {
    dir: PathBuf,
    last: Mutex<HashMap<u32, Vec<u8>>>,
}

impl Recorder {
    pub fn new(dir: impl Into<PathBuf>) -> io::Result<Recorder> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Recorder { dir, last: Mutex::new(HashMap::new()) })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes `frame` and its sidecar under `<dir>/<window id>/`, unless it
    /// looks like the last frame written for that window. Returns whether it
    /// wrote.
    pub fn record(&self, window: &GameWindow, frame: &Frame) -> io::Result<bool> {
        let thumb = thumbnail(frame);
        {
            let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
            if last.get(&frame.window).is_some_and(|t| mean_difference(t, &thumb) < SAME) {
                return Ok(false);
            }
            last.insert(frame.window, thumb);
        }
        let folder = self.dir.join(frame.window.to_string());
        fs::create_dir_all(&folder)?;
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        write_png(&folder.join(format!("{millis}.png")), frame)?;
        let sidecar = serde_json::json!({
            "window": window,
            "width": frame.width,
            "height": frame.height,
            "captured_at_ms": millis,
        });
        fs::write(folder.join(format!("{millis}.json")), serde_json::to_string_pretty(&sidecar)?)?;
        Ok(true)
    }
}

/// The frame shrunk to a grey thumbnail, each cell the mean of its pixels.
fn thumbnail(frame: &Frame) -> Vec<u8> {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let mut sums = vec![0u64; THUMB_WIDTH * THUMB_HEIGHT];
    let mut counts = vec![0u64; THUMB_WIDTH * THUMB_HEIGHT];
    for y in 0..height {
        let row = y * THUMB_HEIGHT / height.max(1);
        for x in 0..width {
            let i = (y * width + x) * 4;
            let Some(p) = frame.bgra.get(i..i + 3) else {
                continue;
            };
            let grey = (u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2])) / 3;
            let cell = row * THUMB_WIDTH + x * THUMB_WIDTH / width.max(1);
            sums[cell] += grey;
            counts[cell] += 1;
        }
    }
    sums.iter().zip(&counts).map(|(s, c)| (s / (*c).max(1)) as u8).collect()
}

fn mean_difference(a: &[u8], b: &[u8]) -> u32 {
    let total: u32 = a.iter().zip(b).map(|(x, y)| u32::from(x.abs_diff(*y))).sum();
    total / u32::try_from(a.len().max(1)).unwrap_or(u32::MAX)
}

fn write_png(path: &Path, frame: &Frame) -> io::Result<()> {
    let file = io::BufWriter::new(fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(io::Error::other)?;
    let rgba: Vec<u8> = frame.bgra.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
    writer.write_image_data(&rgba).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rect;

    fn window() -> GameWindow {
        GameWindow {
            id: 7,
            pid: 1,
            title: "Dofus".into(),
            frame: Rect { x: 0.0, y: 0.0, width: 320.0, height: 180.0 },
        }
    }

    /// A 320×180 frame of one colour, with a lighter square at `square`.
    fn frame(square: Option<(usize, usize)>) -> Frame {
        let (width, height) = (320usize, 180usize);
        let mut bgra = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let lit = square.is_some_and(|(sx, sy)| (sx..sx + 40).contains(&x) && (sy..sy + 40).contains(&y));
                let v = if lit { 240 } else { 30 };
                bgra[(y * width + x) * 4..][..4].copy_from_slice(&[v, v / 2, 10, 255]);
            }
        }
        Frame { window: 7, width: width as u32, height: height as u32, bgra }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("krozties-vision-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// The same image twice is written once; a square that moves is written
    /// again. The file decodes back to the captured pixels, in RGBA.
    #[test]
    fn a_frame_is_written_only_when_it_changes() {
        let dir = scratch("record");
        let recorder = Recorder::new(&dir).unwrap();
        assert!(recorder.record(&window(), &frame(Some((10, 10)))).unwrap());
        assert!(!recorder.record(&window(), &frame(Some((10, 10)))).unwrap());
        assert!(recorder.record(&window(), &frame(Some((200, 100)))).unwrap());

        let mut pngs: Vec<PathBuf> = fs::read_dir(dir.join("7"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "png"))
            .collect();
        pngs.sort();
        assert_eq!(pngs.len(), 2);
        let decoder = png::Decoder::new(fs::File::open(&pngs[0]).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (320, 180));
        let original = frame(Some((10, 10)));
        assert_eq!(&buf[..4], &[original.bgra[2], original.bgra[1], original.bgra[0], 255]);
        let sidecar: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(pngs[0].with_extension("json")).unwrap()).unwrap();
        assert_eq!(sidecar["window"]["id"], 7);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Two windows keep separate histories: the same image is new for each.
    #[test]
    fn each_window_has_its_own_history() {
        let dir = scratch("windows");
        let recorder = Recorder::new(&dir).unwrap();
        let mut other = frame(None);
        other.window = 8;
        assert!(recorder.record(&window(), &frame(None)).unwrap());
        assert!(recorder.record(&GameWindow { id: 8, ..window() }, &other).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }
}
