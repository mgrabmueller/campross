// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of an LZP compressor.

use std::io::{Read, Write};
use std::io;

pub const MAX_BLOCK_SIZE: usize = 1024 * 64;
pub const MIN_BLOCK_SIZE: usize = 1024 * 16;

pub const LENGTH_BITS: usize = 8;
pub const MAX_MATCH_LEN: usize = 1 << LENGTH_BITS;

use error::Error;

pub struct Writer<W> {
    inner:  W,
    block: Vec<u8>,
    out_flags: u8,
    out_count: usize,
    out_data:  [u8; 1 + 8],
    out_len:   usize,
}

impl<W: Write> Writer<W> {
    /// Create a new LZP writer that wraps the given Writer.
    pub fn new(inner: W) -> Writer<W>{
        Writer {
            inner:  inner,
            block: Vec::with_capacity(MIN_BLOCK_SIZE),
            out_flags: 0,
            out_count: 0,
            out_data: [0; 1 + 8],
            out_len:  1,
        }
    }

    fn emit_flush(&mut self) -> io::Result<()> {
        if self.out_count > 0 {
            if self.out_count < 8 {
                self.out_flags <<= 8 - self.out_count;
            }
            self.out_data[0] = self.out_flags;
            let _nread = try!(self.inner.write(&self.out_data[..self.out_len]));
            
            self.out_flags = 0;
            self.out_count = 0;
            self.out_len = 1;
        }
        Ok(())
    }
    
    fn emit_lit(&mut self, lit: u8) -> io::Result<()> {
        if self.out_count == 8 {
            try!(self.emit_flush());
        }
        self.out_count += 1;
        self.out_flags = (self.out_flags << 1) | 1;
        self.out_data[self.out_len] = lit;
        self.out_len += 1;
        Ok(())
    }

    pub fn emit_match(&mut self, len: u8) -> io::Result<()> {
        if self.out_count == 8 {
            try!(self.emit_flush());
        }
        self.out_count += 1;
        self.out_flags = self.out_flags << 1;
        self.out_data[self.out_len] = len;
        self.out_len += 1;
        Ok(())
    }

    fn process_block(&mut self, final_block: bool) -> io::Result<()> {
        if self.block.len() >= MIN_BLOCK_SIZE || final_block {
            let bsz = self.block.len() as u32;
            let sz = [(bsz & 0xff) as u8,
                      ((bsz >> 8) & 0xff) as u8,
                      ((bsz >> 16) & 0xff) as u8,
                      ((bsz >> 24) & 0xff) as u8];
            try!(self.inner.write_all(&sz[..]));
            
            let mut position = 0;
            while position < self.block.len() {
                let lit = self.block[position];
                try!(self.emit_lit(lit));
                position += 1;
            }

            self.block.truncate(0);
            try!(self.emit_flush());
        }
        Ok(())
    }
    
    /// Move the wrapped writer out of the LZP writer.
    pub fn to_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for Writer<W> {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let mut written = 0;
        while buf.len() > 0 {
            let sz = ::std::cmp::min(MAX_BLOCK_SIZE - self.block.len(), buf.len());
            let src = &buf[0..sz];
            buf = &buf[sz..];
            self.block.extend_from_slice(src);
            written += sz;
            try!(self.process_block(false));
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        try!(self.process_block(true));
        self.inner.flush()
    }
}

pub struct Reader<R> {
    inner: R,
    block: Vec<u8>,
    in_block: bool,
    position: usize,
    block_length: usize,
    returned: usize,
    eof: bool,
}

impl<R: Read> Reader<R> {
    /// Create a new LZP reader that wraps another reader.
    pub fn new(inner: R) -> Reader<R> {
        Reader {
            inner: inner,
            block: Vec::with_capacity(MIN_BLOCK_SIZE),
            in_block: false,
            position: 0,
            block_length: 0,
            returned: 0,
            eof: false,
        }
    }
    
    fn getc(&mut self) -> io::Result<Option<u8>> {
        let mut buf = [0u8];
        let n = try!(self.inner.read(&mut buf));
        if n == 1 {
            Ok(Some(buf[0]))
        } else {
            Ok(None)
        }
    }

    fn copy_out(&mut self, output: &mut[u8], written: &mut usize) {
        while *written < output.len() && self.returned < self.position {
            output[*written] = self.block[self.returned];
            *written += 1;
            self.returned += 1;
        }
    }
    
