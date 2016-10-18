use std::io;
use std::error;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    UnexpectedEof,
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::UnexpectedEof => write!(f, "unexpected end of file"),
            Error::Io(ref err) => err.fmt(f),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::UnexpectedEof => "unexpected end of file",
            Error::Io(ref err) => err.description(),
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

