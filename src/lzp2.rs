// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of an LZP compressor, combining the approach
//! from lzp1.rs and a following adaptive Huffman coder.

use std::io::{Read, Write, Bytes};
use std::io;

use huff::adaptive as nested;

use error::Error;

const WINDOW_BITS: usize = 12;
const LENGTH_BITS: usize = 8;

const MIN_MATCH_LEN: usize = 1;
const MAX_MATCH_LEN: usize = ((1 << LENGTH_BITS) - 1) + MIN_MATCH_LEN;

const LOOK_AHEAD_BYTES: usize = MAX_MATCH_LEN;

const WINDOW_SIZE: usize = 1 << WINDOW_BITS;

const HASHTAB_SIZE: usize = 1 << 10;

const MAX_CONTEXT: usize = 3;

/// Writer for LZSS compressed streams.
pub struct Writer<W> {
    inner:  nested::Writer<W>,
    window: [u8; WINDOW_SIZE],
    hashtab: [usize; HASHTAB_SIZE],
    position: usize,
    look_ahead_bytes: usize,
    context: [u8; MAX_CONTEXT],
    out_flags: u8,
    out_count: usize,
    out_data:  [u8; 1 + 8*2],
    out_len:   usize,
}

#[inline(always)]
fn mod_window(x: usize) -> usize {
    x % WINDOW_SIZE
}

impl<W: Write> Writer<W> {
    /// Create a new LZSS writer that wraps the given Writer.
    pub fn new(inner: W) -> Writer<W>{
        Writer {
            inner:  nested::Writer::new(inner),
            window: [0; WINDOW_SIZE],
            hashtab: [0; HASHTAB_SIZE],
            position: 0,
            look_ahead_bytes: 0,
            context: [0; MAX_CONTEXT],
            out_flags: 0,
            out_count: 0,
            out_data: [0; 1 + 8*2],
            out_len:  1,
        }
    }

    /// Output all buffered match/length pairs and literals.
    fn emit_flush(&mut self) -> io::Result<()> {
        if self.out_count > 0 {
            if self.out_count < 8 {
                self.out_flags <<= 8 - self.out_count;
            }
            self.out_data[0] = self.out_flags;
            try!(self.inner.write_all(&self.out_data[..self.out_len]));
            
            self.out_flags = 0;
            self.out_count = 0;
            self.out_len = 1;
        }
        Ok(())
    }

    /// Emit the literal byte `lit`.
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

    /// Emit a match, which just contains the match length.
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

    fn update_context(&mut self) {
        let start =
            if self.position >= MAX_CONTEXT {
                self.position - MAX_CONTEXT
            } else {
                WINDOW_SIZE - (MAX_CONTEXT - self.position)
            };
        for i in 0..MAX_CONTEXT {
            self.context[i] = self.window[mod_window(start + i)];
        }
    }

    /// Calculate a hash of the next 3 bytes in the look-ahead buffer.
    /// This hash is used to look up earlier occurences of the data we
    /// are looking at.  Because hash table entries are overwritten
    /// blindly, we have to validate whatever we take out of the table
    /// when calculating the match length.
    fn hash_context(&self) -> usize {
        let mut h = 0;
        for b in self.context.iter() {
            h = (h << 8) + *b as usize;
        }
        h % HASHTAB_SIZE
    }

    fn find_longest_match(&self, match_pos: usize, search_pos: usize) -> usize {
        if self.look_ahead_bytes > MIN_MATCH_LEN && match_pos != search_pos {
            let mut match_len = 0;
            for i in 0..::std::cmp::min(self.look_ahead_bytes, MAX_MATCH_LEN) {
                if self.window[mod_window(match_pos + i)] != self.window[mod_window(search_pos + i)] {
                    break;
                }
                match_len += 1;
            }
            match_len
        } else {
            0
        }
    }

