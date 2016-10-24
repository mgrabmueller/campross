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

const BLOCK_SIZE: usize = 1024 * 32;
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
//            println!("min1 {} min2 {}", min1, min2);
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

    fn dump_freqs(&self) {
        for i in 0..EOF + 1 {
            if self.freqs[i] > 0 {
                match i {
                    EOB => println!("EOB: {}", self.freqs[i]),
                    EOF => println!("EOF: {}", self.freqs[i]),
                    _ =>
                        println!("{:3} {:?}: {}", i, (i as u8) as char, self.freqs[i]),
                }
            }
        }
    }
    
    fn dump_tree(&self, root: usize, indent: usize) {
        for _ in 0..indent {
            print!(" ");
        }
        if root <= EOF {
            match root {
                EOB => println!("EOB"),
                EOF => println!("EOF"),
                _ if root < 256 => println!("{} {:?}", root, (root as u8) as char),
                _ => println!("{}", root),
            }
        } else {
            println!("{}", root);
            self.dump_tree(self.tree[root].child0, indent + 1);
            self.dump_tree(self.tree[root].child1, indent + 1);
        }
    }

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
    
    fn process_block(&mut self, final_block: bool) -> io::Result<()> {
        println!("");
        self.count_freqs();
        self.dump_freqs();
        let root = self.build_tree();
        self.dump_tree(root, 0);

        self.calc_codes(root);

        let mut sym_cnt = 0;
        for i in 0..EOF + 1 {
            if self.freqs[i] > 0 {
                sym_cnt += 1;
            }
        }
        try!(self.inner.write_bits(sym_cnt as u64, 9));
        for i in 0..EOB {
            if self.freqs[i] > 0 {
                try!(self.inner.write_bits(i as u64, 9));
                try!(self.inner.write_bits(self.freqs[i] as u64, 16));
            }
        }
        for i in 0..self.fill {
            let c = self.block[i];
            let (code, code_len) = self.codes[c as usize];
            try!(self.inner.write_bits(code, code_len));
            println!("{:032b} {}", code, code_len);
        }
        let marker = if final_block { EOF } else { EOB };
        let (code, code_len) = self.codes[marker as usize];
        try!(self.inner.write_bits(code, code_len));
        println!("{:032b} {}", code, code_len);
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
            input_ptr += 1;
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
            in_block: false,
            eof: false,
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
//            println!("min1 {} min2 {}", min1, min2);
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
    
    fn process(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let mut root = 0;
        
        if self.eof {
            return Ok(0);
        }
        
        let mut written = 0;
        'outer:
        loop {
            if !self.in_block {
                let sym_count = try!(self.inner.read_bits(9)) as usize;
                for _ in 0..sym_count {
                    let sym = try!(self.inner.read_bits(9));
                    let freq = try!(self.inner.read_bits(16));
                    self.freqs[sym as usize] = freq as usize;
                }
                self.freqs[EOB] = 1;
                self.freqs[EOF] = 1;
                root = self.build_tree();
                self.in_block = true;
            }
            'inner:
            for i in written..output.len() {
                let b = try!(self.decode(root));
                if b as usize == EOF {
                    self.eof = true;
                    break 'outer;
                } else if b as usize == EOB {
                    self.in_block = false;
                    break 'inner;
                } else {
                    output[i] = b as u8;
                    written += 1;
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
    use ::std::io::Write;
    use super::{CompressWriter}; //, decompress};
    
    #[test]
    fn compress_empty() {
        let input = b"";
        let mut cw = CompressWriter::new(vec![]);
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        let expected = [1, 64];
        println!("compressed: {:?}", compressed);
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_a() {
        let input = b"a";
        let mut cw = CompressWriter::new(vec![]);
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.to_inner();
        let expected = [1, 152, 64, 0, 96];
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
