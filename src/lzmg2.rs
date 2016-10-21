// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of an LZ77+Huffman compressor.

use std::io::{Read, Write};
use std::io;

const MAX_MATCH: usize = 1 << 4;
const MIN_MATCH: usize = 3;

const WINDOW_SIZE: usize = 1 << 12;
//const WINDOW_SIZE: usize = 30;
const LOOK_AHEAD_SIZE: usize = MAX_MATCH + MIN_MATCH;

const WINDOW_BUFFER_SIZE: usize = WINDOW_SIZE * 2 + LOOK_AHEAD_SIZE;
const HASHTAB_SIZE: usize = 1 << 10;
//const HASHTAB_SIZE: usize = 1 << 5;

const UNUSED: usize = !0;

pub struct CompressWriter<W> {
    inner:    W,
    window:   [u8; WINDOW_BUFFER_SIZE],
    hashtab:  [usize; HASHTAB_SIZE],
    hashes:   [usize; WINDOW_BUFFER_SIZE],
    position: usize,
    limit:    usize,

    emit_token: u8,
    emit_cnt: usize,
    emit_data: [u8; 17],
    emit_len: usize,
    hash_collisions: usize,
}

impl<W: Write> CompressWriter<W> {
    pub fn new(inner: W) -> CompressWriter<W>{
        CompressWriter {
            inner:    inner,
            window:   [0; WINDOW_BUFFER_SIZE],
            hashtab:  [UNUSED; HASHTAB_SIZE],
            hashes:   [UNUSED; WINDOW_BUFFER_SIZE],
            position: 0,
            limit:    0,
            emit_token: 0,
            emit_cnt: 0,
            emit_data: [0; 2 * 8 + 1], // max. 2 bytes for pos/len * 8 + token
            emit_len: 0,
            hash_collisions: 0,
        }
    }

    pub fn dump_window(&self) {
        println!("");
        for i in 0..WINDOW_BUFFER_SIZE {
            if i == 0 {
                print!("|");
            } else if i == WINDOW_SIZE {
                print!("|");
            } else if i == WINDOW_SIZE * 2 {
                print!("|");
            } else {
                print!(" ");
            }
        }
        println!("|");
        for i in 0..WINDOW_BUFFER_SIZE + 1 {
            if i == self.position + LOOK_AHEAD_SIZE {
                print!("v");
            } else {
                print!(" ");
            }
        }
        println!("");
        for i in 0..self.limit {
            if self.window[i] >= 32 && self.window[i] < 128 {
                print!("{}", self.window[i] as char);
            } else {
                print!("?");
            }
        }
        for _ in self.limit..WINDOW_BUFFER_SIZE {
            print!("~");
        }
        println!("");
        for i in 0..WINDOW_BUFFER_SIZE + 1 {
            if i == self.position {
                print!("p");
            } else if i == self.limit {
                print!("|");
            } else if i + WINDOW_SIZE >= self.position && i < self.position {
                print!("-");
            } else {
                print!(" ");
            }
        }
        println!("");
        for i in 0..WINDOW_BUFFER_SIZE + 1 {
            if i == self.limit {
                print!("l");
            } else {
                print!(" ");
            }
        }
        println!("");
        for i in 0..WINDOW_BUFFER_SIZE {
            if self.hashes[i] == UNUSED {
                print!(".");
            } else {
                print!("^");
            }
        }
        println!("");
        for i in 0..HASHTAB_SIZE {
            if self.hashtab[i] == UNUSED {
                print!("|--");
            } else {
                print!("|{:2}", self.hashtab[i]);
            }
        }
        println!("|");
        println!("Hash coll: {}", self.hash_collisions);
    }

    fn emit_flush(&mut self) -> io::Result<()> {
        
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
        if self.emit_cnt == 8 {
            try!(self.emit_flush());
        }
        let lp1: u8 = ((((len - MIN_MATCH) as u8) & 0x0f)  << 4) | (ofs as u8) & 0x0f;
        let p2: u8 = ((ofs as u8) >> 4) & 0xff;
        self.emit_token = self.emit_token << 1;
        self.emit_data[self.emit_len + 1] = lp1;
        self.emit_data[self.emit_len + 2] = p2;
        self.emit_cnt += 1;
        self.emit_len += 2;
        Ok(())
    }
    
