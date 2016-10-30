// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of a Huffman encoder.
//!
//! Based on the static Huffman encoder in Mark Nelson, Jean-Loup
//! Gailly: The Data Compression Book, 2nd Edition, M&T Books, 1996.

use std::io::{Read, Write};
use std::io;
use error::Error;
use bitfile::{BitWriter, BitReader};

const BLOCK_SIZE: usize = 1024 * 64;
const EOB: usize = 256;
const EOF: usize = 257;

const MAX_COUNT: usize = 0x1fff;

type Symbol = u32;

#[derive(Clone, Copy)]
struct Node {
    weight: usize,
    child0: usize,
    child1: usize,
    parent: usize,
    active: bool,
}

pub struct CompressWriter<W> {
    inner: BitWriter<W>,
    block: [u8; BLOCK_SIZE],
    fill:  usize,
    freqs: [usize; EOF + 1],
    tree:  [Node; 2 * (EOF + 1) + 1],
    codes: [(u64, usize); EOF + 1],
}

impl<W: Write> CompressWriter<W> {
    pub fn new(inner: W) -> CompressWriter<W>{
        CompressWriter {
            inner: BitWriter::new(inner),
            block: [0; BLOCK_SIZE],
            fill:  0,
            freqs: [0; EOF + 1],
            tree: [Node{weight: 0, child0: 0, child1: 0, parent: 0, active: false};
                   2 * (EOF + 1) + 1],
            codes: [(0, 0); EOF + 1],
        }
    }

    fn reset(&mut self) {
        for i in 0..self.freqs.len() {
            self.freqs[i] = 0;
        }
        for i in 0..self.tree.len() {
            self.tree[i].weight = 0;
            self.tree[i].child0 = 0;
            self.tree[i].child1 = 0;
            self.tree[i].parent = 0;
            self.tree[i].active = false;
        }
        for i in 0..self.codes.len() {
            self.codes[i].0 = 0;
            self.codes[i].1 = 0;
        }
    }
    
    fn count_freqs(&mut self) {
        for i in 0..EOB {
            self.freqs[i] = 0;
        }
        self.freqs[EOF] = 1;
        self.freqs[EOB] = 1;
        for i in 0..self.fill {
            self.freqs[self.block[i] as usize] += 1;
        }
        let mut max_count = 0;
        for i in 0..EOF {
            if self.freqs[i] > max_count {
                max_count = self.freqs[i];
            }
        }
        while max_count > MAX_COUNT {
            max_count = 0;
            for i in 0..EOF {
                self.freqs[i] = (self.freqs[i] + 1) / 2;
            }
            for i in 0..EOF {
                if self.freqs[i] > max_count {
                    max_count = self.freqs[i];
                }
            }
        }
    }

    fn build_tree(&mut self) -> usize {
        for i in 0..EOF + 1 {
            if self.freqs[i] > 0 {
                self.tree[i].weight = self.freqs[i];
                self.tree[i].active = true;
            }
        }
        let last_node_idx = 2 * (EOF + 1);
        self.tree[last_node_idx].weight = 0xffff;
        let mut next_free = EOF + 1;
        loop {
            let mut min1 = last_node_idx;
            let mut min2 = last_node_idx;

            for i in 0..next_free {
                if self.tree[i].active {
                    if self.tree[i].weight < self.tree[min1].weight {
                        min2 = min1;
                        min1 = i;
                    } else if self.tree[i].weight < self.tree[min2].weight {
                        min2 = i;
                    }
                }
            }
            if min2 == last_node_idx {
                break;
            }
            self.tree[next_free].weight =
                self.tree[min1].weight + self.tree[min2].weight;
            self.tree[next_free].active = true;
            self.tree[min1].active = false;
            self.tree[min2].active = false;
            self.tree[min1].parent = next_free;
            self.tree[min2].parent = next_free;
            self.tree[next_free].child0 = min1;
            self.tree[next_free].child1 = min2;
            next_free += 1;
        }
        next_free - 1
    }

    // fn dump_freqs(&self) {
    //     for i in 0..EOF + 1 {
    //         if self.freqs[i] > 0 {
    //             match i {
    //                 EOB => println!("EOB: {}", self.freqs[i]),
    //                 EOF => println!("EOF: {}", self.freqs[i]),
    //                 _ =>
    //                     println!("{:3} {:?}: {}", i, (i as u8) as char, self.freqs[i]),
    //             }
    //         }
    //     }
    // }
    