    fn process(&mut self) -> io::Result<()> {
        let search_pos = self.position;

        let hsh = self.hash_context();
        let match_pos = self.hashtab[hsh];
        
        let ofs =
            if match_pos < self.position {
                self.position - match_pos
            } else {
                self.position + (WINDOW_SIZE - match_pos)
            };
        
        let match_len = self.find_longest_match(match_pos, search_pos);
//        println!("pos: {}, context: {:?}, hash: {}, match_pos: {}, match_len: {}",
//                 self.position, &self.context[..], hsh, match_pos, match_len);
        
        if ofs < WINDOW_SIZE - MAX_MATCH_LEN && match_len >= MIN_MATCH_LEN {
            assert!(ofs != 0);
            assert!((match_len - MIN_MATCH_LEN) < 256);
            
            try!(self.emit_match((match_len - MIN_MATCH_LEN) as u8));
            
            self.position = mod_window(self.position + match_len);
            self.look_ahead_bytes -= match_len;
            self.hashtab[hsh] = search_pos;
        } else {
            let lit = self.window[self.position];
            try!(self.emit_lit(lit));

            self.position = mod_window(self.position + 1);
            self.look_ahead_bytes -= 1;
        }
        self.update_context();
        Ok(())
    }

    /// Move the wrapped writer out of the LZSS writer.
    pub fn to_inner(self) -> W {
        self.inner.into_inner()
    }
}

impl<W: Write> Write for Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < buf.len() {
            while written < buf.len() && self.look_ahead_bytes < LOOK_AHEAD_BYTES {
                self.window[mod_window(self.position + self.look_ahead_bytes)] =
                    buf[written];
                self.look_ahead_bytes += 1;
                written += 1;
            }
            if self.look_ahead_bytes == LOOK_AHEAD_BYTES {
                try!(self.process());
            }
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        while self.look_ahead_bytes > 0 {
            try!(self.process());
        }
        try!(self.emit_flush());
        self.inner.flush()
    }
}

/// Reader for LZSS compressed streams.
pub struct Reader<R> {
    inner: Bytes<nested::Reader<R>>,
    window: [u8; WINDOW_SIZE],
    hashtab: [usize; HASHTAB_SIZE],
    context: [u8; MAX_CONTEXT],
    position: usize,
    returned: usize,
    eof: bool,
}

impl<R: Read> Reader<R> {
    /// Create a new LZSS reader that wraps another reader.
    pub fn new(inner: R) -> Reader<R> {
        Reader {
            inner: nested::Reader::new(inner).bytes(),
            window: [0; WINDOW_SIZE],
            hashtab: [0; HASHTAB_SIZE],
            context: [0; MAX_CONTEXT],
            position: 0,
            returned: 0,
            eof: false,
        }
    }

    fn update_context(&mut self) {
        let start =
            if (self.position) >= MAX_CONTEXT {
                (self.position) - MAX_CONTEXT
            } else {
                WINDOW_SIZE - (MAX_CONTEXT - self.position)
            };
        for i in 0..MAX_CONTEXT {
            self.context[i] = self.window[mod_window(start + i)];
        }
    }
    
    fn hash_context(&self) -> usize {
        let mut h = 0;
        for b in self.context.iter() {
            h = (h << 8) + *b as usize;
        }
        h % HASHTAB_SIZE
    }

    /// Copy all decompressed data from the window to the output
    /// buffer.
    fn copy_out(&mut self, output: &mut [u8], written: &mut usize) {
        while *written < output.len() && self.returned != self.position {
            output[*written] = self.window[self.returned];
            *written += 1;
            self.returned = mod_window(self.returned + 1);
        }
    }

