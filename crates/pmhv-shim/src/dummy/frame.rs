//! The dummy machine's screen: a test pattern with a bar that sweeps across it, so it's
//! obvious in the browser (and in thumbnails) whether the picture is live.

use std::io;
use std::path::Path;

pub const WIDTH: u16 = 640;
pub const HEIGHT: u16 = 480;

/// Colour bars, left to right.
const BARS: [[u8; 3]; 7] = [
    [192, 192, 192],
    [192, 192, 0],
    [0, 192, 192],
    [0, 192, 0],
    [192, 0, 192],
    [192, 0, 0],
    [0, 0, 192],
];
const SWEEP: [u8; 3] = [255, 255, 255];
const SWEEP_WIDTH: usize = 12;

/// Packed RGB, row by row.
#[derive(Clone)]
pub struct Frame {
    pub pixels: Vec<u8>,
}

impl Frame {
    /// The pattern at animation step `step`; `paused` dims it.
    pub fn render(step: u64, paused: bool) -> Self {
        let width = usize::from(WIDTH);
        let height = usize::from(HEIGHT);
        let sweep = usize::try_from(step).unwrap_or(0) * 8 % width;
        let mut pixels = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let colour = if x.abs_diff(sweep) < SWEEP_WIDTH / 2 {
                    SWEEP
                } else if y > height * 3 / 4 {
                    // A grey ramp along the bottom.
                    let level = u8::try_from(x * 255 / width).unwrap_or(u8::MAX);
                    [level, level, level]
                } else {
                    BARS[x * BARS.len() / width]
                };
                let colour = if paused {
                    colour.map(|channel| channel / 3)
                } else {
                    colour
                };
                pixels.extend_from_slice(&colour);
            }
        }
        Self { pixels }
    }

    pub fn write_png(&self, path: &Path) -> io::Result<()> {
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut encoder = png::Encoder::new(file, u32::from(WIDTH), u32::from(HEIGHT));
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(io::Error::other)?;
        writer
            .write_image_data(&self.pixels)
            .map_err(io::Error::other)?;
        writer.finish().map_err(io::Error::other)
    }
}