    // fn dump_tree(&self, root: usize, indent: usize) {
    //     for _ in 0..indent {
    //         print!(" ");
    //     }
    //     if root <= EOF {
    //         match root {
    //             EOB => println!("EOB"),
    //             EOF => println!("EOF"),
    //             _ if root < 256 => println!("{} {:?}", root, (root as u8) as char),
    //             _ => println!("{}", root),
    //         }
    //     } else {
    //         println!("{}", root);
    //         self.dump_tree(self.tree[root].child0, indent + 1);
    //         self.dump_tree(self.tree[root].child1, indent + 1);
    //     }
    // }

    fn calc_code(&self, root: usize, sym: Symbol) -> (u64, usize) {
        let mut node = sym as usize;
        let mut code = 0u64;
        let mut code_len = 0;
        while node != root {
            code >>= 1;
            if self.tree[self.tree[node].parent].child1 == node {
                code |= 1 << 63;
            }
            code_len += 1;
            node = self.tree[node].parent;
        }
        code >>= 64 - code_len;
        assert!(code_len < 20);
        (code, code_len)
    }

    fn calc_codes(&mut self, root: usize) {
        for i in 0..EOF+1 {
            if self.tree[i].weight > 0 {
                self.codes[i] = self.calc_code(root, i as Symbol);
            }
        }
    }

    fn write_freqs(&mut self) -> io::Result<()> {
        let mut first = 0;
        while first < 255 && self.freqs[first] == 0 {
            first += 1;
        }
            
        while first < 256 {
            let mut last = first + 1;
            let mut next;
            loop {
                while last < 256 && self.freqs[last] != 0 {
                    last += 1;
                }
                last -= 1;
                next = last + 1;
                while next < 256 && self.freqs[next] == 0 {
                    next += 1;
                }
                if next == 256 {
                    break;
                }
                if next - last > 3 {
                    break;
                }
                last = next;
            }
            try!(self.inner.write_bits(first as u64, 8));
            try!(self.inner.write_bits(last as u64, 8));
            for i in first..last + 1 {
                try!(self.inner.write_bits(self.freqs[i] as u64, 16));
            }
            first = next;
        }
        try!(self.inner.write_bits(0, 8));
        
        Ok(())
    }
    
    fn process_block(&mut self, final_block: bool) -> io::Result<()> {
        self.reset();
        self.count_freqs();
        let root = self.build_tree();

        self.calc_codes(root);

        try!(self.write_freqs());
        
        for i in 0..self.fill {
            let c = self.block[i];
            let (code, code_len) = self.codes[c as usize];
            try!(self.inner.write_bits(code, code_len));
        }
        let marker = if final_block { EOF } else { EOB };
        let (code, code_len) = self.codes[marker as usize];
        try!(self.inner.write_bits(code, code_len));
        self.fill = 0;
        Ok(())
    }
    
    fn process(&mut self, input: &[u8]) -> io::Result<usize> {
        let mut input_ptr = 0;
        while input_ptr < input.len() {
            let space = BLOCK_SIZE - self.fill;
            let cp = ::std::cmp::min(space, input.len() - input_ptr);
            for i in 0..cp {
                self.block[self.fill + i] = input[input_ptr + i];
            }
            self.fill += cp;
            input_ptr += cp;
            if self.fill == BLOCK_SIZE {
                try!(self.process_block(false));
            }
        }
        
        Ok(input_ptr)
    }

    pub fn to_inner(self) -> W {
        self.inner.to_inner()
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
        try!(self.process_block(true));
        self.inner.flush()
    }
}

pub struct DecompressReader<R> {
    inner: BitReader<R>,
    freqs: [usize; EOF + 1],
    tree:  [Node; 2 * (EOF + 1) + 1],
    root: usize,
    in_block: bool,
    eof: bool,
}

impl<R: Read> DecompressReader<R> {
    pub fn new(inner: R) -> DecompressReader<R> {
        DecompressReader {
            inner: BitReader::new(inner),
            freqs: [0; EOF + 1],
            tree: [Node{weight: 0, child0: 0, child1: 0, parent: 0, active: false};
                   2 * (EOF + 1) + 1],
            root: 0,
            in_block: false,
            eof: false,
        }
    }

    fn reset(&mut self) {
        for i in self.freqs.iter_mut() {
            *i = 0;
        }
        for i in self.tree.iter_mut() {
            i.weight = 0;
            i.child0 = 0;
            i.child1 = 0;
            i.parent = 0;
            i.active = false;
        }
    }
    
