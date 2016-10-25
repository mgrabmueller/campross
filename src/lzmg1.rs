// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of an LZ4-like compressor.

use std::io::{Read, Write};
use std::collections::HashMap;

use error::Error;

const INDEX_BIT_COUNT: usize = 12;
const LENGTH_BIT_COUNT: usize = 8;
const WINDOW_SIZE: usize = 1 << INDEX_BIT_COUNT;
const RAW_LOOK_AHEAD_SIZE: usize = 1 << LENGTH_BIT_COUNT;
const BREAK_EVEN: usize = (1 + INDEX_BIT_COUNT + LENGTH_BIT_COUNT) / 9;
const LOOK_AHEAD_SIZE: usize = RAW_LOOK_AHEAD_SIZE + BREAK_EVEN;

pub struct Compressor<R, W> {
    input: R,
    output: W,
    hash_table: HashMap<u32, usize>,
    window: [u8; WINDOW_SIZE],
    hashes: [u32; WINDOW_SIZE],
    next: [Option<usize>; WINDOW_SIZE],
    literals: Vec<u8>,
    lookups: usize,
    lin_lookups: usize,
}

impl<R, W> Compressor<R, W> {
    pub fn new(r: R, w: W) -> Compressor<R, W> {
        Compressor {
            input: r,
            output: w,
            hash_table: HashMap::new(),
            window: [0; WINDOW_SIZE],
            hashes: [0; WINDOW_SIZE],
            next: [None; WINDOW_SIZE],
            literals: Vec::new(),
            lookups: 0,
            lin_lookups: 0,
        }
    }

    pub fn finish(self) -> W {
//        println!("lookups: {}", self.lookups);
//        println!("lin_lookups: {}", self.lin_lookups);
        self.output
    }
}

impl<R: Read, W: Write> Compressor<R, W> {
    fn hash_at(&self, p: usize) -> u32 {
        ((self.window[self.mod_window(p)] as u32) << 24) +
            ((self.window[self.mod_window(p + 1)] as u32) << 16) +
            ((self.window[self.mod_window(p + 2)] as u32) << 8) +
            (self.window[self.mod_window(p + 3)] as u32)
    }

    fn getc(&mut self) -> Result<Option<u8>, Error> {
        let mut buf = [0u8; 1];
        let n = try!(self.input.read(&mut buf[..]));
        if n == 1 {
            Ok(Some(buf[0]))
        } else {
            Ok(None)
        }
    }

    fn delete_string(&mut self, pos: usize) {
        let hsh = self.hashes[pos];
        self.hash_table.remove(&hsh);
        if let Some(next_pos) = self.next[pos] {
            self.hash_table.insert(self.hashes[next_pos], next_pos);
        }
        self.hashes[pos] = 0;
        self.next[pos] = None;
    }

    fn get_longest_match(&mut self, hsh: u32, current_pos: usize,
                         look_ahead_bytes: usize) -> Option<(usize, usize)> {
        self.lookups += 1;
        let res =
            if let Some(hpos) = self.hash_table.get(&hsh) {
                let mut max_pos = *hpos;
                let mut max_len = 4;
                let mut pos = max_pos;
                let mut iterations = 0;
                loop {
                    let mut len = 0;
                    for i in 0..look_ahead_bytes {
                        if self.window[self.mod_window(max_pos + i)] == self.window[self.mod_window(current_pos + i)] {
                            len += 1;
                        } else {
                            break;
                        }
                    }
                    if len > max_len {
                        max_len = len;
                        max_pos = pos;
                    }
                    if let Some(npos) = self.next[pos] {
                        pos = npos;
                        self.lin_lookups += 1;
                    } else {
                        break;
                    }
                    iterations += 1;
                    if iterations > 10 {
                        break;
                    }
                }
                Some((max_pos, max_len))
            } else {
                None
            };
        if let Some((p, _)) = res {
            self.hash_table.insert(hsh, current_pos);
            self.hashes[current_pos] = hsh;
            self.next[current_pos] = Some(p);
        } else {
            self.hash_table.insert(hsh, current_pos);
            self.hashes[current_pos] = hsh;
            self.next[current_pos] = None;
        }
        res
    }
    
    fn add_string(&mut self, pos: usize, look_ahead_bytes: usize,
                  match_pos: &mut usize) -> usize {
        if look_ahead_bytes < 4 {
            *match_pos = 0;
            0
        } else {
            let hsh = self.hash_at(pos);
            if let Some((hpos, hlen)) = self.get_longest_match(hsh, pos, look_ahead_bytes) {
                *match_pos = hpos;
                hlen
            } else {
                *match_pos = 0;
                0
            }
        }
    }

    fn mod_window(&self, p: usize) -> usize {
        p % WINDOW_SIZE
    }

    pub fn process(&mut self) -> Result<(), Error> {
        let mut current_position = 0;
        let mut look_ahead_bytes =
            try!(self.input.read(&mut self.window[..LOOK_AHEAD_SIZE]));

        let mut match_length = 0;
        let mut match_position = 0;
        let mut replace_count;
        let _ = self.add_string(current_position, look_ahead_bytes,
                                &mut match_position);
        while look_ahead_bytes > 0 {
            if match_length > look_ahead_bytes {
                match_length = look_ahead_bytes;
            }
            if match_length <= BREAK_EVEN {

                self.literals.push(self.window[current_position]);
                replace_count = 1;

            } else {
                try!(self.emit(match_position, match_length));

                replace_count = match_length;
            }
            for _ in 0..replace_count {

                let cp = self.mod_window(current_position + LOOK_AHEAD_SIZE);
                self.delete_string(cp);

                if let Some(c) = try!(self.getc()) {
                    self.window[self.mod_window(current_position + LOOK_AHEAD_SIZE)] = c;
                } else {
                    look_ahead_bytes -= 1;
                }
                current_position = self.mod_window(current_position + 1);
                if look_ahead_bytes > 0 {
                    match_length = self.add_string(current_position, look_ahead_bytes,
                                                   &mut match_position);
                }
            }
        }

        if self.literals.len() > 0 {
            try!(self.emit(0, 0));
        }

        Ok(())
    }

