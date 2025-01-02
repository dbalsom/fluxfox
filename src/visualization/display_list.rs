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

//! A [VizDisplayList] is a list of [VizElement] objects to be rendered.
//! Operations can be implemented on this list, such as scaling and rotation.

use crate::visualization::{types::VizElement, TurningDirection, VizRotate};

/// A [VizDisplayList] is a list of [VizElement] objects to be rendered.
/// Operations can be implemented on this list, such as scaling and rotation.
pub struct VizDisplayList {
    pub turning:  TurningDirection,
    pub elements: Vec<VizElement>,
}

impl VizDisplayList {
    pub fn new(turning: TurningDirection) -> VizDisplayList {
        VizDisplayList {
            turning,
            elements: Vec::new(),
        }
    }

    pub fn push(&mut self, element: VizElement) {
        self.elements.push(element);
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn rotate(&mut self, angle: f32) {
        for element in self.elements.iter_mut() {
            element.rotate(angle);
        }
    }

    pub fn iter(&self) -> VizDisplayListIter {
        VizDisplayListIter {
            iter: self.elements.iter(),
        }
    }
}

// Iterator struct
pub struct VizDisplayListIter<'a> {
    iter: std::slice::Iter<'a, VizElement>,
}

impl<'a> Iterator for VizDisplayListIter<'a> {
    type Item = &'a VizElement;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
