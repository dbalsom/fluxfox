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

//! Byte -> Unicode mapping for IBM OEM Code Page 437 - the original character
//! set of the IBM PC.
//! https://en.wikipedia.org/wiki/Code_page_437

use crate::character_encoding::Chr;

pub const CP437: [Chr; 256] = [
    Chr::C('\x00'),
    Chr::C('\x01'),
    Chr::C('\x02'),
    Chr::C('\x03'),
    Chr::C('\x04'),
    Chr::C('\x05'),
    Chr::C('\x06'),
    Chr::C('\x07'),
    Chr::C('\x08'),
    Chr::C('\x09'),
    Chr::C('\x0A'),
    Chr::C('\x0B'),
    Chr::C('\x0C'),
    Chr::C('\x0D'),
    Chr::C('\x0E'),
    Chr::C('\x0F'),
    Chr::C('\x10'),
    Chr::C('\x11'),
    Chr::C('\x12'),
    Chr::C('\x13'),
    Chr::C('\x14'),
    Chr::C('\x15'),
    Chr::C('\x16'),
    Chr::C('\x17'),
    Chr::C('\x18'),
    Chr::C('\x19'),
    Chr::C('\x1A'),
    Chr::C('\x1B'),
    Chr::C('\x1C'),
    Chr::C('\x1D'),
    Chr::C('\x1E'),
    Chr::C('\x1F'),
    Chr::P(' '),
    Chr::P('!'),
    Chr::P('"'),
    Chr::P('#'),
    Chr::P('$'),
    Chr::P('%'),
    Chr::P('&'),
    Chr::P('\''),
    Chr::P('('),
    Chr::P(')'),
    Chr::P('*'),
    Chr::P('+'),
    Chr::P(','),
    Chr::P('-'),
    Chr::P('.'),
    Chr::P('/'),
    Chr::P('0'),
    Chr::P('1'),
    Chr::P('2'),
    Chr::P('3'),
    Chr::P('4'),
    Chr::P('5'),
    Chr::P('6'),
    Chr::P('7'),
    Chr::P('8'),
    Chr::P('9'),
    Chr::P(':'),
    Chr::P(';'),
    Chr::P('<'),
    Chr::P('='),
    Chr::P('>'),
    Chr::P('?'),
    Chr::P('@'),
    Chr::P('A'),
    Chr::P('B'),
    Chr::P('C'),
    Chr::P('D'),
    Chr::P('E'),
    Chr::P('F'),
    Chr::P('G'),
    Chr::P('H'),
    Chr::P('I'),
    Chr::P('J'),
    Chr::P('K'),
    Chr::P('L'),
    Chr::P('M'),
    Chr::P('N'),
    Chr::P('O'),
    Chr::P('P'),
    Chr::P('Q'),
    Chr::P('R'),
    Chr::P('S'),
    Chr::P('T'),
    Chr::P('U'),
    Chr::P('V'),
    Chr::P('W'),
    Chr::P('X'),
    Chr::P('Y'),
    Chr::P('Z'),
    Chr::P('['),
    Chr::P('\\'),
    Chr::P(']'),
    Chr::P('^'),
    Chr::P('_'),
    Chr::P('`'),
    Chr::P('a'),
    Chr::P('b'),
    Chr::P('c'),
    Chr::P('d'),
    Chr::P('e'),
    Chr::P('f'),
    Chr::P('g'),
    Chr::P('h'),
    Chr::P('i'),
    Chr::P('j'),
    Chr::P('k'),
    Chr::P('l'),
    Chr::P('m'),
    Chr::P('n'),
    Chr::P('o'),
    Chr::P('p'),
    Chr::P('q'),
    Chr::P('r'),
    Chr::P('s'),
    Chr::P('t'),
    Chr::P('u'),
    Chr::P('v'),
    Chr::P('w'),
    Chr::P('x'),
    Chr::P('y'),
    Chr::P('z'),
    Chr::P('{'),
    Chr::P('|'),
    Chr::P('}'),
    Chr::P('~'),
    Chr::C('\x7F'),
    Chr::P('\u{00C7}'),
    Chr::P('\u{00FC}'),
    Chr::P('\u{00E9}'),
    Chr::P('\u{00E2}'),
    Chr::P('\u{00E4}'),
    Chr::P('\u{00E0}'),
    Chr::P('\u{00E5}'),
    Chr::P('\u{00E7}'),
    Chr::P('\u{00EA}'),
    Chr::P('\u{00EB}'),
    Chr::P('\u{00E8}'),
    Chr::P('\u{00EF}'),
    Chr::P('\u{00EE}'),
    Chr::P('\u{00EC}'),
    Chr::P('\u{00C4}'),
    Chr::P('\u{00C5}'),
    Chr::P('\u{00C9}'),
    Chr::P('\u{00E6}'),
    Chr::P('\u{00C6}'),
    Chr::P('\u{00F4}'),
    Chr::P('\u{00F6}'),
    Chr::P('\u{00F2}'),
    Chr::P('\u{00FB}'),
    Chr::P('\u{00F9}'),
    Chr::P('\u{00FF}'),
    Chr::P('\u{00D6}'),
    Chr::P('\u{00DC}'),
    Chr::P('\u{00A2}'),
    Chr::P('\u{00A3}'),
    Chr::P('\u{00A5}'),
    Chr::P('\u{20A7}'),
    Chr::P('\u{0192}'),
    Chr::P('\u{00E1}'),
    Chr::P('\u{00ED}'),
    Chr::P('\u{00F3}'),
    Chr::P('\u{00FA}'),
    Chr::P('\u{00F1}'),
    Chr::P('\u{00D1}'),
    Chr::P('\u{00AA}'),
    Chr::P('\u{00BA}'),
    Chr::P('\u{00BF}'),
    Chr::P('\u{00AE}'),
    Chr::P('\u{00AC}'),
    Chr::P('\u{00BD}'),
    Chr::P('\u{00BC}'),
    Chr::P('\u{00A1}'),
    Chr::P('\u{00AB}'),
    Chr::P('\u{00BB}'),
    Chr::P('\u{2591}'),
    Chr::P('\u{2592}'),
    Chr::P('\u{2593}'),
    Chr::P('\u{2502}'),
    Chr::P('\u{2524}'),
    Chr::P('\u{2561}'),
    Chr::P('\u{2562}'),
    Chr::P('\u{2556}'),
    Chr::P('\u{2555}'),
    Chr::P('\u{2563}'),
    Chr::P('\u{2551}'),
    Chr::P('\u{2557}'),
    Chr::P('\u{255D}'),
    Chr::P('\u{255C}'),
    Chr::P('\u{255B}'),
    Chr::P('\u{2510}'),
    Chr::P('\u{2514}'),
    Chr::P('\u{2534}'),
    Chr::P('\u{252C}'),
    Chr::P('\u{251C}'),
    Chr::P('\u{2500}'),
    Chr::P('\u{253C}'),
    Chr::P('\u{255E}'),
    Chr::P('\u{255F}'),
    Chr::P('\u{255A}'),
    Chr::P('\u{2554}'),
    Chr::P('\u{2569}'),
    Chr::P('\u{2566}'),
    Chr::P('\u{2560}'),
    Chr::P('\u{2550}'),
    Chr::P('\u{256C}'),
    Chr::P('\u{2567}'),
    Chr::P('\u{2568}'),
    Chr::P('\u{2564}'),
    Chr::P('\u{2565}'),
    Chr::P('\u{2559}'),
    Chr::P('\u{2558}'),
    Chr::P('\u{2552}'),
    Chr::P('\u{2553}'),
    Chr::P('\u{256B}'),
    Chr::P('\u{256A}'),
    Chr::P('\u{2518}'),
    Chr::P('\u{250C}'),
    Chr::P('\u{2588}'),
    Chr::P('\u{2584}'),
    Chr::P('\u{258C}'),
    Chr::P('\u{2590}'),
    Chr::P('\u{2580}'),
    Chr::P('\u{03B1}'),
    Chr::P('\u{00DF}'),
    Chr::P('\u{0393}'),
    Chr::P('\u{03C0}'),
    Chr::P('\u{03A3}'),
    Chr::P('\u{03C3}'),
    Chr::P('\u{00B5}'),
    Chr::P('\u{03C4}'),
    Chr::P('\u{03A6}'),
    Chr::P('\u{0398}'),
    Chr::P('\u{03A9}'),
    Chr::P('\u{03B4}'),
    Chr::P('\u{221E}'),
    Chr::P('\u{03C6}'),
    Chr::P('\u{03B5}'),
    Chr::P('\u{2229}'),
    Chr::P('\u{2261}'),
    Chr::P('\u{00B1}'),
    Chr::P('\u{2265}'),
    Chr::P('\u{2264}'),
    Chr::P('\u{2320}'),
    Chr::P('\u{2321}'),
    Chr::P('\u{00F7}'),
    Chr::P('\u{2248}'),
    Chr::P('\u{00B0}'),
    Chr::P('\u{2219}'),
    Chr::P('\u{00B7}'),
    Chr::P('\u{221A}'),
    Chr::P('\u{207F}'),
    Chr::P('\u{00B2}'),
    Chr::P('\u{25A0}'),
    Chr::P('\u{00A0}'),
];
