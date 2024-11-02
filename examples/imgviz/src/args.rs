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
use tiny_skia::Color;

/// Perform `${IN_FILE}` substitution for the `title`, using only the filename portion of `in_filename`.
pub(crate) fn substitute_title(title: Option<String>, in_filename: &PathBuf) -> Option<String> {
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
