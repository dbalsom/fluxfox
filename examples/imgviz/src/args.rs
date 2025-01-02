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

use std::path::{Path, PathBuf};

use bpaf::{construct, long, short, OptionParser, Parser};
use tiny_skia::Color;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct Out {
    pub(crate) debug: bool,
    pub(crate) in_filename: PathBuf,
    pub(crate) out_filename: PathBuf,
    pub(crate) resolution: u32,
    pub(crate) side: Option<u8>,
    pub(crate) sides: Option<u8>,
    pub(crate) hole_ratio: f32,
    pub(crate) angle: f32,
    pub(crate) data: bool,
    pub(crate) weak: bool,
    pub(crate) errors: bool,
    pub(crate) metadata: bool,
    pub(crate) index_hole: bool,
    pub(crate) decode: bool,
    pub(crate) cc: bool,
    pub(crate) dont_reverse: bool,
    pub(crate) supersample: u32,
    pub(crate) img_bg_color: Option<Color>,
    pub(crate) track_bg_color: Option<Color>,
    pub(crate) title: Option<String>,
}

/// Set up bpaf argument parsing.
pub(crate) fn opts() -> OptionParser<Out> {
    let debug = short('d').long("debug").help("Print debug messages").switch();

    let in_filename = short('i')
        .long("in_filename")
        .help("Filename of disk image to read")
        .argument::<PathBuf>("IN_FILE");

    let out_filename = short('o')
        .long("out_filename")
        .help("Filename of image to write")
        .argument::<PathBuf>("OUT_FILE");

    let resolution = short('r')
        .long("resolution")
        .help("Size of resulting image, in pixels")
        .argument::<u32>("SIZE");

    let side = short('s')
        .long("side")
        .help("Side to render. Omit to render both sides")
        .argument::<u8>("SIDE")
        .guard(|side| *side < 2, "Side must be 0 or 1")
        .optional();

    let sides = long("sides")
        .help("Override number of sides to render. Only useful for rendering single-sided disks as double-wide images.")
        .argument::<u8>("SIDES")
        .guard(|sides| *sides > 0 && *sides < 3, "Sides must be 1 or 2")
        .optional();

    let hole_ratio = short('h')
        .long("hole_ratio")
        .help("Ratio of inner radius to outer radius")
        .argument::<f32>("RATIO")
        .fallback(0.33);

    let angle = short('a')
        .long("angle")
        .help("Angle of rotation")
        .argument::<f32>("ANGLE")
        .fallback(0.0);

    let data = long("data").help("Render data").switch();

    let weak = short('w').long("weak").help("Render weak bits").switch();

    let errors = short('e').long("errors").help("Render bitstream errors").switch();

    let metadata = long("metadata").help("Render metadata").switch();

    let decode = long("decode").help("Decode data").switch();

    let index_hole = long("index_hole").help("Render index hole").switch();

    let supersample = long("ss")
        .help("Supersample (2,4,8)")
        .argument::<u32>("FACTOR")
        .fallback(1);

    let cc = long("cc").help("Wrap data counter-clockwise").switch();

    let dont_reverse = long("dont_reverse").help("Don't reverse direction of side 1").switch();

    let img_bg_color = long("img_bg_color")
        .help("Specify the image background color as #RRGGBBAA, #RRGGBB, or R,G,B,A")
        .argument::<String>("IMAGE_BACKGROUND_COLOR")
        .parse(|input: String| parse_color(&input))
        .optional();

    let track_bg_color = long("track_bg_color")
        .help("Specify the track background color as #RRGGBBAA, #RRGGBB, or R,G,B,A")
        .argument::<String>("TRACK_BACKGROUND_COLOR")
        .parse(|input: String| parse_color(&input))
        .optional();

    // Title argument with substitution
    let title = long("title")
        .help("Specify the title string, or ${IN_FILE} to use the input filename.")
        .argument::<String>("TITLE")
        .optional();

    construct!(Out {
        debug,
        in_filename,
        out_filename,
        resolution,
        side,
        sides,
        hole_ratio,
        angle,
        data,
        weak,
        errors,
        metadata,
        index_hole,
        decode,
        cc,
        dont_reverse,
        supersample,
        img_bg_color,
        track_bg_color,
        title,
    })
    .to_options()
    .descr("imgviz: generate a graphical visualization of a disk image")
}

/// Perform `${IN_FILE}` substitution for the `title`, using only the filename portion of `in_filename`.
pub(crate) fn substitute_title(title: Option<String>, in_filename: &Path) -> Option<String> {
    // Extract only the filename portion for substitution
    let in_filename_str = in_filename
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let in_dir_str = in_filename
        .parent() // Get the parent directory
        .and_then(|parent| parent.file_name()) // Get the last component of the parent
        .and_then(|name| name.to_str()) // Convert OsStr to &str
        .map(|name| name.to_string()); // Convert &str to String

    // Substitute `${IN_FILE}` in `title` if provided; otherwise, return `None`
    let title = title.map(|t| t.replace("${IN_FILE}", &in_filename_str));
    // Substitute `${IN_DIR}` in `title`
    title.map(|t| t.replace("${IN_DIR}", &in_dir_str.unwrap_or_default()))
}

/// Parse a color from either a hex string (`#RRGGBBAA` or `#RRGGBB`) or an RGBA string (`R,G,B,A`).
pub(crate) fn parse_color(input: &str) -> Result<Color, String> {
    if input.starts_with('#') {
        // Parse hex color: #RRGGBBAA or #RRGGBB
        let hex = input.strip_prefix('#').ok_or("Invalid hex color")?;
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
