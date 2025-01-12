/*
    FluxFox
    https://github.com/dbalsom/fluxfox

    Copyright 2024-2025 Daniel Balsom

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

/// The [DataSegmenter] struct implements an iterator that yields exactly `divisor`
/// values whose sum is exactly `dividend`. The values are as evenly distributed as
/// possible between `base` and `base + 1`.
pub struct DataSegmenter {
    divisor: usize,
    base: usize,
    remainder_step: usize,
    remainder_accum: usize,
    count: usize,
}

impl DataSegmenter {
    pub fn new(dividend: usize, divisor: usize) -> DataSegmenter {
        // Avoid dividing by zero
        assert!(divisor > 0, "divisor must be > 0");

        let base = dividend / divisor;
        let remainder = dividend % divisor;

        DataSegmenter {
            divisor,
            base,
            remainder_step: remainder,
            remainder_accum: 0,
            count: 0,
        }
    }
}

impl Iterator for DataSegmenter {
    type Item = usize;

    /// Return the next item in the sequence, or None if we've yielded `divisor` items.
    fn next(&mut self) -> Option<Self::Item> {
        // If we've already yielded everything, return None
        if self.count >= self.divisor {
            return None;
        }
        self.count += 1;

        // Accumulate remainder
        self.remainder_accum += self.remainder_step;

        // If remainder_accum >= divisor, we've crossed a boundary
        // that means we add an extra +1 for this element.
        if self.remainder_accum >= self.divisor {
            self.remainder_accum -= self.divisor;
            Some(self.base + 1)
        }
        else {
            Some(self.base)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_segmenter_even_distribution() {
        let segmenter = DataSegmenter::new(10, 5);
        let result: Vec<usize> = segmenter.collect();
        assert_eq!(result, vec![2, 2, 2, 2, 2]);
    }

    #[test]
    fn data_segmenter_uneven_distribution() {
        let segmenter = DataSegmenter::new(20, 6);
        let result: Vec<usize> = segmenter.collect();
        assert_eq!(result, vec![3, 3, 4, 3, 3, 4]);
    }

    #[test]
    fn data_segmenter_single_divisor() {
        let segmenter = DataSegmenter::new(10, 1);
        let result: Vec<usize> = segmenter.collect();
        assert_eq!(result, vec![10]);
    }

    #[test]
    fn data_segmenter_dividend_zero() {
        let segmenter = DataSegmenter::new(0, 5);
        let result: Vec<usize> = segmenter.collect();
        assert_eq!(result, vec![0, 0, 0, 0, 0]);
    }

    #[test]
    fn data_segmenter_divisor_greater_than_dividend() {
        let segmenter = DataSegmenter::new(3, 5);
        let result: Vec<usize> = segmenter.collect();
        assert_eq!(result, vec![0, 1, 0, 1, 1]);
    }
}
