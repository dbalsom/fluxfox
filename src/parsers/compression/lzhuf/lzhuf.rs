/*
    lzhuf.rs

    Original code by Daniel Gordon, retrieved from the 'retrocompressor' repository.
    Minor modifications were made for fluxfox.

    It is assumed that if a BufReader or BufWriter is required, the caller will provide such.

    https://github.com/dfgordon/retrocompressor

    Original comments:

    LZSS Compression with Adaptive Huffman Encoding

    This can perform compression equivalent to the C program `LZHUF.C` by
    Haruyasu Yoshizaki, Haruhiko Okumura, and Kenji Rikitake.  This is not a direct
    port, but it will produce the same bit-for-bit output as `LZHUF.C`, assuming the
    standard options are chosen.  The header is always treated as little endian.

    This program appears to work more reliably than `LZHUF.C`.
    I found that `LZHUF.C` will hang on large files when compiled with `clang 16`,
    among other problems.  One theory is this happens when it gets to the stage
    where the Huffman tree has to be rebuilt, and something goes amiss with the
    C integer types as interpreted by clang (compared to whatever old compiler).
    Neither this module nor the direct port exhibit such problems.
*/

use super::adaptive_huff::*;
use super::node_pool::*;
use super::ring_buffer::*;
use super::Options;
use super::DYNERR;
use crate::io::{Cursor, ErrorKind, Read, Seek, SeekFrom, Write};

/// Structure to perform the LZSS stage of  compression.
/// This maintains two components.  First a sliding window containing
/// the symbols in the order encountered ("dictionary"), and second a
/// tree structure whose nodes point at dictionary locations where matches
/// have been previously found ("index")
#[allow(unused)]
struct LZSS {
    opt: Options,
    dictionary: RingBuffer,
    index: Tree,
    match_offset: i32,
    match_length: usize,
}

