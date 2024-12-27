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
    tree_map::{FoxTreeCursor, FoxTreeMap},
    FoxHashSet,
};
use std::{
    any::Any,
    fmt::{Debug, Display},
};

/// An enum representing a data representation - either decimal, hexadecimal or binary.
/// The internal value represents the number of digits to display. A value of 0 means
/// no width specifier will be used when formatting.
/// The default representation is decimal.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// A state for a value, indicating whether it is good, bad, or questionable.
/// Currently only Good and Bad are used - this field was a bool, but I figured it may be useful
/// to have a third state if we aren't sure if a value is bad or not.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Default)]
pub enum ValueState {
    #[default]
    Good,
    Bad,
    Questionable,
}

/// A [SourceValue] represents a value read from a disk image format. It has an optional [Scalar]
/// value - if `None`, then we will simply display the name of the field.
///  - The `repr` field can be set to indicate how the value should be formatted via the [Repr] enum.
///  - The `invalid` flag can be set to indicate that the value is suspicious or invalid as determined
///    by the parser. This can control special highlighting in the UI.
///  - The `tip` field can be set to provide a string that will be displayed as a tooltip in the UI
///    when the user hovers over the value. A good example of using this is to provide the Debug
///    representation of the fluxfox enum mapped from the source scalar value.
///  - The `comment` field can be set to provide a string that will be displayed as a comment in
///    the UI to the right of the value.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Default)]
pub struct SourceValue {
    pub(crate) scalar: Option<Scalar>,
    pub(crate) repr: Repr,
    pub(crate) state: ValueState,
    pub(crate) tip: Option<String>,
    pub(crate) comment: Option<String>,
}

impl Display for SourceValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(scalar) = &self.scalar {
            match scalar {
                Scalar::String(s) => write!(f, "{}", s),
                _ => write!(f, "{}{}", self.repr.fmt(scalar), if self.is_bad() { "*" } else { "" }),
            }
        }
        else {
            write!(f, "")
        }
    }
}

impl SourceValue {
    // Queries and accessors

    #[inline]
    pub fn state(&self) -> ValueState {
        self.state
    }
    #[inline]
    pub fn state_mut(&mut self) -> &mut ValueState {
        &mut self.state
    }
    #[inline]
    /// Return true if the value is not in a bad state. (Questionable will also return true)
    pub fn is_good(&self) -> bool {
        !self.is_bad()
    }
    /// Return true if the value has a bad state.
    #[inline]
    pub fn is_bad(&self) -> bool {
        matches!(self.state, ValueState::Bad)
    }
    /// Return true if the value has a tool-tip string.
    #[inline]
    pub fn has_tip(&self) -> bool {
        self.tip.is_some()
    }
    /// Get a reference to the tool-tip string, if present.
    #[inline]
    pub fn tip_ref(&self) -> Option<&str> {
        self.tip.as_ref().map(|s| s.as_str())
    }
    /// Return true if the value has a comment.
    #[inline]
    pub fn has_comment(&self) -> bool {
        self.comment.is_some()
    }
    /// Return a reference to the comment string, if present.
    #[inline]
    pub fn comment_ref(&self) -> Option<&str> {
        self.comment.as_deref()
    }
    // Generators

    /// Create a u32 value with the defaults.
    #[inline]
    pub fn u32(value: u32) -> Self {
        Self::u32_base(value, Repr::default(), ValueState::Good, "")
    }
    /// Create a u32 with hexadecimal representation.
    #[inline]
    pub fn hex_u32(value: u32) -> Self {
        Self::u32_base(value, Repr::Hex(8), ValueState::Good, "")
    }
    /// Base function for creating a u32 value with different parameters. Usually not called
    /// directly.
    pub fn u32_base(value: u32, repr: Repr, state: ValueState, tip: &str) -> Self {
        SourceValue {
            scalar: Some(Scalar::U32(value)),
            repr,
            tip: (!tip.is_empty()).then_some(tip.to_string()),
            state,
            comment: None,
        }
    }
    /// Create a String scalar value with the defaults.
    pub fn string(value: &str) -> Self {
        SourceValue {
            scalar: Some(Scalar::String(value.to_string())),
            repr: Repr::default(),
            state: ValueState::Good,
            tip: None,
            comment: None,
        }
    }

    // Inline Modifiers

