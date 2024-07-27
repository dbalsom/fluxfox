//! Module to perform the adaptive Huffman coding.
//! This is used by the `lzss_huff` module.
//! This is supposed to perform the coding the same way as `LZHUF.C`,
//! see the `direct_ports` module for more on the legacy.

use bit_vec::BitVec;
use std::io::{Read, Seek, SeekFrom, Write};

/// Tree used for both encoding and decoding.
/// The tree is constantly updated during either operation.
pub struct AdaptiveHuffmanTree {
    max_freq: usize,
    num_symb: usize,
    node_count: usize,
    root: usize,
    /// node frequency and sorting key, extra is the frequency backstop
    freq: Vec<usize>,
    /// index of parent node of the node in this slot
    parent: Vec<usize>,
    /// index of the left son of the node in this slot, right son is found by incrementing by 1
    son: Vec<usize>,
    /// map from symbols (index) to leaves (value)
    symb_map: Vec<usize>,
}

pub struct AdaptiveHuffmanCoder {
    tree: AdaptiveHuffmanTree,
    bits: BitVec,
    ptr: usize,
}

pub struct AdaptiveHuffmanDecoder {
    tree: AdaptiveHuffmanTree,
    bits: BitVec,
    ptr: usize,
}

/// encoding table giving number of bits used to encode the
/// upper 6 bits of the position
const P_LEN: [u8; 64] = [
    0x03, 0x04, 0x04, 0x04, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06,
    0x06, 0x06, 0x06, 0x06, 0x06, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07,
    0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
    0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
];

/// codes for the upper 6 bits of position, the P_LEN
/// most significant bits are the code, remaining bits should
/// not be written.
const P_CODE: [u8; 64] = [
    0x00, 0x20, 0x30, 0x40, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80, 0x88, 0x90, 0x94, 0x98, 0x9C, 0xA0, 0xA4, 0xA8,
    0xAC, 0xB0, 0xB4, 0xB8, 0xBC, 0xC0, 0xC2, 0xC4, 0xC6, 0xC8, 0xCA, 0xCC, 0xCE, 0xD0, 0xD2, 0xD4, 0xD6, 0xD8, 0xDA,
    0xDC, 0xDE, 0xE0, 0xE2, 0xE4, 0xE6, 0xE8, 0xEA, 0xEC, 0xEE, 0xF0, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
    0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xFF,
];

/// decoding table for number of bits used to encode the
/// upper 6 bits of the position, the index is the code
/// plus some few bits on the right that don't matter
/// (extra bits are the MSB's of the lower 6 bits)
const D_LEN: [u8; 256] = [
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x04, 0x04, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06,
    0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06,
    0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06,
    0x06, 0x06, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07,
    0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07,
    0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
    0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08,
];

/// values for the upper 6 bits of position, indexing is
/// the same as for D_LEN
const D_CODE: [u8; 256] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
    0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x03, 0x03, 0x03, 0x03, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    0x05, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x08, 0x08,
    0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x09, 0x09, 0x09, 0x09, 0x09, 0x09, 0x09, 0x09, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A,
    0x0A, 0x0A, 0x0A, 0x0B, 0x0B, 0x0B, 0x0B, 0x0B, 0x0B, 0x0B, 0x0B, 0x0C, 0x0C, 0x0C, 0x0C, 0x0D, 0x0D, 0x0D, 0x0D,
    0x0E, 0x0E, 0x0E, 0x0E, 0x0F, 0x0F, 0x0F, 0x0F, 0x10, 0x10, 0x10, 0x10, 0x11, 0x11, 0x11, 0x11, 0x12, 0x12, 0x12,
    0x12, 0x13, 0x13, 0x13, 0x13, 0x14, 0x14, 0x14, 0x14, 0x15, 0x15, 0x15, 0x15, 0x16, 0x16, 0x16, 0x16, 0x17, 0x17,
    0x17, 0x17, 0x18, 0x18, 0x19, 0x19, 0x1A, 0x1A, 0x1B, 0x1B, 0x1C, 0x1C, 0x1D, 0x1D, 0x1E, 0x1E, 0x1F, 0x1F, 0x20,
    0x20, 0x21, 0x21, 0x22, 0x22, 0x23, 0x23, 0x24, 0x24, 0x25, 0x25, 0x26, 0x26, 0x27, 0x27, 0x28, 0x28, 0x29, 0x29,
    0x2A, 0x2A, 0x2B, 0x2B, 0x2C, 0x2C, 0x2D, 0x2D, 0x2E, 0x2E, 0x2F, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36,
    0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F,
];