impl LZSS {
    fn create(opt: Options) -> Self {
        let dictionary = RingBuffer::create(opt.window_size);
        let index = Tree::create(opt.window_size, 256);
        Self {
            opt,
            dictionary,
            index,
            match_offset: 0,
            match_length: 0,
        }
    }
    /// This finds a match to the symbol run starting at position `pos`.
    /// It always exits by inserting a node: either for a match that was found,
    /// or for a prospective match to come.
    #[allow(dead_code)]
    fn insert_node(&mut self) -> Result<(), Error> {
        let pos = self.dictionary.get_pos(0);
        self.match_length = 0;
        // Whatever is attached at this position can only index things that are ahead of us.
        // Therefore throw it all away. (but see note below)
        self.index.set_cursor(pos)?;
        self.index.drop_branch(Side::Left)?;
        self.index.drop_branch(Side::Right)?;
        // find or create root for this symbol
        let symbol = self.dictionary.get(0);
        let mut curs = match self.index.set_cursor_to_root(symbol as usize) {
            Ok(()) => self.index.get_cursor().unwrap(),
            Err(_) => {
                // Symbol has not been indexed yet, save position and go out.
                self.index.spawn_root(symbol as usize, pos)?;
                return Ok(());
            }
        };
        self.index.set_cursor(curs)?;
        loop {
            let mut cmp = 0;
            let mut i: usize = 1;
            // upon exiting this loop, `i` will have the number of matched symbols,
            // and `cmp` will have the difference in first mismatched symbol values.
            while i < self.opt.lookahead {
                cmp = self.dictionary.get(i as i64) as i16 - self.dictionary.get_abs(curs + i) as i16;
                if cmp != 0 {
                    break;
                }
                i += 1;
            }
            if i > self.opt.threshold {
                if i > self.match_length {
                    // we found a better match, take it
                    self.match_offset = self.dictionary.distance_behind(curs) as i32 - 1;
                    self.match_length = i;
                    if self.match_length >= self.opt.lookahead {
                        // cannot get a better match than this, so remove the prior position from the index,
                        // and index this position in its place. TODO: this seems to break the assumption
                        // that farther from root means later in buffer.
                        self.index.change_value(pos, true)?;
                        return Ok(());
                    }
                }
                if i == self.match_length {
                    // if a match has the same length, but occurs with smaller offset, take it
                    let c = self.dictionary.distance_behind(curs) as i32 - 1;
                    if c < self.match_offset {
                        self.match_offset = c;
                    }
                }
            }
            // try next match on one of two branches, determined by the symbol ordering associated
            // with the last mismatch.
            let side = match cmp >= 0 {
                true => Side::Right,
                false => Side::Left,
            };
            curs = match self.index.down(side) {
                Ok(c) => c,
                Err(Error::NodeMissing) => {
                    // no match, make this position a new node, go out
                    self.index.spawn(pos, side)?;
                    return Ok(());
                }
                Err(e) => {
                    return Err(e);
                }
            };
        }
    }
    fn delete_node(&mut self, offset: i64) -> Result<(), Error> {
        // The big idea here is to delete the node without having to cut a whole branch.
        // If p has only one branch, this is easy, the next node down replaces p.
        // If p has two branches, and the left branch has no right branch, then p's right branch
        // moves down to become the left branch's right branch.  The left branch moves up to replace p.
        // If p has two branches, and the left branch branches right, we go down on the right as deep
        // as possible.  The deepest node is brought up to replace p, see below.
        let p = self.dictionary.get_pos(offset);
        if self.index.is_free(p)? {
            return Ok(());
        }
        self.index.set_cursor(p)?;
        // first assemble the branch that will replace p
        let replacement = match self.index.get_down()? {
            [None, None] => {
                return self.index.drop();
            }
            [Some(repl), None] => repl, // only 1 branch, it moves up to replace p
            [None, Some(repl)] => repl, // only 1 branch, it moves up to replace p
            [Some(left), Some(right)] => {
                // There are 2 branches, we have to rearrange things to avoid losing data.
                self.index.set_cursor(left)?;
                match self.index.get_down()? {
                    [_, None] => {
                        // Left branch does not branch right.
                        // Therefore we can simply attach the right branch to left branch's right branch.
                        // The updated left branch will be the replacement.
                        self.index.set_cursor(right)?;
                        self.index.move_node(left, Side::Right, false)?;
                        left
                    }
                    [_, Some(_)] => {
                        // The left branch branches right, find the terminus on the right.
                        // A right-terminus is not necessarily a leaf, i.e., it can have a left branch.
                        let terminus: usize = self.index.terminus(Side::Right)?;
                        let (terminus_dad, _) = self.index.get_parent_and_side()?;
                        self.index.cut_upward()?;
                        // possible left branch of the terminus takes the former spot of the terminus
                        match self.index.get_down()? {
                            [Some(_), None] => {
                                self.index.down(Side::Left)?;
                                self.index.move_node(terminus_dad, Side::Right, false)?;
                            }
                            [None, None] => {}
                            _ => panic!("unexpected children"),
                        }
                        // The 2 branches of p can now be attached to what was the terminus,
                        // whereas the terminus will be the replacement.
                        self.index.set_cursor(left)?;
                        self.index.move_node(terminus, Side::Left, false)?;
                        self.index.set_cursor(right)?;
                        self.index.move_node(terminus, Side::Right, false)?;
                        terminus
                    }
                }
            }
        };
        // Replace `p` with `replacement`
        self.index.set_cursor(p)?;
        if self.index.is_root()? {
            let symbol = self.index.get_symbol()?;
            self.index.set_cursor(replacement)?;
            self.index.move_node_to_root(symbol, true)
        } else {
            let (parent, side) = self.index.get_parent_and_side()?;
            self.index.set_cursor(replacement)?;
            self.index.move_node(parent, side, true)
        }
    }
}

