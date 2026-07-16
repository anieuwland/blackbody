use std::fmt;

/// What went wrong reading or writing a thermogram file.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The file's magic number doesn't match any supported format.
    UnrecognizedFormat([u8; 4]),
    /// The file matched a supported format but could not be decoded.
    Decode(String),
    /// The output image could not be encoded or written.
    Encode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "i/o error: {e}"),
            Error::UnrecognizedFormat(magic) => {
                write!(f, "unrecognized file format (magic number {magic:02x?})")
            }
            Error::Decode(msg) => write!(f, "could not decode file: {msg}"),
            Error::Encode(msg) => write!(f, "could not write file: {msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