    /// Set the value state to Good or Bad based on the provided predicate
    #[inline]
    pub fn good_if(mut self, predicate: bool) -> Self {
        self.state = if predicate { ValueState::Good } else { ValueState::Bad };
        self
    }
    /// Set the value state to Bad.
    #[inline]
    pub fn bad(mut self) -> Self {
        self.state = ValueState::Bad;
        self
    }
    /// Set the value state to Bad or Good based on the provided predicate
    #[inline]
    pub fn bad_if(mut self, predicate: bool) -> Self {
        self.state = if predicate { ValueState::Bad } else { ValueState::Good };
        self
    }
    /// Set the value state to Questionable.
    #[inline]
    pub fn quest(mut self) -> Self {
        self.state = ValueState::Questionable;
        self
    }
    /// Set the value state to Questionable or Good based on the provided predicate
    #[inline]
    pub fn quest_if(mut self, predicate: bool) -> Self {
        self.state = if predicate {
            ValueState::Questionable
        }
        else {
            ValueState::Good
        };
        self
    }
    /// Set a tooltip for the value.
    #[inline]
    pub fn tip(mut self, tip: &str) -> Self {
        self.tip = (!tip.is_empty()).then_some(tip.to_string());
        self
    }
    /// Set the representation to [Repr] hexadecimal with the provided number of `digits`.
    /// The 0x prefix is not included.
    #[inline]
    pub fn hex(mut self, digits: u8) -> Self {
        self.repr = Repr::Hex(digits);
        self
    }
    /// Set the scalar representation to [Repr] binary with the provided number of `digits`.
    /// The 0b prefix is not included.
    #[inline]
    pub fn bin(mut self, digits: u8) -> Self {
        self.repr = Repr::Bin(digits);
        self
    }
    /// Set the comment string if `comment` is not empty.
    #[inline]
    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = (!comment.is_empty()).then_some(comment.to_string());
        self
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Default)]
pub struct SourceMap {
    map: FoxTreeMap<SourceValue>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self {
            map: FoxTreeMap::new(SourceValue::default()),
        }
    }

    pub fn for_each<F>(&self, f: F)
    where
        F: FnMut(usize, &SourceValue),
    {
        self.map.for_each(f);
    }

    pub fn root(&self) -> usize {
        self.map.root()
    }

    pub fn children(&self, index: usize) -> &[usize] {
        self.map.children(index)
    }

    pub fn node(&self, index: usize) -> (&str, &SourceValue) {
        let node = &self.map.node(index);
        (&node.name, &node.data)
    }
}

// impl FoxTree for SourceMap {
//     type Data = SourceValue;
//
//     fn tree_mut(&mut self) -> &mut FoxTreeMap<Self::Data> {
//         &mut self.map
//     }
//
//     fn tree(&self) -> &FoxTreeMap<Self::Data> {
//         &self.map
//     }
// }

impl Debug for SourceMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.map.debug_with(f, &|data: &SourceValue| {
            if let Some(scalar) = &data.scalar {
                match scalar {
                    Scalar::String(s) => s.clone(),
                    _ => format!("{}{}", data.repr.fmt(scalar), if data.is_bad() { "*" } else { "" }),
                }
            }
            else {
                "".to_string()
            }
        })
    }
}

/// A trait for a source map that can be optionally created by a parser.
/// We can create a null source map that does nothing, to avoid having a lot of conditional code
/// in our parsers.
pub trait OptionalSourceMap: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_some(&self) -> Option<&SourceMap>;
    fn add_child(&mut self, parent: usize, name: &str, data: SourceValue) -> FoxTreeCursor<SourceValue>;
    fn debug_tree(&self);
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;

    fn last_node(&mut self) -> FoxTreeCursor<SourceValue>;
}

impl OptionalSourceMap for SourceMap {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_some(&self) -> Option<&SourceMap> {
        Some(self)
    }

    fn add_child(&mut self, parent: usize, name: &str, data: SourceValue) -> FoxTreeCursor<SourceValue> {
        let child_index = self.map.add_child(parent, name, data);
        FoxTreeCursor::new(&mut self.map, parent, child_index)
    }

    fn debug_tree(&self) {
        let mut visited = FoxHashSet::new();
        self.map.debug_tree(0, 0, &|_| "".to_string(), &mut visited);
    }

    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }

    fn last_node(&mut self) -> FoxTreeCursor<SourceValue> {
        let (parent, last) = self.map.last_node();
        FoxTreeCursor::new(&mut self.map, parent, last)
    }
}

// Null implementation of SourceMap that does nothing
pub struct NullSourceMap {
    tree: FoxTreeMap<SourceValue>,
}

impl NullSourceMap {
    pub(crate) fn new() -> Self {
        Self {
            tree: FoxTreeMap::new(SourceValue::default()),
        }
    }
}

impl OptionalSourceMap for NullSourceMap {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_some(&self) -> Option<&SourceMap> {
        None
    }
    fn add_child(&mut self, _parent: usize, _name: &str, _data: SourceValue) -> FoxTreeCursor<SourceValue> {
        // Always return a cursor to the root; do nothing else
        FoxTreeCursor::new(&mut self.tree, 0, 0)
    }
    fn debug_tree(&self) {
        // No-op
    }
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NullSourceMap")
    }

    fn last_node(&mut self) -> FoxTreeCursor<SourceValue> {
        FoxTreeCursor::new(&mut self.tree, 0, 0)
    }
}

impl Debug for dyn OptionalSourceMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt(f)
    }
}

impl Debug for NullSourceMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NullSourceMap")
    }
}

pub trait MapDump {
    /// Writes the structure's information to the provided `OptionalSourceMap`.
    fn write_to_map(&self, map: &mut Box<dyn OptionalSourceMap>, parent: usize) -> usize;
}