/// Main compression function.
/// `expanded_in` is an object with `Read` and `Seek` traits, usually `std::fs::File`, or `std::io::Cursor<&[u8]>`.
/// `compressed_out` is an object with `Write` and `Seek` traits, usually `std::fs::File`, or `std::io::Cursor<Vec<u8>>`.
/// Returns (in_size,out_size) or error, can panic if offsets are out of range.
#[allow(dead_code)]
pub fn compress<R, W>(expanded_in: &mut R, compressed_out: &mut W, opt: &super::Options) -> Result<(u64, u64), DYNERR>
where
    R: Read + Seek,
    W: Write + Seek,
{
    let reader = expanded_in;
    let mut writer = compressed_out;
    let expanded_length = reader.seek(SeekFrom::End(0))? - opt.in_offset;
    if expanded_length >= u32::MAX as u64 {
        return Err(Box::new(super::Error::FileTooLarge));
    }
    reader.seek(SeekFrom::Start(opt.in_offset))?;
    writer.seek(SeekFrom::Start(opt.out_offset))?;
    // write the 32-bit header with length of expanded data
    if opt.header {
        let header = u32::to_le_bytes(expanded_length as u32);
        writer.write(&header)?;
    }
    // init
    let mut bytes = reader.bytes();
    let mut lzss = LZSS::create(opt.clone());
    let mut huff = AdaptiveHuffmanCoder::create(256 + opt.lookahead - opt.threshold);
    // setup dictionary
    let start_pos = opt.window_size - opt.lookahead;
    for i in 0..start_pos {
        lzss.dictionary.set(i as i64, opt.precursor);
    }
    let mut len = 0;
    lzss.dictionary.set_pos(start_pos);
    while len < opt.lookahead {
        match bytes.next() {
            Some(Ok(c)) => {
                lzss.dictionary.set(len as i64, c);
                len += 1;
            }
            None => {
                break;
            }
            Some(Err(e)) => {
                return Err(Box::new(e));
            }
        }
    }
    for _i in 1..=opt.lookahead {
        lzss.dictionary.retreat();
        lzss.insert_node()?;
    }
    lzss.dictionary.set_pos(start_pos);
    lzss.insert_node()?;
    // main compression loop
    loop {
        if lzss.match_length > len {
            lzss.match_length = len;
        }
        if lzss.match_length <= opt.threshold {
            lzss.match_length = 1;
            huff.encode_char(lzss.dictionary.get(0) as u16, &mut writer);
        } else {
            huff.encode_char((255 - opt.threshold + lzss.match_length) as u16, &mut writer);
            huff.encode_position(lzss.match_offset as u16, &mut writer);
        }
        let last_match_length = lzss.match_length;
        let mut i = 0;
        while i < last_match_length {
            let c = match bytes.next() {
                Some(Ok(c)) => c,
                None => break,
                Some(Err(e)) => return Err(Box::new(e)),
            };
            lzss.delete_node(opt.lookahead as i64)?;
            lzss.dictionary.set(opt.lookahead as i64, c);
            lzss.dictionary.advance();
            lzss.insert_node()?;
            i += 1;
        }
        while i < last_match_length {
            lzss.delete_node(opt.lookahead as i64)?;
            lzss.dictionary.advance();
            len -= 1;
            if len > 0 {
                lzss.insert_node()?;
            }
            i += 1;
        }
        if len <= 0 {
            break;
        }
    }
    writer.seek(SeekFrom::End(0))?; // coder could be rewound
    writer.flush()?;
    Ok((expanded_length, writer.stream_position()? - opt.out_offset))
}