    fn build_tree(&mut self) -> usize {
        for i in 0..EOF + 1 {
            if self.freqs[i] > 0 {
                self.tree[i].weight = self.freqs[i];
                self.tree[i].active = true;
            } else {
                self.tree[i].active = false;
            }
        }
        let last_node_idx = 2 * (EOF + 1);
        self.tree[last_node_idx].weight = 0xffff;
        let mut next_free = EOF + 1;
        loop {
            let mut min1 = last_node_idx;
            let mut min2 = last_node_idx;

            for i in 0..next_free {
                if self.tree[i].active {
                    if self.tree[i].weight < self.tree[min1].weight {
                        min2 = min1;
                        min1 = i;
                    } else if self.tree[i].weight < self.tree[min2].weight {
                        min2 = i;
                    }
                }
            }
            if min2 == last_node_idx {
                break;
            }
            self.tree[next_free].weight =
                self.tree[min1].weight + self.tree[min2].weight;
            self.tree[next_free].active = true;
            self.tree[min1].active = false;
            self.tree[min2].active = false;
            self.tree[min1].parent = next_free;
            self.tree[min2].parent = next_free;
            self.tree[next_free].child0 = min1;
            self.tree[next_free].child1 = min2;
            next_free += 1;
        }
        next_free - 1
    }

    fn decode(&mut self, root: usize) -> io::Result<Symbol> {
        let mut n = root;
        while n > EOF {
            let b = try!(self.inner.read_bits(1)) as usize;
            if b == 0 {
                n = self.tree[n].child0;
            } else {
                n = self.tree[n].child1;
            }
        }
        Ok(n as Symbol)
    }

    // fn dump_freqs(&self) {
    //     for i in 0..EOF + 1 {
    //         if self.freqs[i] > 0 {
    //             match i {
    //                 EOB => println!("EOB: {}", self.freqs[i]),
    //                 EOF => println!("EOF: {}", self.freqs[i]),
    //                 _ =>
    //                     println!("{:3} {:?}: {}", i, (i as u8) as char, self.freqs[i]),
    //             }
    //         }
    //     }
    // }
    
    fn read_freqs(&mut self) -> io::Result<()> {
        let mut first = try!(self.inner.read_bits(8)) as usize;
        let mut last  = try!(self.inner.read_bits(8)) as usize;
        loop {
            for i in first..last + 1 {
                let freq  = try!(self.inner.read_bits(16)) as usize;
                self.freqs[i] = freq;
            }
            first = try!(self.inner.read_bits(8)) as usize;
            if first == 0 {
                break;
            }
            last  = try!(self.inner.read_bits(8)) as usize;
        }
        self.freqs[EOB] = 1;
        self.freqs[EOF] = 1;
        Ok(())
    }
    
