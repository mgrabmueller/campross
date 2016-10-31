// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of an LZ77 compressor.

use std::io::{Read, Write};
use std::io;

use error::Error;

const WINDOW_BITS: usize = 12;
const LENGTH_BITS: usize = 4;

const MIN_MATCH_LEN: usize = 4;
const MAX_MATCH_LEN: usize = ((1 << LENGTH_BITS) - 1) + MIN_MATCH_LEN - 1;

const LOOK_AHEAD_BYTES: usize = MAX_MATCH_LEN + 1;

const WINDOW_SIZE: usize = 1 << WINDOW_BITS;

const HASHTAB_SIZE: usize = 1 << 10;

pub struct Writer<W> {
    inner:  W,
    window: [u8; WINDOW_SIZE],
    hashtab: [usize; HASHTAB_SIZE],
    position: usize,
    look_ahead_bytes: usize,

    biggest_len: usize,
    biggest_ofs: usize,
}

#[inline(always)]
fn mod_window(x: usize) -> usize {
    x % WINDOW_SIZE
}

impl<W: Write> Writer<W> {
    /// Create a new LZP writer that wraps the given Writer.
    pub fn new(inner: W) -> Writer<W>{
        Writer {
            inner:  inner,
            window: [0; WINDOW_SIZE],
            hashtab: [0; HASHTAB_SIZE],
            position: 0,
            look_ahead_bytes: 0,
            biggest_len: 0,
            biggest_ofs: 0,
        }
    }

    fn hash_at(&self, pos: usize) -> usize {
        // This might go over the data actually in the window, but as
        // long as the compressor and decompressor maintain the same
        // window contents, it should not matter.
        let h1 = self.window[pos] as usize;
        let h2 = self.window[mod_window(pos + 1)] as usize;
        let h3 = self.window[mod_window(pos + 2)] as usize;

        let h = (h1 >> 5) ^ ((h2 << 8) + h3);

        h % HASHTAB_SIZE
    }

    fn find_longest_match(&self, match_pos: usize, search_pos: usize) -> usize {
        if self.look_ahead_bytes > MIN_MATCH_LEN - 1 && match_pos != search_pos {
            let mut match_len = 0;
            for i in 0..::std::cmp::min(self.look_ahead_bytes - 1, MAX_MATCH_LEN) {
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
        
        let hsh = self.hash_at(search_pos);
        let match_pos = self.hashtab[hsh];
        
        let ofs =
            if match_pos < self.position {
                self.position - match_pos
            } else {
                self.position + (WINDOW_SIZE - match_pos)
            };
        
        let match_len = self.find_longest_match(match_pos, search_pos);
        
        if ofs < WINDOW_SIZE - MAX_MATCH_LEN && match_len >= MIN_MATCH_LEN {
            let follow = self.window[mod_window(self.position + match_len)];

            if match_len > self.biggest_len {
                self.biggest_len = match_len;
            }
            if ofs > self.biggest_ofs {
                self.biggest_ofs = ofs;
            }
            assert!(ofs != 0);
            assert!(ofs < WINDOW_SIZE - MAX_MATCH_LEN);
            assert!((match_len - MIN_MATCH_LEN) < 16);
            
            let m1 = (((match_len - MIN_MATCH_LEN) as u8) << 4) | (((ofs >> 8) as u8) & 0x0f);
            let m2 = (ofs & 0xff) as u8;
            
            try!(self.inner.write_all(&[m1, m2, follow]));
            
            self.position = mod_window(self.position + match_len + 1);
            self.look_ahead_bytes -= match_len + 1;
        } else {
            try!(self.inner.write_all(&[0, 0, self.window[self.position]]));
            self.position = mod_window(self.position + 1);
            self.look_ahead_bytes -= 1;
        }
        self.hashtab[hsh] = search_pos;
        Ok(())
    }

    /// Move the wrapped writer out of the LZP writer.
    pub fn to_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for Writer<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < buf.len() {
            while written < buf.len() && self.look_ahead_bytes < LOOK_AHEAD_BYTES {
                self.window[mod_window(self.position + self.look_ahead_bytes)] = buf[written];
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
        println!("biggest len={}, ofs={}", self.biggest_len, self.biggest_ofs);
        self.inner.flush()
    }
}

pub struct Reader<R> {
    inner: R,
    window: [u8; WINDOW_SIZE],
    position: usize,
    returned: usize,
}

impl<R: Read> Reader<R> {
    /// Create a new LZP reader that wraps another reader.
    pub fn new(inner: R) -> Reader<R> {
        Reader {
            inner: inner,
            window: [0; WINDOW_SIZE],
            position: 0,
            returned: 0,
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

    fn copy_out(&mut self, output: &mut [u8], written: &mut usize) {
        while *written < output.len() && self.returned != self.position {
            output[*written] = self.window[self.returned];
            *written += 1;
            self.returned = mod_window(self.returned + 1);
        }
    }
    
    fn process(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < output.len() {
            if let Some(m1) = try!(self.getc()) {
                let mbm2 = try!(self.getc());
                let mblit = try!(self.getc());
                match (mbm2, mblit) {
                    (Some(m2), Some(lit)) => {
                        let len = ((m1 >> 4) as usize) + MIN_MATCH_LEN;
                        let ofs = (((m1 as usize) & 0xf) << 8) | (m2 as usize);
                        if ofs > 0 {
                            let pos =
                                if ofs < self.position {
                                    self.position - ofs
                                } else {
                                    WINDOW_SIZE - (ofs - self.position)
                                };
                            for i in 0..len {
                                self.window[mod_window(self.position + i)] = self.window[mod_window(pos + i)];
                            }
                            self.window[mod_window(self.position + len)] = lit;
                            self.position = mod_window(self.position + len + 1);
                        } else {
                            self.window[mod_window(self.position)] = lit;
                            self.position = mod_window(self.position + 1);
                        }

                        self.copy_out(output, &mut written);
                    },
                    _ => {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "cannot read match/lit pair"));
                    },
                }
            } else {
                self.copy_out(output, &mut written);
                break;
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
        cmp_test(b"", &[]);
    }

    #[test]
    fn compress_a() {
        cmp_test(b"a", &[0, 0, b'a']);
    }

    #[test]
    fn compress_aaa() {
        cmp_test(b"aaaaaaaaa", &[0, 0, 97, 48, 1, 97]);
    }

    #[test]
    fn compress_abc() {
        cmp_test(b"abcdefgabcdefgabcabcabcdefg",
                 &[0, 0, 97, 0, 0, 98, 0, 0, 99, 0, 0, 100, 0, 0, 101, 0,
                   0, 102, 0, 0, 103, 96, 7, 97, 0, 0, 98, 0, 0, 99, 32, 13,
                   103]);
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
        decmp_test(&[], &[]);
    }

    #[test]
    fn decompress_a() {
        decmp_test(&[0, 0, b'a'], b"a");
    }

    #[test]
    fn decompress_aaa() {
        decmp_test(&[0, 0, 97, 48, 1, 97], b"aaaaaaaaa");
    }

    #[test]
    fn decompress_abc() {
        decmp_test(
            &[0, 0, 97, 0, 0, 98, 0, 0, 99, 0, 0, 100, 0, 0, 101, 0,
              0, 102, 0, 0, 103, 96, 7, 97, 0, 0, 98, 0, 0, 99, 32, 13,
              103],
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
        let input = include_bytes!("lz77.rs");
        roundtrip(input);
    }
}
