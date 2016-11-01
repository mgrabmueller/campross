// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of an LZ77 compressor.

use std::io::{Read, Write};
use std::io;

use error::Error;
use window::SlidingWindow;

const LENGTH_BITS: usize = 4;
const OFFSET_BITS: usize = 12;

const MIN_MATCH: usize = 2;
const MAX_LENGTH: usize = ((1 << LENGTH_BITS) - 1) + MIN_MATCH;

const WINDOW_SIZE: usize = 1 << OFFSET_BITS;
const LOOK_AHEAD_SIZE: usize = MAX_LENGTH;

const HASHTAB_SIZE: usize = 1 << 10;

// Marks unused hash table and hash position slots.
const UNUSED: usize = !0;

// Max. 2 bytes for pos/len * 8 + token.
const MAX_RUN_LENGTH: usize = 2 * 8 + 1;

pub struct CompressWriter<W> {
    inner:    W,
    window:   SlidingWindow,
    hashtab:  [usize; HASHTAB_SIZE],

    emit_token: u8,
    emit_cnt: usize,
    emit_data: [u8; MAX_RUN_LENGTH],
    emit_len: usize,
}

impl<W: Write> CompressWriter<W> {
    pub fn new(inner: W) -> CompressWriter<W>{
        CompressWriter {
            inner:      inner,
            window:     SlidingWindow::new(WINDOW_SIZE, LOOK_AHEAD_SIZE),
            hashtab:    [UNUSED; HASHTAB_SIZE],
            emit_token: 0,
            emit_cnt:   0,
            emit_data:  [0; MAX_RUN_LENGTH],
            emit_len:   0,
        }
    }

    fn emit_flush(&mut self) -> io::Result<()> {
        if self.emit_cnt < 8 && self.emit_cnt > 0 {
            self.emit_token <<= 8 - self.emit_cnt;
        }
        self.emit_data[0] = self.emit_token;
        try!(self.inner.write(&self.emit_data[0..self.emit_len + 1]));
        self.emit_token = 0;
        self.emit_cnt = 0;
        self.emit_len = 0;
        Ok(())
    }
    
    fn emit_literal(&mut self, l: u8) -> io::Result<()> {
        if self.emit_cnt == 8 {
            try!(self.emit_flush());
        }
        self.emit_token = (self.emit_token << 1) | 1;
        self.emit_data[self.emit_len + 1] = l;
        self.emit_cnt += 1;
        self.emit_len += 1;
        Ok(())
    }

    fn emit_match(&mut self, ofs: usize, len: usize) -> io::Result<()> {
        assert!(ofs > 0);
        assert!(ofs < WINDOW_SIZE);
        assert!(len >= MIN_MATCH);
        assert!(len - MIN_MATCH <= 15);
        
        if self.emit_cnt == 8 {
            try!(self.emit_flush());
        }
        let lp1: u8 = ((((len - MIN_MATCH) as u8) & 0x0f) << 4) | (ofs as u8) & 0x0f;
        let p2: u8 = (ofs >> 4) as u8;
        self.emit_token = self.emit_token << 1;
        self.emit_data[self.emit_len + 1] = lp1;
        self.emit_data[self.emit_len + 2] = p2;
        self.emit_cnt += 1;
        self.emit_len += 2;
        Ok(())
    }
    
    fn process(&mut self, flush: bool) -> io::Result<()> {
        let headroom = if flush { 0 } else { LOOK_AHEAD_SIZE };
        while self.window.position + headroom < self.window.limit {

            let h = self.calc_hash(self.window.position);
            let search_pos = self.hashtab[h];
            let mut match_len = 0;

            if search_pos != UNUSED && search_pos < self.window.position
                && self.window.position - search_pos < WINDOW_SIZE {
                for i in 0..MAX_LENGTH {
                    if self.window.position + i >= self.window.limit {
                        break;
                    }
                    if self.window.window[search_pos + i] !=
                        self.window.window[self.window.position + i] {
                        break;
                    }
                    match_len += 1;
                }
            }
            let replace =
                if match_len >= MIN_MATCH {
                    let ofs = self.window.position - search_pos;
                    try!(self.emit_match(ofs, match_len));
 
                    match_len
                } else {
                    let lit = self.window.window[self.window.position];
                    try!(self.emit_literal(lit));
                    1
                };
            for i in 0..replace {
                let pos = self.window.position;
                self.hash(pos + i);
                if self.window.advance() {
                    self.slide_hashes();
                }
            }
        }
        Ok(())
    }

    fn calc_hash(&self, i: usize) -> usize {
        let mut hash: usize = 0;
        for x in i .. ::std::cmp::min(i + 3, self.window.limit) {
            hash = (hash << 8) | self.window.window[x] as usize;
        }
        hash = ((hash >> 5) ^ hash) & (HASHTAB_SIZE - 1);
        hash
    }
    
    fn hash(&mut self, i: usize) {
        let hash = self.calc_hash(i);
        self.hashtab[hash] = i;
    }

