/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024 Daniel Balsom

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of this software and associated documentation files (the “Software”),
    to deal in the Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, sublicense,
    and/or sell copies of the Software, and to permit persons to whom the
    Software is furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
    FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.

    --------------------------------------------------------------------------

    examples/imgviz/src/args.rs

    Argument parsers for imgviz.
*/

use std::path::PathBuf;

use bpaf::{construct, long, short, OptionParser, Parser};
use fluxfox::{tiny_skia::Color, StandardFormat};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct Out {
    pub(crate) debug: bool,
    pub(crate) in_disk: Option<PathBuf>,
    pub(crate) in_image0: PathBuf,
    pub(crate) in_image1: Option<PathBuf>,
    pub(crate) out_disk: PathBuf,
    pub(crate) hole_ratio: Option<f32>,
    pub(crate) angle: f32,
    pub(crate) cc: bool,
    pub(crate) skip: u16,
    pub(crate) disk_format: StandardFormat,
    pub(crate) formatted: bool,
    pub(crate) sectors_only: bool,
    pub(crate) applesauce: bool,
    pub(crate) grayscale: bool,
}

/// Set up bpaf argument parsing.
pub(crate) fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let in_disk = long("in_disk")
        .help("Filename of disk image to read")
        .argument::<PathBuf>("INPUT_DISK_IMAGE")
        .optional();

    let in_image0 = long("in_image0")
        .help("Filename of PNG image to use for head 0")
        .argument::<PathBuf>("INPUT_PNG_IMAGE0");

    let in_image1 = long("in_image1")
        .help("Filename of PNG image to use for head 1")
        .argument::<PathBuf>("INPUT_PNG_IMAGE1")
        .optional();

    let out_disk = short('o')
        .long("out_disk")
        .help("Filename of disk image to write")
        .argument::<PathBuf>("OUTPUT_DISK_IMAGE");

    let skip = long("skip")
        .help("Number of tracks to skip. Default is 0")
        .argument::<u16>("SKIP")
        .fallback(0);

    let hole_ratio = short('h')
        .long("hole_ratio")
        .help("Ratio of inner radius to outer radius")
        .argument::<f32>("RATIO")
        .optional();

    let angle = short('a')
        .long("angle")
        .help("Angle of rotation")
        .argument::<f32>("ANGLE")
        .fallback(0.0);

    let cc = long("cc").help("Wrap data counter-clockwise").switch();

    let disk_format = standard_format_parser();

    let formatted = long("formatted")
        .help("If no input disk image was specified, create a formatted image")
        .switch();

    let sectors_only = long("sectors_only").help("Only render image into sector data").switch();
    let applesauce = long("applesauce")
        .help("Use presets for Applesauce disk visualization (default HxC/fluxfox)")
        .switch();

    let grayscale = long("grayscale").help("Render grayscale image (slower)").switch();

    construct!(Out {
        debug,
        in_disk,
        in_image0,
        in_image1,
        out_disk,
        hole_ratio,
        angle,
        cc,
        skip,
        disk_format,
        formatted,
        sectors_only,
        applesauce,
        grayscale,
    })
    .to_options()
    .descr("imgviz: generate a graphical visualization of a disk image")
}

// Implement a parser for `StandardFormat`
fn standard_format_parser() -> impl Parser<StandardFormat> {
    long("disk_format")
        .help("Specify a standard disk format (e.g., 160k, 1440k)")
        .argument::<String>("STANDARD_DISK_FORMAT")
        .parse(|input| input.parse())
        .fallback(StandardFormat::PcFloppy1200)
}

/// Parse a color from either a hex string (`#RRGGBBAA` or `#RRGGBB`) or an RGBA string (`R,G,B,A`).
#[allow(dead_code)]
pub(crate) fn parse_color(input: &str) -> Result<Color, String> {
    if input.starts_with('#') {
        // Parse hex color: #RRGGBBAA or #RRGGBB
        let hex = &input[1..];
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "Invalid hex color")?;
                let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| "Invalid hex color")?;
                let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "Invalid hex color")?;
                Ok(Color::from_rgba8(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "Invalid hex color")?;
                let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| "Invalid hex color")?;
                let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "Invalid hex color")?;
                let a = u8::from_str_radix(&hex[6..8], 16).map_err(|_| "Invalid hex color")?;
                Ok(Color::from_rgba8(r, g, b, a))
            }
            _ => Err("Hex color must be in the format #RRGGBB or #RRGGBBAA".to_string()),
        }
    }
    else {
        // Parse RGBA color: R,G,B,A
        let parts: Vec<&str> = input.split(',').collect();
        if parts.len() != 4 {
            return Err("RGBA color must be in the format R,G,B,A".to_string());
        }
        let r = parts[0].parse::<u8>().map_err(|_| "Invalid RGBA color component")?;
        let g = parts[1].parse::<u8>().map_err(|_| "Invalid RGBA color component")?;
        let b = parts[2].parse::<u8>().map_err(|_| "Invalid RGBA color component")?;
        let a = parts[3].parse::<u8>().map_err(|_| "Invalid RGBA color component")?;
        Ok(Color::from_rgba8(r, g, b, a))
    }
}