    fn process(&mut self, flush: bool) -> io::Result<()> {
        let headroom = if flush { 0 } else { LOOK_AHEAD_SIZE };
        while self.position + headroom < self.limit {

            let h = self.calc_hash(self.position);
            let search_pos = self.hashtab[h];
            let mut match_len = 0;

            if search_pos != UNUSED {
                for i in 0..MAX_MATCH {
                    if search_pos + i >= self.limit {
                        break;
                    }
                    if self.window[search_pos + i] != self.window[self.position + i] {
                        break;
                    }
                    match_len += 1;
                }
                if match_len > 0 {
                }
            }
            let replace =
                if match_len > MIN_MATCH {
                    let ofs = self.position - search_pos;
                    try!(self.emit_match(ofs, match_len));
                    println!("match: len: {}, ofs: {} ({} -> {})", match_len, ofs,
                             search_pos, self.position);
                    match_len
                } else {
                    let lit = self.window[self.position];
                    try!(self.emit_literal(lit));
                    println!("literal: {:?}", self.window[self.position]);
                    1
                };
            for _ in 0..replace {
                let pos = self.position;
                if pos >= WINDOW_SIZE {
                    self.unhash(pos - WINDOW_SIZE);
                }
                self.hash(pos);
                self.position += 1;
            }
        }
        Ok(())
    }

    fn calc_hash(&self, i: usize) -> usize {
        let mut hash: usize = 0;
        for x in i .. ::std::cmp::min(i + 3, self.limit) {
            hash = (hash << 8) | self.window[x] as usize;
        }
        hash = ((hash >> 5) ^ hash) & (HASHTAB_SIZE - 1);
        hash
    }
    
    fn hash(&mut self, i: usize) {
        let hash = self.calc_hash(i);
        if self.hashtab[hash] != UNUSED {
            self.hash_collisions += 1;
        }
        self.hashtab[hash] = i;
        self.hashes[i] = hash;
    }
    
    fn unhash(&mut self, i: usize) {
        let hashpos = self.hashes[i];
        if hashpos != UNUSED {
            self.hashtab[hashpos] = UNUSED;
            self.hashes[i] = UNUSED;
        }
    }
    
    fn slide_down(&mut self) {
        for i in 0..WINDOW_BUFFER_SIZE {
            self.unhash(i);
        }
        for i in 0..WINDOW_SIZE + LOOK_AHEAD_SIZE {
            self.window[i] = self.window[i + WINDOW_SIZE];
        }
        self.position -= WINDOW_SIZE;
        self.limit -= WINDOW_SIZE;
        for i in 0..::std::cmp::min(self.position, WINDOW_SIZE) {
            self.hash(i);
        }
    }

    pub fn to_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for CompressWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let space = WINDOW_BUFFER_SIZE - self.limit;
        if space > 0 {
            let amount = ::std::cmp::min(space, buf.len());
            for t in 0..amount {
                self.window[self.limit + t] = buf[t];
            }
            self.limit += amount;

            try!(self.process(false));
            
            if self.position >= WINDOW_SIZE * 2 {
                self.slide_down();
            }
            Ok(amount)
        } else {
            Ok(0)
        }
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
    inner: R,
}

impl<R> DecompressReader<R> {
    pub fn new(inner: R) -> DecompressReader<R> {
        DecompressReader {
            inner: inner,
        }
    }
}

impl<R: Read> Read for DecompressReader<R> {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
//    use ::std::io::Cursor;

    use super::CompressWriter;
    use ::std::io::Write;
    
    #[test]
    fn compress_empty() {
        let mut cw = CompressWriter::new(vec![]);
        let data = b"abcdefghij";
        cw.write(&data[..]).unwrap();
        cw.dump_window();

        cw.write(&data[..]).unwrap();
        cw.dump_window();

        cw.write(&data[..]).unwrap();
        cw.dump_window();

        cw.write(&data[..]).unwrap();
        cw.dump_window();

        cw.flush().unwrap();
        cw.dump_window();
        let compressed = cw.to_inner();
        println!("comp: {:?}", compressed);
        println!("ratio: {:.2}%",
                 1f32 - (compressed.len() as f32 / (data.len() * 4) as f32));
//        assert!(false);
    }

    #[test]
    fn compress_decompress() {
        let input = include_bytes!("lzmg1.rs");
        let mut cw = CompressWriter::new(vec![]);
        cw.write_all(&input[..]).unwrap();
        cw.flush().unwrap();
//        cw.dump_window();
        let compressed = cw.to_inner();
        println!("ratio: {:.2}%",
                 1f32 - (compressed.len() as f32 / input.len() as f32));
        assert!(false);
    }
}
