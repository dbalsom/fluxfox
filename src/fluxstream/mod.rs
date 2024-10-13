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
use std::fmt;
use std::fmt::{Display, Formatter};

pub mod flux_stream;
pub mod pll;

#[derive(PartialEq, Debug)]
pub enum FluxTransition {
    Short,
    Medium,
    Long,
    Other,
}

impl Display for FluxTransition {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            FluxTransition::Short => write!(f, "S"),
            FluxTransition::Medium => write!(f, "M"),
            FluxTransition::Long => write!(f, "L"),
            FluxTransition::Other => write!(f, "X"),
        }
    }
}

impl FluxTransition {
    pub fn to_bits(&self) -> &[bool] {
        match self {
            FluxTransition::Short => &[true, false],
            FluxTransition::Medium => &[true, false, false],
            FluxTransition::Long => &[true, false, false, false],
            FluxTransition::Other => &[],
        }
    }
}