    fn process(&mut self, output: &mut[u8]) -> io::Result<usize> {
        if self.eof {
            return Ok(0);
        }
        
        let mut written = 0;
        while written < output.len() {

            if !self.in_block {
                let b1 = try!(self.getc());
                let b2 = try!(self.getc());
                let b3 = try!(self.getc());
                let b4 = try!(self.getc());
                let block_length =
                    match (b1, b2, b3, b4) {
                        (None, _, _, _) => {
                            self.eof = true;
                            self.copy_out(output, &mut written);
                            return Ok(written);
                        },
                        (Some(c1), Some(c2), Some(c3), Some(c4)) =>
                            ((c1 as u64) +
                             ((c2 as u64) << 8) +
                             ((c3 as u64) << 16) +
                             ((c4 as u64) << 24)) as usize,
                        _ => {
                            self.eof = true;
                            return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                                      "cannot read block size"));
                        },
                    };
                self.block_length = block_length;
                self.in_block = true;
                self.block.truncate(0);
                self.position = 0;
                self.returned = 0;
            }
            let mut token;
            if let Some(tok) = try!(self.getc()) {
                token = tok;
            } else {
                self.eof = true;
                break;
            }
            for _ in 0..8 {
                if token & 0x80 != 0 {
                    if let Some(lit) = try!(self.getc()) {
                        self.block.push(lit);
                        self.position += 1;
                    } else {
                        self.eof = true;
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                                  "cannot read literal"));
                    }
                } else {
                    if let Some(_) = try!(self.getc()) {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                                  "did not expect match"));
                    } else {
                        self.eof = true;
                        self.copy_out(output, &mut written);
                        return Ok(written);
                    }
                }
                token <<= 1;
            }
            self.copy_out(output, &mut written);
            if self.position == self.block_length {
                self.in_block = false;
            }
        }
        Ok(written)
    }
}

impl<R: Read> Read for Reader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.process(output)
    }
}

pub fn compress<R: Read, W: Write>(mut input: R, output: W) -> Result<W, Error> {
    let mut cw = Writer::new(output);
    try!(io::copy(&mut input, &mut cw));
    try!(cw.flush());
    Ok(cw.to_inner())
}

pub fn decompress<R: Read, W: Write>(input: R, mut output: W) -> Result<W, Error> {
    let mut cr = Reader::new(input);
    try!(io::copy(&mut cr, &mut output));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use ::std::io::Cursor;

    use super::{Writer, Reader};
    use ::std::io::{Read, Write};

    fn cmp_test(input: &[u8], expected_output: &[u8]) {
        let mut cw = Writer::new(vec![]);
        
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        
        assert_eq!(&expected_output[..], &compressed[..]);
    }
    
    #[test]
    fn compress_empty() {
        cmp_test(b"", &[0, 0, 0, 0]);
    }

    #[test]
    fn compress_a() {
        cmp_test(b"a", &[1, 0, 0, 0, 128, b'a']);
    }

    #[test]
    fn compress_aaa() {
        cmp_test(b"aaaaaaaaa", &[9, 0, 0, 0, 255,
                                 b'a', b'a', b'a', b'a', b'a', b'a', b'a',
                                 b'a', 128, b'a']);
    }

    fn decmp_test(compressed: &[u8], expected_output: &[u8]) {
        let mut cr = Reader::new(Cursor::new(compressed));
        
        let mut decompressed = Vec::new();
        let nread = cr.read_to_end(&mut decompressed).unwrap();
        
        assert_eq!(expected_output.len(), nread);
        assert_eq!(&expected_output[..], &decompressed[..]);
    }
    
    #[test]
    fn decompress_empty() {
        decmp_test(&[0, 0, 0, 0], &[]);
    }

    #[test]
    fn decompress_a() {
        decmp_test(&[1, 0, 0, 0, 128, b'a'], b"a");
    }

    #[test]
    fn decompress_aaa() {
        decmp_test(&[9, 0, 0, 0, 255, b'a', b'a', b'a', b'a', b'a', b'a', b'a',
                     b'a', 128, b'a'], b"aaaaaaaaa");
    }

    fn roundtrip(input: &[u8]) {
        let mut cw = Writer::new(vec![]);
        cw.write_all(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();

        let mut cr = Reader::new(Cursor::new(compressed));
        let mut decompressed = Vec::new();
        let nread = cr.read_to_end(&mut decompressed).unwrap();

        assert_eq!(input.len(), nread);
        assert_eq!(&input[..], &decompressed[..]);
    }
    
    #[test]
    fn compress_decompress() {
        let input = include_bytes!("lzp.rs");
        roundtrip(input);
    }
}
