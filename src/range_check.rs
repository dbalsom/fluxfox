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

    src/range_check.rs

    Implement an O(log n) range checker for detecting if a value is within a range.
*/

#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct RangeChecker {
    events: Vec<(usize, i32)>, // (value, type), where type is +1 for start, -1 for end
}

impl RangeChecker {
    pub fn new(ranges: Vec<(usize, usize)>) -> Self {
        let mut events = Vec::new();
        for (start, end) in ranges {
            events.push((start, 1)); // Start of range
            events.push((end + 1, -1)); // End of range, exclusive
        }
        events.sort_unstable();
        RangeChecker { events }
    }

    pub fn contains(&self, value: usize) -> bool {
        let mut active_ranges = 0;
        for &(point, event_type) in &self.events {
            if point > value {
                break;
            }
            active_ranges += event_type;
        }
        active_ranges > 0
    }
}
