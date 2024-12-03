pub mod lzhuf;
pub mod lzw;

type DYNERR = Box<dyn std::error::Error>;

/// Tree Errors
#[derive(thiserror::Error, Debug)]
#[allow(unused)]
pub enum CompressionError {
    #[error("file format mismatch")]
    FileFormatMismatch,
    #[error("file too large")]
    FileTooLarge,
    #[error("checksum failed")]
    BadChecksum,
}
