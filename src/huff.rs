use std::collections::HashMap;

pub type Symbol = u16;

const ESC: Symbol = 256;
const EOF: Symbol = 257;

struct Leaf {
    symbol: Symbol,
}

struct Inner {
    left: usize,
    right: usize,
}

enum Entry {
    Leaf(Leaf),
    Inner(Inner),
}

struct Node {
    count: u16,
    parent: u16,
    node: Entry,
}

struct Tree {
    nodes:  Vec<Node>,
    root: u16,
}

impl Tree {
    pub fn new() -> Tree {
        let mut tree =
            Tree {
                nodes: Vec::new(),
                root: 0,
            };
        tree.add_symbol(ESC);
        tree.add_symbol(EOF);
        tree
    }

    pub fn add_symbol(&mut self, sym: Symbol) {
    }
}

pub struct Huff {
    tree: Tree,
}

impl Huff {
    pub fn new() -> Huff {
        Huff {
            tree: Tree::new(),
        }
    }
    
}