    fn slide_hashes(&mut self) {
        for e in self.hashtab.iter_mut() {
            if *e > WINDOW_SIZE {
                *e -= WINDOW_SIZE;
            } else {
                *e = UNUSED;
            }
        }
    }
    
    pub fn to_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for CompressWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < buf.len() {
            let space = self.window.free_space();
            let amount = ::std::cmp::min(space, buf.len() - written);
            if amount == 0 {
                break;
            }
            for t in 0..amount {
                self.window.push(buf[written + t]);
            }
            written += amount;

            try!(self.process(false));
        }
        Ok(written)
    }

    /// Flush the compression writer.  This will cause all not-yet
    /// written data to be compressed and written to the underlying
    /// Writer, which is also flushed.
    fn flush(&mut self) -> io::Result<()> {
        // Process remaining window contents. `true` indicates to go
        // to the end, including the look-ahead buffer.
        try!(self.process(true));
        try!(self.emit_flush());
        self.inner.flush()
    }
}

pub struct DecompressReader<R> {
    inner:     R,
    window:    SlidingWindow,
    start:     usize,
}

impl<R: Read> DecompressReader<R> {
    pub fn new(inner: R) -> DecompressReader<R> {
        DecompressReader {
            inner:     inner,
            window:    SlidingWindow::new(WINDOW_SIZE, LOOK_AHEAD_SIZE),
            start:     0,
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
        // Copy as much decoded as possible from the decoding
        // window to the output array.
        while self.start < self.window.position && *written < output.len() {
            output[*written] = self.window.window[self.start];
            *written += 1;
            self.start += 1;
        }
    }
    
    fn process(&mut self, output: &mut[u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < output.len() {

            let token;
            if let Some(tok) = try!(self.getc()) {
                token = tok;
            } else {
                break;
            }
            for i in 0..8 {
                if token & 1 << (7 - i) != 0 {
                    if let Some(lit) = try!(self.getc()) {
                        self.window.push(lit);
                        if self.window.advance() {
                            self.start -= WINDOW_SIZE;
                        }
                    } else {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                                  ""));
                    }
                } else {
                    if let Some(b1) = try!(self.getc()) {
                        if let Some(b2) = try!(self.getc()) {
                            let w1 = b1 as usize;
                            let w2 = b2 as usize;
                    
                            let len = (w1 >> 4) + MIN_MATCH;
                            let ofs = (w1 & 0x0f) | (w2 << 4);

                            for i in 0..len {
                                let c = self.window.window[self.window.position
                                                           - ofs + i];
                                self.window.push(c);
                            }
                            for _ in 0..len {
                                if self.window.advance() {
                                    self.start -= WINDOW_SIZE;
                                }
                            }                            
                        } else {
                            return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                                      ""));
                        }
                    } else {
                        break;
                    }
                }
                self.copy_out(output, &mut written);
                            
            }
        }
        Ok(written)
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
mod tests {
    use ::std::io::Cursor;

    use super::{CompressWriter, DecompressReader};
    use ::std::io::{Read, Write};
    
    #[test]
    fn compress_empty() {
        let mut cw = CompressWriter::new(vec![]);
        let input = b"";
        let expected = [0u8];
        
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_a() {
        let mut cw = CompressWriter::new(vec![]);
        let input = b"a";
        let expected = [128u8, 97];
        
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_aaa() {
        let mut cw = CompressWriter::new(vec![]);
        let input = b"aaaaaaaaa";
        let expected = [128u8, 97, 81, 0];
        
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn decompress_empty() {
        let compressed = [];
        let mut cr = DecompressReader::new(Cursor::new(compressed));
        let expected: [u8; 0] = [];
        
        let mut decompressed = Vec::new();
        let nread = cr.read_to_end(&mut decompressed).unwrap();
        
        assert_eq!(0, nread);
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_a() {
        let compressed = [128u8, 97];
        let mut cr = DecompressReader::new(Cursor::new(compressed));
        let expected = b"a";
        
        let mut decompressed = Vec::new();
        let nread = cr.read_to_end(&mut decompressed).unwrap();
        
        assert_eq!(1, nread);
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_aaa() {
        let compressed = [128u8, 97, 81, 0];
        let mut cr = DecompressReader::new(Cursor::new(compressed));
        let expected = b"aaaaaaaaa";
        
        let mut decompressed = Vec::new();
        let nread = cr.read_to_end(&mut decompressed).unwrap();
        
        assert_eq!(9, nread);
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_decompress() {
        let input = include_bytes!("lzmg2.rs");
        let mut cw = CompressWriter::new(vec![]);
        cw.write_all(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();

        let mut cr = DecompressReader::new(Cursor::new(compressed));
        let mut decompressed = Vec::new();
        let nread = cr.read_to_end(&mut decompressed).unwrap();

        assert_eq!(input.len(), nread);
        assert_eq!(&input[..], &decompressed[..]);
    }
}