impl AdaptiveHuffmanTree {
    pub fn create(num_symbols: usize) -> Self {
        let mut ans = Self {
            max_freq: 0x8000,
            num_symb: num_symbols,
            node_count: 2 * num_symbols - 1,
            root: 2 * num_symbols - 2,
            freq: vec![0; 2 * num_symbols],
            parent: vec![0; 2 * num_symbols - 1],
            son: vec![0; 2 * num_symbols - 1],
            symb_map: vec![0; num_symbols],
        };
        // Leaves are stored first, one for each symbol (character)
        // leaves are signaled by son[i] >= node_count
        for i in 0..ans.num_symb {
            ans.freq[i] = 1;
            ans.son[i] = i + ans.node_count;
            ans.symb_map[i] = i;
        }
        // Next construct the branches and root, there are num_symb-1 non-leaf nodes.
        // The sons will be 0,2,4,...,node_count-3, these are left sons, the right sons
        // are not explicitly stored, because we always have rson[i] = lson[i] + 1
        // parent will be n,n,n+1,n+1,n+2,n+2,...,n+node_count-1,n+node_count-1
        // Frequency (freq) of a parent node is the sum of the frequencies attached to it.
        // Note the frequencies will be in ascending order.
        let mut i = 0;
        let mut j = ans.num_symb;
        while j <= ans.root {
            ans.freq[j] = ans.freq[i] + ans.freq[i + 1];
            ans.son[j] = i;
            ans.parent[i] = j;
            ans.parent[i + 1] = j;
            i += 2;
            j += 1;
        }
        // last frequency entry is a backstop that prevents any frequency from moving
        // beyond the end of the array (must be larger than any possible frequency)
        ans.freq[ans.node_count] = 0xffff;
        ans.parent[ans.root] = 0;
        ans
    }
    /// Rebuild the adaptive Huffman tree, triggered by frequency hitting the maximum.
    fn rebuild_huff(&mut self) {
        // Collect leaf nodes from anywhere and pack them on the left.
        // Replace the freq of every leaf by (freq+1)/2.
        let mut j = 0;
        for i in 0..self.node_count {
            if self.son[i] >= self.node_count {
                self.freq[j] = (self.freq[i] + 1) / 2;
                self.son[j] = self.son[i];
                j += 1;
            }
        }
        // Connect sons, old connections are not used in any way.
        // LZHUF has i,j,k as signed, seems to be no reason.
        let mut i: usize = 0; // left son
        j = self.num_symb; // parent node - should already be num_symb
        let mut k: usize; // right son or sorting reference
        let mut f: usize; // sum of lson and rson frequencies
        let mut l: usize; // offset from sorting reference to parent node
        while j < self.node_count {
            // first set parent frequency, supposing i,k are sons
            k = i + 1;
            f = self.freq[i] + self.freq[k];
            self.freq[j] = f;
            // make k the farthest node with frequency > this frequency
            k = j - 1;
            while f < self.freq[k] {
                k -= 1;
            }
            k += 1;
            // insert parent of i at position k
            l = (j - k) * 2;
            for kp in (k..k + l).rev() {
                self.freq[kp + 1] = self.freq[kp]
            }
            self.freq[k] = f;
            for kp in (k..k + l).rev() {
                self.son[kp + 1] = self.son[kp]
            }
            self.son[k] = i;
            i += 2; // next left son
            j += 1; // next parent
        }
        // Connect parents.
        // In this loop i is the parent, k is the son
        for i in 0..self.node_count {
            k = self.son[i];
            if k >= self.node_count {
                // k is a leaf, connect to symbol table
                self.symb_map[k - self.node_count] = i;
            } else {
                // k=left son, k+1=right son
                self.parent[k] = i;
                self.parent[k + 1] = i;
            }
        }
    }
    /// increment frequency of given code by one, and update tree
    fn update(&mut self, c0: i16) {
        let mut i: usize;
        let mut j: usize;
        let mut k: usize;
        let mut l: usize;
        if self.freq[self.root] == self.max_freq {
            self.rebuild_huff()
        }
        // the leaf node corresponding to this character
        let mut c = self.symb_map[c0 as usize];
        // sorting loop, node pool is arranged in ascending frequency order
        loop {
            self.freq[c] += 1;
            k = self.freq[c];
            // if order is disturbed, exchange nodes
            l = c + 1;
            if k > self.freq[l] {
                while k > self.freq[l] {
                    l += 1;
                }
                l -= 1;
                // swap the node being checked with the farthest one that is smaller than it
                self.freq[c] = self.freq[l];
                self.freq[l] = k;

                i = self.son[c];
                if i < self.node_count {
                    self.parent[i] = l;
                    self.parent[i + 1] = l;
                } else {
                    self.symb_map[i - self.node_count] = l;
                }

                j = self.son[l];
                self.son[l] = i;

                if j < self.node_count {
                    self.parent[j] = c;
                    self.parent[j + 1] = c;
                } else {
                    self.symb_map[j - self.node_count] = c;
                }
                self.son[c] = j;

                c = l;
            }
            c = self.parent[c];
            if c == 0 {
                break; // root was reached
            }
        }
    }
}

