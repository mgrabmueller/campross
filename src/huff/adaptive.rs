// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple adaptive Huffman coder.  Based on Mark Nelson, Jean-Loup
//! Gailly: The Data Compression Book, 2nd Edition, M&T Books, 1996.

use std::io;
use std::io::{Read, Write};

use bitfile::{BitReader, BitWriter};
use error::Error;

type Symbol = usize;

const EOF: usize = 256;
const ESCAPE: usize = 257;
const SYMBOL_COUNT: usize = 258;
const NODE_TABLE_COUNT: usize = SYMBOL_COUNT * 2 - 1;
const ROOT_NODE: usize = 0;
const MAX_WEIGHT: usize = 0x8000;

#[derive(Copy, Clone)]
struct Node {
    weight: usize,
    parent: Option<usize>,
    child_is_leaf: bool,
    child: usize,
}

impl Node {
    fn new() -> Self {
        Node{
            weight: 0,
            parent: None,
            child_is_leaf: false,
            child: 0,
        }
    }
}

struct Tree {
    leaf: [Option<usize>; SYMBOL_COUNT],
    next_free_node: usize,
    nodes: [Node; NODE_TABLE_COUNT],
}

impl Tree {
    fn new() -> Self {
        let mut tree = Tree {
            leaf: [None; SYMBOL_COUNT],
            next_free_node: 0,
            nodes: [Node::new(); NODE_TABLE_COUNT],
        };
        tree.nodes[ROOT_NODE].child = ROOT_NODE + 1;
        tree.nodes[ROOT_NODE].child_is_leaf = false;
        tree.nodes[ROOT_NODE].weight = 2;
        tree.nodes[ROOT_NODE].parent = None;
        
        tree.nodes[ROOT_NODE + 1].child = EOF;
        tree.nodes[ROOT_NODE + 1].child_is_leaf = true;
        tree.nodes[ROOT_NODE + 1].weight = 1;
        tree.nodes[ROOT_NODE + 1].parent = Some(ROOT_NODE);
        tree.leaf[EOF] = Some(ROOT_NODE + 1);

        tree.nodes[ROOT_NODE + 2].child = ESCAPE;
        tree.nodes[ROOT_NODE + 2].child_is_leaf = true;
        tree.nodes[ROOT_NODE + 2].weight = 1;
        tree.nodes[ROOT_NODE + 2].parent = Some(ROOT_NODE);
        tree.leaf[ESCAPE] = Some(ROOT_NODE + 2);

        tree.next_free_node = ROOT_NODE + 3;
        
        tree
    }

    // fn dump_tree(&self, node: usize, nesting: usize) {
    //     for _ in 0..nesting*2 {
    //         print!(" ");
    //     }
    //     if self.nodes[node].child_is_leaf {
    //         if self.nodes[node].child < 256 {
    //             println!("n{} {:?} w:{} p:{:?}", node, (self.nodes[node].child as u8) as char, self.nodes[node].weight, self.nodes[node].parent);
    //         } else {
    //             println!("n{} {} w:{} p:{:?}", node, self.nodes[node].child, self.nodes[node].weight, self.nodes[node].parent);
    //         }
    //     } else {
    //         println!("n{} w:{} p:{:?}", node, self.nodes[node].weight, self.nodes[node].parent);
    //         self.dump_tree(self.nodes[node].child, nesting + 1);
    //         self.dump_tree(self.nodes[node].child + 1, nesting + 1);
    //     }
    // }
    
    // fn dump(&self) {
    //     self.dump_tree(ROOT_NODE, 1);
    //     for i in 0..ESCAPE + 1 {
    //         if let Some(idx) = self.leaf[i] {
    //             println!(" {:?} ({:?}) -> {}", i, (i as u8) as char, idx);
    //         }
    //     }
    // }
    
    fn add_new_node(&mut self, sym: Symbol) {
        let lightest_node = self.next_free_node - 1;
        let new_node = self.next_free_node;
        let zero_weight_node = self.next_free_node + 1;
        self.next_free_node += 2;

        self.nodes[new_node] = self.nodes[lightest_node];
        self.nodes[new_node].parent = Some(lightest_node);
        self.leaf[self.nodes[new_node].child] = Some(new_node);

        self.nodes[lightest_node].child = new_node;
        self.nodes[lightest_node].child_is_leaf = false;

        self.nodes[zero_weight_node].child = sym;
        self.nodes[zero_weight_node].child_is_leaf = true;
        self.nodes[zero_weight_node].weight = 0;
        self.nodes[zero_weight_node].parent = Some(lightest_node);
        self.leaf[sym] = Some(zero_weight_node);
    }

