// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of a Huffman encoder.

use std::io::{Read, Write};
use std::io;
use error::Error;
use bitfile::{BitWriter, BitReader};


pub struct CompressWriter<W> {
    inner: W,
}

impl<W: Write> CompressWriter<W> {
    pub fn new(inner: W) -> CompressWriter<W>{
        CompressWriter {
            inner: inner,
        }
    }

    fn process(&mut self, input: &[u8]) -> io::Result<usize> {
        Ok(0)
    }

    pub fn to_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for CompressWriter<W> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.process(input)
    }

    /// Flush the compression writer.  This will cause all not-yet
    /// written data to be compressed and written to the underlying
    /// Writer, which is also flushed.
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub struct DecompressReader<R> {
    inner: R,
}

impl<R: Read> DecompressReader<R> {
    pub fn new(inner: R) -> DecompressReader<R> {
        DecompressReader {
            inner: inner,
        }
    }

    fn process(&mut self, output: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
}

impl<R: Read> Read for DecompressReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.process(output)
    }
}

pub fn compress<R: Read, W: Write>(mut input: R, output: W) -> Result<W, Error> {
    let mut cw = CompressWriter::new(output);
    try!(io::copy(&mut input, &mut cw));
    try!(cw.flush());
    Ok(cw.to_inner())
}

pub fn decompress<R: Read, W: Write>(input: R, mut output: W) -> Result<W, Error> {
    let mut cr = DecompressReader::new(input);
    try!(io::copy(&mut cr, &mut output));
    Ok(output)
}


#[cfg(test)]
mod test {
    use ::std::io::Cursor;
    use super::{compress}; //, decompress};
    
    #[test]
    fn compress_empty() {
        let input = [];
        let compressed = compress(Cursor::new(&input[..]), vec![]).unwrap();
        let expected = [0u8];
        println!("compressed: {:?}", compressed);
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_a() {
        let input = b"a";
        let compressed = compress(Cursor::new(&input[..]), vec![]).unwrap();
        let expected = [0u8];
        println!("compressed: {:?}", compressed);
        assert_eq!(&expected[..], &compressed[..]);
    }

    // #[test]
    // fn compress_decompress() {
    //     let input = include_bytes!("huff.rs");
    //     let compressed = compress(Cursor::new(&input[..]), vec![]).unwrap();
    //     let decompressed = decompress(Cursor::new(&compressed[..]), vec![]).unwrap();
    //     assert_eq!(&input[..], &decompressed[..]);
    // }
}