/// Main decompression function.
/// `compressed_in` is an object with `Read` and `Seek` traits, usually `std::fs::File`, or `std::io::Cursor<&[u8]>`.
/// `expanded_out` is an object with `Write` and `Seek` traits, usually `std::fs::File`, or `std::io::Cursor<Vec<u8>>`.
/// Returns (in_size,out_size) or error, can panic if offsets are out of range.
pub fn expand<R, W>(compressed_in: &mut R, expanded_out: &mut W, opt: &super::Options) -> Result<(u64, u64), DYNERR>
where
    R: Read + Seek,
    W: Write + Seek,
{
    let mut reader = compressed_in;
    let writer = expanded_out;
    let compressed_size = reader.seek(SeekFrom::End(0))? - opt.in_offset;
    reader.seek(SeekFrom::Start(opt.in_offset))?;
    writer.seek(SeekFrom::Start(opt.out_offset))?;
    // get size of expanded data from 32 bit header or set to max
    let max_expanded_size = match opt.header {
        true => {
            let mut header: [u8; 4] = [0; 4];
            reader.read_exact(&mut header)?;
            u32::from_le_bytes(header)
        }
        false => u32::MAX,
    };
    // init
    let mut huff = AdaptiveHuffmanDecoder::create(256 + opt.lookahead - opt.threshold);
    let mut lzss = LZSS::create(opt.clone());
    let start_pos = opt.window_size - opt.lookahead;
    for i in 0..start_pos {
        lzss.dictionary.set(i as i64, opt.precursor);
    }
    lzss.dictionary.set_pos(start_pos);
    // start expanding
    while writer.stream_position()? < max_expanded_size as u64 {
        let c = match huff.decode_char(&mut reader) {
            Ok(c) => c,
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(Box::new(e)),
        };
        if c < 256 {
            writer.write(&[c as u8])?;
            lzss.dictionary.set(0, c as u8);
            lzss.dictionary.advance();
        } else {
            let offset = match huff.decode_position(&mut reader) {
                Ok(pos) => -(pos as i64 + 1),
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(Box::new(e)),
            };
            let strlen = c as i64 + opt.threshold as i64 - 255;
            for _k in 0..strlen {
                let c8 = lzss.dictionary.get(offset);
                writer.write(&[c8])?;
                lzss.dictionary.set(0, c8 as u8);
                lzss.dictionary.advance();
            }
        }
    }
    writer.flush()?;
    Ok((compressed_size, writer.stream_position()? - opt.out_offset))
}

/// Convenience function, calls `compress` with a slice returning a Vec
#[allow(dead_code)]
pub fn compress_slice(slice: &[u8], opt: &super::Options) -> Result<Vec<u8>, DYNERR> {
    let mut src = Cursor::new(slice);
    let mut ans: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    compress(&mut src, &mut ans, opt)?;
    Ok(ans.into_inner())
}

/// Convenience function, calls `expand` with a slice returning a Vec
#[allow(dead_code)]
pub fn expand_slice(slice: &[u8], opt: &super::Options) -> Result<Vec<u8>, DYNERR> {
    let mut src = Cursor::new(slice);
    let mut ans: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    expand(&mut src, &mut ans, opt)?;
    Ok(ans.into_inner())
}

#[test]
fn compression_works() {
    use super::STD_OPTIONS;
    let test_data = "12345123456789123456789\n".as_bytes();
    let lzhuf_str = "18 00 00 00 DE EF B7 FC 0E 0C 70 13 85 C3 E2 71 64 81 19 60";
    let compressed = compress_slice(test_data, &STD_OPTIONS).expect("compression failed");
    assert_eq!(compressed, hex::decode(lzhuf_str.replace(" ", "")).unwrap());

    let test_data = "I am Sam. Sam I am. I do not like this Sam I am.\n".as_bytes();
    let lzhuf_str = "31 00 00 00 EA EB 3D BF 9C 4E FE 1E 16 EA 34 09 1C 0D C0 8C 02 FC 3F 77 3F 57 20 17 7F 1F 5F BF C6 AB 7F A5 AF FE 4C 39 96";
    let compressed = compress_slice(test_data, &STD_OPTIONS).expect("compression failed");
    assert_eq!(compressed, hex::decode(lzhuf_str.replace(" ", "")).unwrap());
}

#[test]
fn invertibility() {
    use super::STD_OPTIONS;
    let test_data = "I am Sam. Sam I am. I do not like this Sam I am.\n".as_bytes();
    let compressed = compress_slice(test_data, &STD_OPTIONS).expect("compression failed");
    let expanded = expand_slice(&compressed, &STD_OPTIONS).expect("expansion failed");
    assert_eq!(test_data.to_vec(), expanded);

    let test_data = "1234567".as_bytes();
    let compressed = compress_slice(test_data, &STD_OPTIONS).expect("compression failed");
    let expanded = expand_slice(&compressed, &STD_OPTIONS).expect("expansion failed");
    assert_eq!(test_data.to_vec(), expanded[0..7]);
}