    fn update_model(&mut self, sym: Symbol) {
        if self.nodes[ROOT_NODE].weight == MAX_WEIGHT {
            self.rebuild_tree();
        }
        let mut mb_current_node = self.leaf[sym];
        while let Some(mut current_node) = mb_current_node {
            self.nodes[current_node].weight += 1;
            let mut new_node = current_node;
            while new_node > ROOT_NODE {
                if self.nodes[new_node - 1].weight >= self.nodes[current_node].weight {
                    break;
                }
                new_node -= 1;
            }
            if new_node != current_node {
                self.swap_nodes(current_node, new_node);
                current_node = new_node;
            }
            mb_current_node = self.nodes[current_node].parent;
        }
    }

    fn swap_nodes(&mut self, i: usize, j: usize) {
        if self.nodes[i].child_is_leaf {
            self.leaf[self.nodes[i].child] = Some(j);
        } else {
            let child = self.nodes[i].child;
            self.nodes[child].parent = Some(j);
            self.nodes[child + 1].parent = Some(j);
        }
        if self.nodes[j].child_is_leaf {
            self.leaf[self.nodes[j].child] = Some(i);
        } else {
            let child = self.nodes[j].child;
            self.nodes[child].parent = Some(i);
            self.nodes[child + 1].parent = Some(i);
        }
        let mut temp = self.nodes[i];
        self.nodes[i] = self.nodes[j];
        self.nodes[i].parent = temp.parent;
        temp.parent = self.nodes[j].parent;
        self.nodes[j] = temp;
    }
    
    fn rebuild_tree(&mut self) {
        let mut i;
        let mut j;
        let mut k;
        let mut weight;

        j = self.next_free_node - 1;
        i = j;
        loop {
            if self.nodes[i].child_is_leaf {
                self.nodes[j] = self.nodes[i];
                self.nodes[j].weight = (self.nodes[j].weight + 1) / 2;
                j -= 1;
            }
            if i == ROOT_NODE {
                break;
            }
            i -= 1;
        }

        i = self.next_free_node - 2;
        loop {
            k = i + 1;
            self.nodes[j].weight = self.nodes[i].weight +
                self.nodes[k].weight;
            weight = self.nodes[j].weight;
            self.nodes[j].child_is_leaf = false;
            k = j + 1;
            while weight < self.nodes[k].weight {
                k += 1;
            }
            k -= 1;
            for x in 0..k-j {
                self.nodes[j + x] = self.nodes[j + x + 1];
            }
            self.nodes[k].weight = weight;
            self.nodes[k].child = i;
            self.nodes[k].child_is_leaf = false;

            if j == ROOT_NODE {
                break;
            }
            i -= 2;
            j -= 1;
        }

        i = self.next_free_node - 1;
        loop {
            if self.nodes[i].child_is_leaf {
                k = self.nodes[i].child;
                self.leaf[k] = Some(i);
            } else {
                k = self.nodes[i].child;
                self.nodes[k].parent = Some(i);
                self.nodes[k + 1].parent = Some(i);
            }
            if i == ROOT_NODE {
                break;
            }
            i -= 1;
        }
    }
}

pub struct Writer<W> {
    inner: BitWriter<W>,
    tree: Tree,
}

impl<W: Write> Writer<W> {
    pub fn new(output: W) -> Self {
        Writer{
            inner: BitWriter::new(output),
            tree: Tree::new(),
        }
    }

    pub fn encode_symbol(&mut self, sym: Symbol) -> io::Result<()> {
        let mut code = 0;
        let mut code_size = 0;
        let mut current_bit = 1;
        
        let mut mb_current_node = self.tree.leaf[sym];
        
        if mb_current_node.is_none() {
            mb_current_node = self.tree.leaf[ESCAPE];
        }
        
        while let Some(current_node) = mb_current_node {
            if current_node == ROOT_NODE {
                break;
            }
            if current_node & 1 == 0 {
                code |= current_bit;
            }
            current_bit <<= 1;
            code_size += 1;
            mb_current_node = self.tree.nodes[current_node].parent;
        }

        try!(self.inner.write_bits(code, code_size));
        if self.tree.leaf[sym].is_none() {
            try!(self.inner.write_bits(sym as u64, 8));
            self.tree.add_new_node(sym);
        }
        Ok(())
    }

