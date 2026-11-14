//! A frame reduced to its luminance, the plane the readers measure on.

use crate::Frame;

/// Luminance from 0 to 255, rows packed.
#[derive(Clone)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Gray {
    /// The frame's luminance, shrunk by whole factors until it is at most
    /// `max_width` wide; returns the plane and the factor applied.
    pub fn from_frame(frame: &Frame, max_width: usize) -> (Gray, usize) {
        let (width, height) = (frame.width as usize, frame.height as usize);
        let factor = width.div_ceil(max_width.max(1)).max(1);
        let (w, h) = (width / factor, height / factor);
        let mut data = vec![0.0f32; w * h];
        let area = (factor * factor) as f32;
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0.0f32;
                for dy in 0..factor {
                    let row = (y * factor + dy) * width;
                    for dx in 0..factor {
                        let i = (row + x * factor + dx) * 4;
                        if let Some(p) = frame.bgra.get(i..i + 3) {
                            // Rec. 601 weights, on BGRA.
                            sum += 0.114 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.299 * f32::from(p[2]);
                        }
                    }
                }
                data[y * w + x] = sum / area;
            }
        }
        (Gray { width: w, height: h, data }, factor)
    }

    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }

    /// The luminance at a continuous point, the centre of pixel (x, y) being
    /// at (x + ½, y + ½), interpolated between the four nearest pixels.
    pub fn sample(&self, x: f32, y: f32) -> Option<f32> {
        let (fx, fy) = (x - 0.5, y - 0.5);
        if fx < 0.0 || fy < 0.0 || fx >= (self.width - 1) as f32 || fy >= (self.height - 1) as f32 {
            return None;
        }
        let (x0, y0) = (fx as usize, fy as usize);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let top = self.at(x0, y0) * (1.0 - tx) + self.at(x0 + 1, y0) * tx;
        let bottom = self.at(x0, y0 + 1) * (1.0 - tx) + self.at(x0 + 1, y0 + 1) * tx;
        Some(top * (1.0 - ty) + bottom * ty)
    }
}