    fn process(&mut self, output: &mut [u8]) -> io::Result<usize> {

        if self.eof {
            return Ok(0);
        }
        
        let mut written = 0;
        'outer:
        while written < output.len() {
            if !self.in_block {
                self.reset();
                try!(self.read_freqs());
                self.root = self.build_tree();
                self.in_block = true;
            }
            'inner:
            loop {
                let root = self.root;
                let b = try!(self.decode(root));
                if b as usize == EOF {
                    self.eof = true;
                    break 'outer;
                } else if b as usize == EOB {
                    self.in_block = false;
                    break 'inner;
                } else {
                    output[written] = b as u8;
                    written += 1;
                    if written == output.len() {
                        break 'outer;
                    }
                }
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
mod test {
    use ::std::io::{Cursor, Write, Read};
    use super::{CompressWriter, DecompressReader};
    
    #[test]
    fn compress_empty() {
        let input = b"";
        let mut cw = CompressWriter::new(vec![]);
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        let expected = [255, 255, 0, 0, 0, 128];
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_a() {
        let input = b"a";
        let mut cw = CompressWriter::new(vec![]);
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        let expected = [97, 97, 0, 1, 0, 128];
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_aaa() {
        let input = b"aaaaaaaaa";
        let mut cw = CompressWriter::new(vec![]);
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        let expected = [97, 97, 0, 9, 0, 255, 160];
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_lorem() {
        let input = b"Lorem ipsum dolor sit amet, consetetur \
                      sadipscing elitr, sed diam nonumy eirmod \
                      tempor invidunt ut labore et dolore magna \
                      aliquyam erat, sed diam voluptua. At vero \
                      eos et accusam et justo duo dolores et ea \
                      rebum. Stet clita kasd gubergren, no sea \
                      takimata sanctus est Lorem ipsum dolor sit \
                      amet. Lorem ipsum dolor sit amet, consetetur \
                      sadipscing elitr, sed diam nonumy eirmod \
                      tempor invidunt ut labore et dolore magna \
                      aliquyam erat, sed diam voluptua. At vero \
                      eos et accusam et justo duo dolores et ea \
                      rebum. Stet clita kasd gubergren, no sea \
                      takimata sanctus est Lorem ipsum dolor sit \
                      amet.";
        let mut cw = CompressWriter::new(vec![]);
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        let expected =
            [32, 32, 0, 99, 44, 46, 0, 8, 0, 0, 0, 6, 65, 65, 0, 2, 76, 76, 0,
             4, 83, 83, 0, 2, 97, 121, 0, 44, 0, 6, 0, 12, 0, 26, 0, 56, 0, 0,
             0, 8, 0, 0, 0, 30, 0, 2, 0, 4, 0, 18, 0, 32, 0, 20, 0, 42, 0, 10,
             0, 2, 0, 32, 0, 36, 0, 50, 0, 30, 0, 6, 0, 0, 0, 0, 0, 4, 0, 207,
             166, 11, 207, 8, 69, 229, 82, 166, 240, 123, 237, 70, 185, 241,
             169, 192, 209, 168, 222, 44, 143, 8, 196, 230, 239, 18, 61, 103,
             60, 2, 228, 118, 190, 117, 52, 87, 188, 27, 45, 23, 208, 184, 83,
             115, 158, 99, 36, 158, 244, 223, 43, 203, 76, 56, 222, 85, 42, 97,
             214, 221, 157, 251, 145, 191, 163, 219, 94, 26, 245, 207, 0, 185,
             29, 175, 205, 82, 76, 53, 47, 39, 124, 223, 152, 53, 113, 81, 198,
             251, 199, 20, 139, 94, 55, 191, 36, 109, 114, 74, 229, 82, 166,
             17, 198, 241, 125, 134, 84, 92, 157, 247, 70, 252, 100, 123, 125,
             229, 193, 119, 83, 40, 103, 88, 77, 207, 58, 240, 47, 237, 188,
             53, 189, 191, 23, 60, 117, 35, 136, 223, 159, 76, 23, 158, 16,
             139, 202, 165, 77, 224, 247, 218, 141, 201, 243, 233, 130, 243,
             194, 17, 121, 84, 169, 188, 30, 251, 81, 174, 124, 106, 112, 52,
             106, 55, 139, 35, 194, 49, 57, 187, 196, 143, 89, 207, 0, 185,
             29, 175, 157, 77, 21, 239, 6, 203, 69, 244, 46, 20, 220, 231,
             152, 201, 39, 189, 55, 202, 242, 211, 14, 55, 149, 74, 152, 117,
             183, 103, 126, 228, 111, 232, 246, 215, 134, 189, 115, 192, 46,
             71, 107, 243, 84, 147, 13, 75, 201, 223, 55, 230, 13, 92, 84,
             113, 190, 241, 197, 34, 215, 141, 239, 201, 27, 92, 146, 185,
             84, 169, 132, 113, 188, 95, 97, 149, 23, 39, 125, 209, 191, 25,
             30, 223, 121, 112, 93, 212, 202, 25, 214, 19, 115, 206, 188, 11,
             251, 111, 13, 111, 111, 197, 207, 29, 72, 226, 55, 231, 211, 5,
             231, 132, 34, 242, 169, 83, 120, 61, 246, 163, 114, 103, 64];
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn decompress_empty() {
        let input = [255, 255, 0, 0, 0, 128];
        let mut cr = DecompressReader::new(Cursor::new(input));
        let mut decompressed = Vec::new();
        let _ = cr.read_to_end(&mut decompressed).unwrap();
        let expected = b"";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_a() {
        let input = [97, 97, 0, 1, 0, 128];
        let mut cr = DecompressReader::new(Cursor::new(input));
        let mut decompressed = Vec::new();
        let _ = cr.read_to_end(&mut decompressed).unwrap();
        let expected = b"a";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_aaa() {
        let input = [97, 97, 0, 9, 0, 255, 160];
        let mut cr = DecompressReader::new(Cursor::new(input));
        let mut decompressed = Vec::new();
        let _ = cr.read_to_end(&mut decompressed).unwrap();
        let expected = b"aaaaaaaaa";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_lorem() {
        let input =
            [32, 32, 0, 99, 44, 46, 0, 8, 0, 0, 0, 6, 65, 65, 0, 2, 76, 76, 0,
             4, 83, 83, 0, 2, 97, 121, 0, 44, 0, 6, 0, 12, 0, 26, 0, 56, 0, 0,
             0, 8, 0, 0, 0, 30, 0, 2, 0, 4, 0, 18, 0, 32, 0, 20, 0, 42, 0, 10,
             0, 2, 0, 32, 0, 36, 0, 50, 0, 30, 0, 6, 0, 0, 0, 0, 0, 4, 0, 207,
             166, 11, 207, 8, 69, 229, 82, 166, 240, 123, 237, 70, 185, 241,
             169, 192, 209, 168, 222, 44, 143, 8, 196, 230, 239, 18, 61, 103,
             60, 2, 228, 118, 190, 117, 52, 87, 188, 27, 45, 23, 208, 184, 83,
             115, 158, 99, 36, 158, 244, 223, 43, 203, 76, 56, 222, 85, 42, 97,
             214, 221, 157, 251, 145, 191, 163, 219, 94, 26, 245, 207, 0, 185,
             29, 175, 205, 82, 76, 53, 47, 39, 124, 223, 152, 53, 113, 81, 198,
             251, 199, 20, 139, 94, 55, 191, 36, 109, 114, 74, 229, 82, 166,
             17, 198, 241, 125, 134, 84, 92, 157, 247, 70, 252, 100, 123, 125,
             229, 193, 119, 83, 40, 103, 88, 77, 207, 58, 240, 47, 237, 188,
             53, 189, 191, 23, 60, 117, 35, 136, 223, 159, 76, 23, 158, 16,
             139, 202, 165, 77, 224, 247, 218, 141, 201, 243, 233, 130, 243,
             194, 17, 121, 84, 169, 188, 30, 251, 81, 174, 124, 106, 112, 52,
             106, 55, 139, 35, 194, 49, 57, 187, 196, 143, 89, 207, 0, 185,
             29, 175, 157, 77, 21, 239, 6, 203, 69, 244, 46, 20, 220, 231,
             152, 201, 39, 189, 55, 202, 242, 211, 14, 55, 149, 74, 152, 117,
             183, 103, 126, 228, 111, 232, 246, 215, 134, 189, 115, 192, 46,
             71, 107, 243, 84, 147, 13, 75, 201, 223, 55, 230, 13, 92, 84,
             113, 190, 241, 197, 34, 215, 141, 239, 201, 27, 92, 146, 185,
             84, 169, 132, 113, 188, 95, 97, 149, 23, 39, 125, 209, 191, 25,
             30, 223, 121, 112, 93, 212, 202, 25, 214, 19, 115, 206, 188, 11,
             251, 111, 13, 111, 111, 197, 207, 29, 72, 226, 55, 231, 211, 5,
             231, 132, 34, 242, 169, 83, 120, 61, 246, 163, 114, 103, 64];
        let mut cr = DecompressReader::new(Cursor::new(&input[..]));
        let mut decompressed = Vec::new();
        let _ = cr.read_to_end(&mut decompressed).unwrap();
        let expected = b"Lorem ipsum dolor sit amet, consetetur \
                         sadipscing elitr, sed diam nonumy eirmod \
                         tempor invidunt ut labore et dolore magna \
                         aliquyam erat, sed diam voluptua. At vero \
                         eos et accusam et justo duo dolores et ea \
                         rebum. Stet clita kasd gubergren, no sea \
                         takimata sanctus est Lorem ipsum dolor sit \
                         amet. Lorem ipsum dolor sit amet, consetetur \
                         sadipscing elitr, sed diam nonumy eirmod \
                         tempor invidunt ut labore et dolore magna \
                         aliquyam erat, sed diam voluptua. At vero \
                         eos et accusam et justo duo dolores et ea \
                         rebum. Stet clita kasd gubergren, no sea \
                         takimata sanctus est Lorem ipsum dolor sit \
                         amet.";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_decompress() {
        let input = include_bytes!("huff.rs");
        let mut cw = CompressWriter::new(vec![]);
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        
        let mut cr = DecompressReader::new(Cursor::new(&compressed[..]));
        let mut decompressed = Vec::new();
        let _ = cr.read_to_end(&mut decompressed).unwrap();
        
        assert_eq!(&input[..], &decompressed[..]);
    }
}