    /// Process a group of 8 literals or match/length pairs.  The
    /// given token is contains the flag bits.
    fn process_group(&mut self, token: u8) -> io::Result<()> {
        for i in 0..8 {
            if token & 0x80 >> i == 0 {
                // Zero bit indicates a match/length pair. Decode the
                // next two bytes into a 4-bit length and a 12-bit
                // offset.
                let mblen = self.inner.next();
                match mblen {
                    None => {
                        self.eof = true;
                        return Ok(());
                    }
                    Some(alen) => {
                        let len = try!(alen) as usize + MIN_MATCH_LEN;
                        let hsh = self.hash_context();
                        let pos = self.hashtab[hsh];
//                        println!("pos: {}, context: {:?}, hash: {}, match_pos: {}, match_len: {}",
//                                 self.position, &self.context[..], hsh, pos, len);
                        for i in 0..len {
                            self.window[mod_window(self.position + i)] =
                                self.window[mod_window(pos + i)];
                        }
                        self.hashtab[hsh] = self.position;
                        self.position = mod_window(self.position + len);
                    },
                }
            } else {
                // A 1-bit in the token indicates a literal.  Just
                // take the next byte from the input and add it to the
                // window.
                if let Some(lit) = self.inner.next() {
                    let lit = try!(lit);
                    self.window[self.position] = lit;
                    self.position = mod_window(self.position + 1);
                } else {
                    // EOF here means corrupted input, because the
                    // encoder does not put a 1-bit into the token
                    // when the stream ends.
                    self.eof = true;
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                              "cannot read literal"));
                }
            }
            self.update_context();
        }
        Ok(())
    }

    /// Process as much from the underlying input as necessary to fill
    /// the output buffer.  When more data than necessary is
    /// decompressed, it stays in the window for later processing.
    fn process(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let mut written = 0;
        
        // Copy out data that already was decompressed but did not fit
        // into output last time.
        self.copy_out(output, &mut written);
        
        'outer:
        while written < output.len() {
            if let Some(token) = self.inner.next() {
                let token = try!(token);
                try!(self.process_group(token));
                self.copy_out(output, &mut written);
            } else {
                self.eof = true;
                break;
            }
        }
        Ok(written)
    }
}

impl<R: Read> Read for Reader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.eof {
            Ok(0)
        } else {
            self.process(output)
        }
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
        cmp_test(&[], &[0]);
    }

    #[test]
    fn compress_a() {
        cmp_test(b"a", &[192, 12, 40]);
    }

    #[test]
    fn compress_aaa() {
        cmp_test(b"aaaaaaaaa", &[192, 12, 32, 58]);
    }

    #[test]
    fn compress_abc() {
        cmp_test(b"abcdefgabcdefgabcabcabcdefg",
                 &[255, 12, 35, 22, 199, 178, 108, 181, 154, 179, 208, 154, 121, 64, 167, 1, 34, 0]);
    }

    fn decmp_test(compressed: &[u8], expected_output: &[u8]) {
        let mut cr = Reader::new(Cursor::new(compressed));

        let mut decompressed = Vec::new();
        let nread = cr.read_to_end(&mut decompressed).unwrap();

//        println!("{:?} {:?}", String::from_utf8_lossy(expected_output), String::from_utf8_lossy(&decompressed));
        assert_eq!(expected_output.len(), nread);
        assert_eq!(&expected_output[..], &decompressed[..]);
    }

    #[test]
    fn decompress_empty() {
        decmp_test(&[0], &[]);
    }

    #[test]
    fn decompress_a() {
        decmp_test(&[192, 12, 40], b"a");
    }

    #[test]
    fn decompress_aaa() {
        decmp_test(&[192, 12, 32, 58], b"aaaaaaaaa");
    }

    #[test]
    fn decompress_abc() {
        decmp_test(
            &[255, 12, 35, 22, 199, 178, 108, 181, 154, 179, 208, 154, 121, 64, 167, 1, 34, 0],
//            &[254, 97, 98, 99, 100, 101, 102, 103, 128,
//              7, 0, 16, 10, 16, 3, 32, 20],
            b"abcdefgabcdefgabcabcabcdefg");
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
        let input = include_bytes!("lzp1.rs");
        roundtrip(input);
    }
}
