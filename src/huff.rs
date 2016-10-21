// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of a Huffman encoder.

use std::io::{Read, Write};
use error::Error;
use bitfile::{BitWriter, BitReader};


const MAX_FREQ: u32 = (1 << 16) - 1;

const EOB: Symbol = 256;
const EOF: Symbol = 257;

const ALPHABET_SIZE: usize = (EOF as usize) + 1;

const BLOCK_SIZE: usize = 1 << 15;

type Symbol = u16;

struct State {
    freqs: [u32; ALPHABET_SIZE],
    nodes: Vec<Node>,
}

#[derive(Debug)]
struct Node {
    freq: u32,
    entry: Entry,
}

#[derive(Debug)]
enum Entry {
    Leaf(Symbol),
    Inner(Box<Node>, Box<Node>),
}

impl State {
    fn new() -> State {
        let mut s = State {
            freqs: [0; ALPHABET_SIZE],
            nodes: Vec::new(),
        };
        s.freqs[EOB as usize] = 1;
        s.freqs[EOF as usize] = 1;
        s
    }

    fn reset(&mut self) {
        for i in self.freqs.iter_mut() {
            *i = 0;
        }
        self.freqs[EOB as usize] = 1;
        self.freqs[EOF as usize] = 1;
    }
    
    fn update(&mut self, sym: Symbol) {
        let idx = sym as usize;
        self.freqs[idx] += 1;
        if self.freqs[idx] >= MAX_FREQ {
            for i in 0..ALPHABET_SIZE {
                if self.freqs[i] > 1 {
                    self.freqs[i] /= 2;
                }
            }
        }
    }

    fn build_tree(&mut self) {
        for i in 0..EOF + 1 {
            if self.freqs[i as usize] > 0 {
                self.nodes.push(Node{freq: self.freqs[i as usize], entry: Entry::Leaf(i)});
            }
        }
        while self.nodes.len() > 1 {
            self.nodes.sort_by(|&Node{freq: f1, ..}, &Node{freq: f2, ..}| f2.cmp(&f1));
            let e1 = self.nodes.pop().unwrap();
            let e2 = self.nodes.pop().unwrap();
            let e = Node{freq: e1.freq + e2.freq, entry: Entry::Inner(Box::new(e1), Box::new(e2))};
            self.nodes.push(e);
        }
//        println!("Nodes: {:?}", self.nodes);
    }

    fn build_c(&self, code: u64, code_len: usize, node: &Node, codes: &mut [(u64, usize)]) {
        match node {
            &Node{entry: Entry::Leaf(c), ..} => {
                codes[c as usize] = (code, code_len);
            },
            &Node{entry: Entry::Inner(ref n1, ref n2), ..} => {
                self.build_c((code << 1), code_len + 1, &n1, codes);
                self.build_c((code << 1) | 1, code_len + 1, &n2, codes);
            },
        }
    }
    
    fn build_codes(&self, codes: &mut [(u64, usize)]) {
        if self.nodes.len() > 0 {
            let ref n0 = self.nodes[0];
            self.build_c(0, 0, &n0, codes);
        }
        // println!("{} codes: ", codes.len());
        // for (i, &(c, clen)) in codes.iter().enumerate() {
        //     if clen > 0 {
        //         println!("{:3} [{:4}] {:20b} ({})", i, self.freqs[i as usize], c, clen);
        //     }
        // }
    }
}

pub fn compress<R: Read, W: Write>(mut input: R, output: W) -> Result<W, Error> {
    let mut state = State::new();
    let mut block = [0u8; BLOCK_SIZE];
    let mut codes: [(u64, usize); ALPHABET_SIZE] =  [(0, 0); ALPHABET_SIZE];

    let mut outp = BitWriter::new(output);

    // Build initial Huffman tree and generate prefix codes. This is only used for empty files.
    state.build_tree();
    state.build_codes(&mut codes);

    // Read the first block of data.
    let mut block_size = try!(input.read(&mut block[..]));
    while block_size > 0 {
        // Reset state in order to clear out the symbol statistics
        // from the start/previous block.
        state.reset();
        
        // Count character frequencies.
        for i in 0..block_size {
            state.update(block[i] as Symbol);
        }
        // Build Huffman tree and generate prefix codes.
        state.build_tree();
        state.build_codes(&mut codes);

        // Write symbol frequencies to output stream.
        let mut code_cnt = 0;
        for i in 0..ALPHABET_SIZE {
            if state.freqs[i as usize] > 0 {
                code_cnt += 1;
            }
        }
        try!(outp.write_bits(code_cnt as u64, 9));
        for i in 0..ALPHABET_SIZE {
            if state.freqs[i as usize] > 0 {
                try!(outp.write_bits(i as u64, 9));
                try!(outp.write_bits(state.freqs[i as usize] as u64, 16));
            }
        }

        // Finally, encode the block's data.
        for i in 0..block_size {
            let (c, clen) = codes[block[i] as usize];
            try!(outp.write_bits(c, clen));
        }

        // Read the next block's data.
        block_size = try!(input.read(&mut block[..]));
        if block_size > 0 {
            let (c, clen) = codes[EOB as usize];
            try!(outp.write_bits(c, clen));
        }
    }

    let (c, clen) = codes[EOF as usize];
    try!(outp.write_bits(c, clen));
    
    outp.flush()
}

pub fn decompress<R: Read, W: Write>(input: R, mut output: W) -> Result<W, Error> {
    let mut codes: [(u64, usize); ALPHABET_SIZE] =  [(0, 0); ALPHABET_SIZE];
    let mut inp = BitReader::new(input);

    let mut state = State::new();

    // Build initial Huffman tree and generate prefix codes. This is only used for empty files.
    state.build_tree();
    state.build_codes(&mut codes);

    'outer:
    loop {
        state.reset();
        
        let code_cnt = try!(inp.read_bits(9));
        for _ in 0..code_cnt {
            let i = try!(inp.read_bits(9)) as Symbol;
            let f = try!(inp.read_bits(16)) as u32;
            state.freqs[i as usize] = f;
        }
        
        // Build Huffman tree and generate prefix codes.
        state.build_tree();
        state.build_codes(&mut codes);

        let root_node = &state.nodes[0];
        let mut n = root_node;
        'inner:
        loop {
            let b = try!(inp.read_bit());
//            println!("processing bit {}", b);
            let (next_node, code) =
                match n {
                    &Node{entry: Entry::Leaf(c), ..} => {
                        n = root_node;
                        (n, Some(c))
                    },
                    &Node{entry: Entry::Inner(ref n1, ref n2), ..} => {
                        if b {
                            (&*(*n1), None)
                        } else {
                            (&*(*n2), None)
                        }
                    },
                };
            n = next_node;
            if let Some(c) = code {
                if c == EOB {
                    break 'inner;
                } else if c == EOF {
                    break 'outer;
                } else {
                    try!(output.write(&[c as u8]));
                    println!("{}", (c as u8) as char);
                }
            }
        }
    }

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

    // #[test]
    // fn compress_decompress() {
    //     let input = include_bytes!("huff.rs");
    //     let compressed = compress(Cursor::new(&input[..]), vec![]).unwrap();
    //     let decompressed = decompress(Cursor::new(&compressed[..]), vec![]).unwrap();
    //     assert_eq!(&input[..], &decompressed[..]);
    // }
}
