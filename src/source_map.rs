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

//! Define a disk image file "Source Map" - a tree structure that holds scalar
//! data values representing fields read from the source disk image. This can
//! provide  some useful debugging information, or introspection into how various
//! fields are being interpreted.
//! The ability to mark data values as suspicious, and to insert comments, adds
//! to the utility of this structure.
//!
//! Additionally, an [ImageSourceMap] could be provided to a file format parser
//! on write, to better allow a parser to create an identical disk image from
//! the parsed data.
//!
//! A source map is not created by a parser by default, to reduce memory usage.
//! You can request source map creation by setting the CREATE_SOURCE_MAP flag
//! in [ParserWriteOptions].

use crate::{
    tree_map::{FoxTree, FoxTreeCursor, FoxTreeMap},
    FoxHashSet,
};

/// An enum representing a data representation - either decimal, hexadecimal or binary.
/// The internal value represents the number of digits to display. A value of 0 means
/// no width specifier will be used when formatting.
/// The default representation is decimal.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Repr {
    Dec(u8),
    Hex(u8),
    Bin(u8),
}

impl Default for Repr {
    fn default() -> Self {
        Repr::Dec(0)
    }
}

impl Repr {
    /// Format the provided [Scalar] with the current representation.
    pub fn fmt(&self, value: &Scalar) -> String {
        let value = value.int();
        match self {
            Repr::Dec(0) => format!("{}", value),
            Repr::Dec(digits) => format!("{:0width$}", value, width = *digits as usize),

            Repr::Hex(0) => format!("{:#X}", value),
            Repr::Hex(digits) => format!("{:#0width$X}", value, width = (*digits + 2) as usize),

            Repr::Bin(0) => format!("{:#b}", value),
            Repr::Bin(digits) => format!("{:#0width$b}", value, width = (*digits + 2) as usize),
        }
    }
}

/// A scalar value, primarily defining simple integers and a String type.
/// In theory all integers could be stored as the same type, but I suppose this might save some
/// memory in some cases.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    U8(u8),
    U32(u32),
    String(String),
}

impl Scalar {
    pub fn int(&self) -> u64 {
        use Scalar::*;
        match self {
            U8(v) => *v as u64,
            U32(v) => *v as u64,
            String(_) => 0,
        }
    }
}

/// A SourceValue represents a value read from a disk image format. It has an optional Scalar
/// value - if None, then we will simply display the name of the field.
/// The `repr` field can be set to indicate how the value should be formatted.
/// The `invalid` flag can be set to indicate that the value is suspicious or invalid as determined
/// by the parser.
#[derive(Clone, Default)]
pub struct SourceValue {
    scalar:  Option<Scalar>,
    repr:    Repr,
    invalid: bool,
}

pub struct SourceMap {
    tree: FoxTreeMap<SourceValue>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self {
            tree: FoxTreeMap::new(SourceValue::default()),
        }
    }
}

impl FoxTree for SourceMap {
    type Data = SourceValue;

    fn tree_mut(&mut self) -> &mut FoxTreeMap<Self::Data> {
        &mut self.tree
    }

    fn tree(&self) -> &FoxTreeMap<Self::Data> {
        &self.tree
    }
}

fn main() {
    let mut source_map = SourceMap::new();

    source_map
        .add_child(
            source_map.root(),
            Some("Head"),
            SourceValue {
                scalar: Scalar::U8(0).into(),
                ..Default::default()
            },
        )
        .add_child(
            Some("Cylinder"),
            SourceValue {
                scalar: Scalar::U8(0).into(),
                ..Default::default()
            },
        )
        .add_sibling(
            Some("Sussy"),
            SourceValue {
                scalar:  Scalar::U32(0x1234).into(),
                repr:    Repr::Hex(6),
                invalid: true,
            },
        )
        .add_sibling(
            Some("Comment"),
            SourceValue {
                scalar: Scalar::String("/* This is a comment */".to_string()).into(),
                ..Default::default()
            },
        );

    println!("Source Map Tree:");
    source_map.debug_tree(|data: &SourceValue| {
        if let Some(scalar) = &data.scalar {
            match scalar {
                Scalar::String(s) => s.clone(),
                _ => format!(
                    "{}{}",
                    data.repr.fmt(scalar),
                    if data.invalid { " **BAD VALUE** " } else { "" }
                ),
            }
        }
        else {
            "".to_string()
        }
    });
}

/// A trait for a source map that can be optionally created by a parser.
/// We can create a null source map that does nothing, to avoid having a lot of conditional code
/// in our parsers.
pub trait OptionalSourceMap {
    fn add_child(&mut self, parent: usize, name: Option<&str>, data: SourceValue) -> FoxTreeCursor<SourceValue>;
    fn debug_tree(&self);
}

pub struct RealSourceMap {
    map: FoxTreeMap<SourceValue>,
}

impl RealSourceMap {
    pub fn new() -> Self {
        Self {
            map: FoxTreeMap::new(SourceValue::default()),
        }
    }
}

impl OptionalSourceMap for RealSourceMap {
    fn add_child(&mut self, parent: usize, name: Option<&str>, data: SourceValue) -> FoxTreeCursor<SourceValue> {
        let child_index = self.map.add_child(parent, name, data);
        FoxTreeCursor::new(&mut self.map, parent, child_index)
    }

    fn debug_tree(&self) {
        let mut visited = FoxHashSet::new();
        self.map.debug_tree(0, 0, &|data| "".to_string(), &mut visited);
    }
}

// Null implementation of SourceMap that does nothing
pub struct NullSourceMap {
    tree: FoxTreeMap<SourceValue>,
}

impl NullSourceMap {
    fn new() -> Self {
        Self {
            tree: FoxTreeMap::new(SourceValue::default()),
        }
    }
}

impl OptionalSourceMap for NullSourceMap {
    fn add_child(&mut self, _parent: usize, _name: Option<&str>, _data: SourceValue) -> FoxTreeCursor<SourceValue> {
        // Always return a cursor to the root; do nothing else
        FoxTreeCursor::new(&mut self.tree, 0, 0)
    }
    fn debug_tree(&self) {
        // No-op
    }
}
