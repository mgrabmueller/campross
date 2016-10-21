// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of an LZ77+Huffman compressor.

use std::io::{Read, Write};
use std::io;

const WINDOW_SIZE: usize = 40; //1 << 12;
const LOOK_AHEAD_SIZE: usize = 17;

const WINDOW_BUFFER_SIZE: usize = WINDOW_SIZE * 2 + LOOK_AHEAD_SIZE;

pub struct CompressWriter<W> {
    inner: W,
    window: [u8; WINDOW_BUFFER_SIZE],
    position: usize,
    limit: usize,
    current_buffer: usize,
}

impl<W> CompressWriter<W> {
    pub fn new(inner: W) -> CompressWriter<W>{
        CompressWriter {
            inner: inner,
            window: [0; WINDOW_BUFFER_SIZE],
            position: 0,
            limit: 0,
            current_buffer: 0,
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
    }

    fn process(&mut self, flush: bool) {
        let headroom = if flush { 0 } else { LOOK_AHEAD_SIZE };
        while self.position + headroom < self.limit {

            // Do compression magic here...

            // ... and advance position.
            let pos = self.position;
            self.hash(pos);
            self.position += 1;
        }
    }

    fn hash(&mut self, _i: usize) {
        // Add entry at position `i` to hash table.
    }
    
    fn unhash(&mut self, _i: usize) {
        // Remove entry at position `i` from hash table because it is
        // about to be overwritten.
    }
    
    fn slide_down(&mut self) {
        for i in 0..WINDOW_SIZE + LOOK_AHEAD_SIZE {
            self.unhash(i);
            self.window[i] = self.window[i + WINDOW_SIZE];
        }
        self.position -= WINDOW_SIZE;
        self.limit -= WINDOW_SIZE;
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

            self.process(false);
            
            if self.position >= WINDOW_SIZE * 2 {
                self.slide_down();
            }
            Ok(amount)
        } else {
            Ok(0)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        while self.position < self.limit {
            self.process(true);
        }
        Ok(())
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
        let data = b"abcdefg. ";
        cw.write(&data[..]).unwrap();
        cw.dump_window();

        let data = b"let x = 1; let y = 2; let z = x * 2 + y;";
        cw.write(&data[..]).unwrap();
        cw.dump_window();

        let data = b".0123456789.0123456789.";
        cw.write(&data[..]).unwrap();
        cw.dump_window();

        let data = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ.";
        cw.write_all(&data[..]).unwrap();
        cw.dump_window();

        cw.flush().unwrap();
        cw.dump_window();
//        assert!(false);
    }

    #[test]
    fn compress_decompress() {
        let input = include_bytes!("lzmg1.rs");
        let mut cw = CompressWriter::new(vec![]);
        cw.write_all(&input[..]).unwrap();
        cw.dump_window();
        cw.flush().unwrap();
        cw.dump_window();
        assert!(false);
    }
}
