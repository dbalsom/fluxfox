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
*/

//! Fluxfox maintains a 'source map' for supported file formats it parses.
//! This is a simple hash tree with a basic schema that allows for reporting
//! of the internal fields and parameters of the source image format.
//!
//! The source map can also be used to help reconstruct an identical image
//! of the same format as the source, if there would otherwise be ambiguity.
//!
//! A source map is not automatically generated. It must be requested in
//! ParserReadOptions.

use std::fmt::{self, Display, Formatter};

// Representation of how to display numeric values
#[derive(Debug, Clone, Copy)]
pub enum Representation {
    Decimal,
    Hexadecimal,
    Binary,
}

impl Representation {
    pub fn format(&self, value: u32) -> String {
        match self {
            Representation::Decimal => format!("{}", value),
            Representation::Hexadecimal => format!("0x{:X}", value),
            Representation::Binary => format!("0b{:b}", value),
        }
    }
}

/// Support a few basic types of source data.
/// The intent is not to support every possible kind of data field -
/// shove something in a string if necessary.
#[derive(Debug, Clone)]
pub enum SourceData {
    U32(u32, Representation),
    U16(u16, Representation),
    U8(u8, Representation),
    String(String),
}

impl Display for SourceData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SourceData::U32(value, repr) => write!(f, "{}", repr.format(*value)),
            SourceData::U16(value, repr) => write!(f, "{}", repr.format(*value as u32)),
            SourceData::U8(value, repr) => write!(f, "{}", repr.format(*value as u32)),
            SourceData::String(s) => write!(f, "{}", s),
        }
    }
}

// The SourceMapNode structure
#[derive(Debug, Clone)]
pub struct SourceMapNode {
    pub name: String,                 // Node name
    pub data: Option<SourceData>,     // Optional value
    pub valid: bool,                  // Whether the parser thinks this field is valid
    pub children: Vec<SourceMapNode>, // Child nodes
}

impl SourceMapNode {
    pub fn new(name: &str, data: Option<SourceData>, valid: bool) -> Self {
        Self {
            name: name.to_string(),
            data,
            valid,
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, child: SourceMapNode) {
        self.children.push(child);
    }
}
