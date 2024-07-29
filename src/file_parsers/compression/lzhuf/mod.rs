pub mod adaptive_huff;
pub mod lzhuf;
pub mod node_pool;
pub mod ring_buffer;

type DYNERR = Box<dyn std::error::Error>;

/// Tree Errors
#[derive(thiserror::Error, Debug)]
#[allow(unused)]
pub enum Error {
    #[error("file format mismatch")]
    FileFormatMismatch,
    #[error("file too large")]
    FileTooLarge,
    #[error("checksum failed")]
    BadChecksum,
}

/// Options controlling compression
#[derive(Clone)]
pub struct Options {
    /// whether to include an optional header
    header: bool,
    /// starting position in the input file
    in_offset: u64,
    /// starting position in the output file
    out_offset: u64,
    /// size of window, e.g., for LZSS dictionary
    window_size: usize,
    /// threshold, e.g. minimum length of match to encode
    threshold: usize,
    /// lookahead, e.g. for LZSS matches
    lookahead: usize,
    /// precursor symbol, e.g. backfill symbol for LZSS dictionary
    precursor: u8,
}

#[allow(unused)]
pub const STD_OPTIONS: Options = Options {
    header: true,
    in_offset: 0,
    out_offset: 0,
    window_size: 4096,
    threshold: 2,
    lookahead: 60,
    precursor: b' ',
};

pub const TD0_READ_OPTIONS: Options = Options {
    header: false,
    in_offset: 12,
    out_offset: 0,
    window_size: 4096,
    threshold: 2,
    lookahead: 60,
    precursor: b' ',
};

pub use lzhuf::expand;