    pub fn into_inner(self) -> W {
        self.inner.to_inner()
    }
}


pub struct Reader<R> {
    inner: BitReader<R>,
    tree: Tree,
    eof: bool,
}

impl<R: Read> Reader<R> {
    pub fn new(output: R) -> Self {
        Reader{
            inner: BitReader::new(output),
            tree: Tree::new(),
            eof: false,
        }
    }

    fn decode_symbol(&mut self) -> io::Result<Symbol> {
        let mut current_node = ROOT_NODE;

        while !self.tree.nodes[current_node].child_is_leaf {
            current_node = self.tree.nodes[current_node].child;
            current_node += try!(self.inner.read_bits(1)) as usize;
        }
        let mut c = self.tree.nodes[current_node].child;
        if c == ESCAPE {
            c = try!(self.inner.read_bits(8)) as usize;
            self.tree.add_new_node(c);
        }
        Ok(c)
    }
}

impl<W: Write> Write for Writer<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        for b in buffer {
            let sym = *b as Symbol;
            try!(self.encode_symbol(sym));
            self.tree.update_model(sym);
        }
        Ok(buffer.len())
    }
    
    fn flush(&mut self) -> io::Result<()> {
        try!(self.encode_symbol(EOF));
        self.inner.flush()
    }
}

impl<R: Read> Read for Reader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.eof {
            return Ok(0);
        }

        let mut written = 0;
        for p in buffer.iter_mut() {
            let s = try!(self.decode_symbol());
            if s == EOF {
                self.eof = true;
                break;
            }
            *p = s as u8;
            written += 1;
            self.tree.update_model(s);
        }
        Ok(written)
    }
}

pub fn compress<R: Read, W: Write>(mut input: R, output: W) -> Result<W, Error> {
    let mut cw = Writer::new(output);
    try!(io::copy(&mut input, &mut cw));
    try!(cw.flush());
    Ok(cw.into_inner())
}

pub fn decompress<R: Read, W: Write>(input: R, mut output: W) -> Result<W, Error> {
    let mut cr = Reader::new(input);
    try!(io::copy(&mut cr, &mut output));
    Ok(output)
}


#[cfg(test)]
mod test {
    use std::io::{Cursor, Write, Read};
    use super::{Writer, Reader};

    #[test]
    fn compress_empty() {
        let input = b"";
        let mut e = Writer::new(vec![]);
        e.write_all(input).unwrap();
        e.flush().unwrap();
        let compressed = e.into_inner();
        let expected = vec![0];
        assert_eq!(expected, compressed);
    }

    #[test]
    fn decompress_empty() {
        let input = vec![0];
        let mut e = Reader::new(Cursor::new(input));
        let mut decompressed = Vec::new();
        e.read_to_end(&mut decompressed).unwrap();
        let expected = b"";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_a() {
        let input = b"a";
        let mut e = Writer::new(vec![]);
        e.write_all(input).unwrap();
        e.flush().unwrap();
        let compressed = e.into_inner();
        let expected = vec![176, 192];
        assert_eq!(expected, compressed);
    }

    #[test]
    fn decompress_a() {
        let input = vec![176, 192];
        let mut e = Reader::new(Cursor::new(input));
        let mut decompressed = Vec::new();
        e.read_to_end(&mut decompressed).unwrap();
        let expected = b"a";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_aaa() {
        let input = b"aaaaaaaaa";
        let mut e = Writer::new(vec![]);
        e.write_all(input).unwrap();
        e.flush().unwrap();
        let compressed = e.into_inner();
        let expected = vec![176, 176, 48];
        assert_eq!(expected, compressed);
    }

    #[test]
    fn decompress_aaa() {
        let input = vec![176, 176, 48];
        let mut e = Reader::new(Cursor::new(input));
        let mut decompressed = Vec::new();
        e.read_to_end(&mut decompressed).unwrap();
        let expected = b"aaaaaaaaa";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_decompress() {
        let input = include_bytes!("adaptive.rs");
        let mut cw = Writer::new(vec![]);
        cw.write(&input[..]).unwrap();
        cw.flush().unwrap();
        let compressed = cw.into_inner();

        let mut cr = Reader::new(Cursor::new(&compressed[..]));
        let mut decompressed = Vec::new();
        let _ = cr.read_to_end(&mut decompressed).unwrap();
        
        assert_eq!(&input[..], &decompressed[..]);
    }
}
