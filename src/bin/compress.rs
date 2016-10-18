extern crate campross;

use std::fs::File;
use std::io::Cursor;
use std::io::Read;

use campross::{Compressor, Decompressor};

pub fn main() {

    let mut f = File::open(std::env::args().skip(1).next().unwrap()).unwrap();
    let mut input: Vec<u8> = vec![];
    let _ = f.read_to_end(&mut input);
    let output = vec![];
    let mut compressor = Compressor::new(Cursor::new(&input[..]), output);
    compressor.process().unwrap();
    let result = compressor.finish();
//    println!("{}", String::from_utf8_lossy(&result));

    let decoutput = vec![];
    let mut decompressor = Decompressor::new(Cursor::new(&result[..]), decoutput);
    decompressor.process().unwrap();
    let decresult = decompressor.finish();
//    println!("{}", String::from_utf8_lossy(&result));
    println!("raw input size: {}", input.len());
    println!("compressed output size: {}", result.len());
    println!("compressed input size: {}", result.len());
    println!("decompressed output size: {}", decresult.len());
    println!("ratio: {}", result.len() as f32 / input.len() as f32);
    assert_eq!(input, decresult);
}