    fn emit(&mut self, match_pos: usize, match_len: usize) -> Result<(), Error> {
        let (lit_tok, lit_extra) =
            if self.literals.len() > 14 {
                (15u8, Some(self.literals.len()))
            } else {
                (self.literals.len() as u8, None)
            };
        let (match_tok, match_extra) =
            if match_len > 14 {
                (15u8, Some(match_len))
            } else {
                (match_len as u8, None)
            };
        let token = (lit_tok << 4) | match_tok;
        try!(self.output.write(&[token]));
        if let Some(mut le) = lit_extra {
            while le >= 255 {
                try!(self.output.write(&[255]));
                le -= 255;
            }
            try!(self.output.write(&[le as u8]));
        }
        if self.literals.len() > 0 {
            try!(self.output.write(&self.literals));
            self.literals.truncate(0);
        }
        if let Some(mut me) = match_extra {
            while me >= 255 {
                try!(self.output.write(&[255]));
                me -= 255;
            }
            try!(self.output.write(&[me as u8]));
        }
        if match_len > 0 {
            let mut mp = match_pos;
            while mp >= 255 {
                try!(self.output.write(&[255]));
                mp -= 255;
            }
            try!(self.output.write(&[mp as u8]));
        }
        Ok(())
    }
}

pub struct Decompressor<R, W> {
    input: R,
    output: W,
    window: [u8; WINDOW_SIZE],
}

impl<R, W> Decompressor<R, W> {
    pub fn new(r: R, w: W) -> Decompressor<R, W> {
        Decompressor {
            input: r,
            output: w,
            window: [0; WINDOW_SIZE],
        }
    }

    pub fn finish(self) -> W {
        self.output
    }
}

impl<R: Read, W: Write> Decompressor<R, W> {
    fn getc(&mut self) -> Result<Option<u8>, Error> {
        let mut buf = [0u8; 1];
        let n = try!(self.input.read(&mut buf[..]));
        if n == 1 {
            Ok(Some(buf[0]))
        } else {
            Ok(None)
        }
    }

    fn mod_window(&self, p: usize) -> usize {
        p % WINDOW_SIZE
    }

    fn get_len(&mut self) -> Result<(usize, usize), Error> {
        let mut llen = 0;
        let mut l: usize = 0;
        if let Some(mut c) = try!(self.getc()) {
            llen += 1;
            while c == 255 {
                l += 255;
                if let Some(cc) = try!(self.getc()) {
                    llen += 1;
                    c = cc;
                } else {
                    return Err(Error::UnexpectedEof);
                }
            }
            l += c as usize;
        }
        Ok((l, llen))
    }
    
    pub fn process(&mut self) -> Result<(), Error> {
        let mut current_position = 0;
        loop {
            if let Some(token) = try!(self.getc()) {
                let lit_tok = token >> 4;
                let match_tok = token & 0x0f;
                let (lit_len, _extra_lit_len) =
                    if lit_tok == 15 {
                        try!(self.get_len())
                    } else {
                        (lit_tok as usize, 0)
                    };
                let mut lit: Vec<u8> = Vec::new();
                let mut mtch: Vec<u8> = Vec::new();
                for _ in 0..lit_len {
                    if let Some(c) = try!(self.getc()) {
                        self.window[current_position] = c;
                        try!(self.output.write(&[c]));
                        current_position = self.mod_window(current_position + 1);
                        lit.push(c);
                    } else {
                        return Err(Error::UnexpectedEof);
                    }
                }
                let (match_len, _extra_match_len) =
                    if match_tok == 15 {
                        try!(self.get_len())

                    } else {
                        (match_tok as usize, 0)
                    };
                let (match_pos, _match_pos_len) = try!(self.get_len());
//                println!("literal length: {}, match length: {}, match pos: {}",
//                         lit_len, match_len, match_pos);
                for i in 0..match_len {
                    let c = self.window[self.mod_window(match_pos + i)];
                    self.window[current_position] = c;
                    try!(self.output.write(&[c]));
                    current_position = self.mod_window(current_position + 1);
                    mtch.push(c);
                }
//                let enc_len = 1 + extra_lit_len + lit_len + extra_match_len + match_pos_len;
//                let dec_len = lit_len + match_len;
//                println!("{:?} {:?}; {} -> {}", String::from_utf8_lossy(&lit), String::from_utf8_lossy(&mtch), enc_len, dec_len);
            } else {
                break;
            }
        }
        Ok(())
    }
}

pub fn compress<R: Read, W: Write>(input: R, output: W) -> Result<W, Error> {
    let mut compressor = Compressor::new(input, output);
    try!(compressor.process());
    Ok(compressor.finish())
}

pub fn decompress<R: Read, W: Write>(input: R, output: W) -> Result<W, Error> {
    let mut decompressor = Decompressor::new(input, output);
    try!(decompressor.process());
    Ok(decompressor.finish())
}

#[cfg(test)]
mod tests {
    use ::std::io::Cursor;
    use super::{compress, decompress};

    #[test]
    fn compress_empty() {
    }

    #[test]
    fn compress_decompress() {
        let input = include_bytes!("lzmg1.rs");
        let result = compress(Cursor::new(&input[..]), vec![]).unwrap();

        let dec_result = decompress(Cursor::new(&result[..]), vec![]).unwrap();
        assert_eq!(&input[..], &dec_result[..]);
    }
}
