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

    src/random.rs

    Provide a simple random bit generator.
*/

#![allow(dead_code)]

const RANDOM_BITS_SIZE: usize = 2048;

const PSEUDO_RANDOM_BITS: [bool; RANDOM_BITS_SIZE] = generate_pseudo_random_bits(0x57A857FA, RANDOM_BITS_SIZE);

const fn pseudo_random_bit(seed: u32, index: usize) -> bool {
    // A simple pseudo-random function using bit shifts and XOR
    let mut value = seed ^ (index as u32);
    value = value.wrapping_mul(0x45d9f3b);
    value ^= value >> 16;
    (value & 1) != 0
}

const fn generate_pseudo_random_bits(seed: u32, len: usize) -> [bool; RANDOM_BITS_SIZE] {
    let mut bits = [false; RANDOM_BITS_SIZE];
    let mut i = 0;
    while i < len {
        bits[i] = pseudo_random_bit(seed, i);
        i += 1;
    }
    bits
}

pub fn random_bit(index: usize) -> bool {
    PSEUDO_RANDOM_BITS[index & (RANDOM_BITS_SIZE - 1)]
}

pub fn random_bit_ref(index: usize) -> &'static bool {
    &PSEUDO_RANDOM_BITS[index & (RANDOM_BITS_SIZE - 1)]
}