#[allow(dead_code)]
impl AdaptiveHuffmanCoder {
    pub fn create(num_symbols: usize) -> Self {
        Self {
            tree: AdaptiveHuffmanTree::create(num_symbols),
            bits: BitVec::new(),
            ptr: 0,
        }
    }
    /// keep the bit vector small, we don't need the bits behind us
    fn drop_leading_bits(&mut self) {
        let cpy = self.bits.clone();
        self.bits = BitVec::new();
        for i in self.ptr..cpy.len() {
            self.bits.push(cpy.get(i).unwrap());
        }
        self.ptr = 0;
    }
    /// output `num_bits` of `code` starting from the MSB, unlike LZHUF.C the bits are always
    /// written to the output stream (sometimes backing up and rewriting)
    fn put_code<W: Write + Seek>(&mut self, num_bits: u16, mut code: u16, writer: &mut W) {
        for _i in 0..num_bits {
            self.bits.push(code & 0x8000 > 0);
            code <<= 1;
            self.ptr += 1;
        }
        let bytes = self.bits.to_bytes();
        writer.write(&bytes.as_slice()).expect("write err");
        if self.bits.len() % 8 > 0 {
            writer.seek(SeekFrom::Current(-1)).expect("seek err");
            self.ptr = 8 * (self.bits.len() / 8);
            self.drop_leading_bits();
        } else {
            self.bits = BitVec::new();
            self.ptr = 0;
        }
    }
    pub fn encode_char<W: Write + Seek>(&mut self, c: u16, writer: &mut W) {
        let mut code: u16 = 0;
        let mut num_bits: u16 = 0;
        let mut curr_node: usize = self.tree.symb_map[c as usize];
        // This is the Huffman scheme: going from leaf to root, add a 0 bit if we
        // are coming from the left, or a 1 bit if we are coming from the right.
        loop {
            code >>= 1;
            // if node's address is odd-numbered, we are coming from the right
            code += (curr_node as u16 & 1) << 15;
            num_bits += 1;
            curr_node = self.tree.parent[curr_node];
            if curr_node == self.tree.root {
                break;
            }
        }
        self.put_code(num_bits, code, writer);
        self.tree.update(c as i16); // TODO: why is input to update signed
    }
    pub fn encode_position<W: Write + Seek>(&mut self, c: u16, writer: &mut W) {
        // upper 6 bits come from table
        let i = (c >> 6) as usize;
        self.put_code(P_LEN[i] as u16, (P_CODE[i] as u16) << 8, writer);
        // lower 6 bits verbatim
        self.put_code(6, (c & 0x3f) << 10, writer);
    }
}

impl AdaptiveHuffmanDecoder {
    pub fn create(num_symbols: usize) -> Self {
        Self {
            tree: AdaptiveHuffmanTree::create(num_symbols),
            bits: BitVec::new(),
            ptr: 0,
        }
    }
    /// keep the bit vector small, we don't need the bits behind us
    fn drop_leading_bits(&mut self) {
        let cpy = self.bits.clone();
        self.bits = BitVec::new();
        for i in self.ptr..cpy.len() {
            self.bits.push(cpy.get(i).unwrap());
        }
        self.ptr = 0;
    }
    /// Get the next bit reading from the stream as needed.
    /// When EOF is reached 0 is returned, consistent with original C code.
    /// `reader` should not be advanced outside this function until decoding is done.
    fn get_bit<R: Read>(&mut self, reader: &mut R) -> Result<u8, std::io::Error> {
        match self.bits.get(self.ptr) {
            Some(bit) => {
                self.ptr += 1;
                Ok(bit as u8)
            }
            None => {
                let mut by: [u8; 1] = [0];
                match reader.read_exact(&mut by) {
                    Ok(()) => {
                        if self.bits.len() > 512 {
                            self.drop_leading_bits();
                        }
                        self.bits.append(&mut BitVec::from_bytes(&by));
                        self.get_bit(reader)
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }
    /// get the next 8 bits into a u8, used exclusively to decode the position
    fn get_byte<R: Read>(&mut self, bytes: &mut R) -> Result<u8, std::io::Error> {
        let mut ans: u8 = 0;
        for _i in 0..8 {
            ans <<= 1;
            ans |= self.get_bit(bytes)?;
        }
        Ok(ans)
    }
    pub fn decode_char<R: Read>(&mut self, reader: &mut R) -> Result<i16, std::io::Error> {
        let mut c: usize = self.tree.son[self.tree.root];
        // This is the Huffman scheme: go from root to leaf, branching left or right depending on the
        // successive bits.  The nodes are arranged so that branching left or right means adding 0 or
        // 1 to the index.  Remember leaves are signaled by son >= node_count.
        while c < self.tree.node_count {
            c += self.get_bit(reader)? as usize;
            c = self.tree.son[c];
        }
        c -= self.tree.node_count;
        self.tree.update(c as i16); // TODO: why is input to update signed
        Ok(c as i16)
    }
    pub fn decode_position<R: Read>(&mut self, reader: &mut R) -> Result<u16, std::io::Error> {
        // get upper 6 bits from table
        let mut first8 = self.get_byte(reader)? as u16;
        let upper6 = (D_CODE[first8 as usize] as u16) << 6;
        let coded_bits = D_LEN[first8 as usize] as u16;
        // read lower 6 bits verbatim
        // we already got 8 bits, we need another 6 - (8-coded_bits) = coded_bits - 2
        for _i in 0..coded_bits - 2 {
            first8 <<= 1;
            first8 += self.get_bit(reader)? as u16;
        }
        Ok(upper6 | (first8 & 0x3f))
    }
}
