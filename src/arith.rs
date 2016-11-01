// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Simple implementation of an arithmetic coder.
//!
//! Implementation based on http://marknelson.us/2014/10/19/data-compression-with-arithmetic-coding/

use std::io::{Read, Write};

use bitfile::{BitReader, BitWriter};
use error::Error;

type Symbol = u16;

const EOF:  Symbol = 256;
const SYM_CNT: usize = (EOF + 1) as usize;

// https://sachingarg.com/compression/entropy_coding/64bit/ says we
// can use up to 0x3fff_ffff as the maximum frequency on 64 bit
// machines.  That's true, but it turns out that compression is much
// better with smaller numbers such a 0x3fff, probably due to better
// locality.
const MAX_FREQ: u64      = 0x3fff;

const ONE_HALF: u64      = 0x8000_0000;
const ONE_FOURTH: u64    = 0x4000_0000;
const THREE_FOURTHS: u64 = 0xC000_0000;
const MAX_CODE: u64      = 0xffff_ffff;

    #[derive(Debug,PartialEq)]
struct Prob {
    low:   u64,
    high:  u64,
    total: u64,
}

struct State {
    freqs: [u64; SYM_CNT + 1],
}

impl State {
    // Create a new state of the arithmetic coder.
    fn new() -> State {
        let mut st = State {
            freqs: [0; (SYM_CNT + 1) as usize],
        };
        for i in 0..SYM_CNT + 1 {
            st.freqs[i] = i as u64;
        }
        st
    }

    fn get_count(&self) -> u64 {
        self.freqs[SYM_CNT]
    }

    fn debug_print(&self) {
        for i in 0..SYM_CNT {
            let mut bar = String::new();
            let low = self.freqs[i as usize];
            let high = self.freqs[(i + 1) as usize];
            let range = high - low;
            for _ in 0..(range) {
                bar.push_str("#");
            }
            println!("{:?} {}: {} {}", (i as u8) as char, i, range, bar);
        }
    }
    
    fn preload(&mut self, counts: &[(u8, u64)]) {
        for &(s, c) in counts {
            for _ in 0..c {
                let _ = self.get_prob_and_update(s as Symbol);
            }
        }
    }
    
    /// Return the probability range for symbol `sym`. Also update the
    /// symbol frequency of `sym`, adapting the model to the symbols
    /// seen.
    fn get_prob_and_update(&mut self, sym: Symbol) -> Prob {
        let p = Prob {
            low: self.freqs[sym as usize],
            high: self.freqs[(sym+1) as usize],
            total: self.freqs[SYM_CNT],
        };
        self.update(sym);
        p
    }

    /// Increase the count for symbol `sym`, updating the cumulative
    /// frequencies accordingly.
    fn update(&mut self, sym: Symbol) {
        // Update all cumulative frequencies for the symbol `sym` and
        // the following symbols.
        for i in (sym as usize) + 1..(SYM_CNT + 1) {
            self.freqs[i] += 1;
        }
        // Bound the cumulative frequencies to avoid overflow.
        if self.freqs[SYM_CNT] >= MAX_FREQ {
            self.downscale();
        }
    }

    /// Scale down all frequencies by a half.  This is needed to avoid
    /// overflow on cumulative character counts.
    fn downscale(&mut self) {
        // 1. Convert from cumulative frequencies to individual
        // frequencies.
        for i in 1..SYM_CNT {
            self.freqs[SYM_CNT - i] -= self.freqs[SYM_CNT - i - 1];
        }
        self.freqs[SYM_CNT] = 1;
        // 2. Halve each frequency, making sure it never drops below
        // 1.
        for i in 1..SYM_CNT {
            if self.freqs[i] > 1 {
                self.freqs[i] /= 2;
            }
        }
        // 3. Convert back to cumulative frequencies.
        for i in 1..SYM_CNT + 1 {
            self.freqs[i] += self.freqs[i - 1];
        }
    }

    /// Determine the next encoded symbol from `scaled_value`, and
    /// return it together with its range bounds.
    fn get_symbol_and_update(&mut self, scaled_value: u64) -> (Prob, Symbol) {
        for i in 0..SYM_CNT {
            if scaled_value < self.freqs[i + 1] {
                let sym = i as Symbol;
                let prob = Prob {low: self.freqs[i],
                                 high: self.freqs[i + 1],
                                 total: self.freqs[SYM_CNT]};
                self.update(sym);
                return (prob, sym);
            }
        }
        unreachable!();
    }

}

/// This is an arithmetic encoder.
pub struct Encoder {
    state: State,
}

impl Encoder {
    /// Create a new encoder.  The encoder can only be used to
    /// compress one data stream.
    pub fn new() -> Encoder {
        Encoder { state: State::new() }
    }

    pub fn preload(&mut self, counts: &[(u8, u64)]) {
        self.state.preload(counts);
    }
    
    pub fn debug_print(&self) {
        self.state.debug_print();
    }
    
    fn output_bit_plus_pending<W: Write>(&mut self, bit: usize, pending_bits: &mut usize, bw: &mut BitWriter<W>) -> Result<(), Error> {
        try!(bw.write_bits(bit as u64, 1));
        while *pending_bits > 0 {
            try!(bw.write_bits((1 - bit) as u64, 1));
            *pending_bits -= 1;
        }
        Ok(())
    }

    /// Compress all the data from reader `input` and write the
    /// compressed data to the writer `output`.
    pub fn compress<R, W>(mut self, mut input: R, output: W) -> Result<W, Error>
        where R: Read,
              W: Write {

        let mut outp = BitWriter::new(output);
        
        let mut low: u64  = 0;
        let mut high: u64 = MAX_CODE;
        let mut pending_bits = 0;
        
        let mut cbuf = [0u8; 1];

        let mut nread = try!(input.read(&mut cbuf[..]));
        loop {
            // Convert short reads to the EOF symbol.
            let c = if nread == 0 {
                EOF
            } else {
                cbuf[0] as Symbol
            };
            
            let p = self.state.get_prob_and_update(c);
            
            let range: u64 = high - low + 1;
            
            high = low + (range * p.high / p.total) - 1;
            low = low + (range * p.low / p.total);
            
            loop {
                if high < ONE_HALF {
                    try!(self.output_bit_plus_pending(0, &mut pending_bits, &mut outp));
                } else if low >= ONE_HALF {
                    try!(self.output_bit_plus_pending(1, &mut pending_bits, &mut outp));
                } else if low >= ONE_FOURTH && high < THREE_FOURTHS {
                    pending_bits += 1;
                    low -= ONE_FOURTH;  
                    high -= ONE_FOURTH;  
                } else {
                    break;
                }
                high <<= 1;
                high += 1;
                low <<= 1;
                high &= MAX_CODE;
                low &= MAX_CODE;
            }

            // When EOF is encoded, terminate encoding loop.
            if c == EOF {
                break;
            }

            // Read character for next iteration.
            nread = try!(input.read(&mut cbuf[..]));
        }
        // Write out two MSB of low to make sure the decoder has
        // enough precision for decoding the last symbol.
        pending_bits += 1;
        if low < ONE_FOURTH {
            try!(self.output_bit_plus_pending(0, &mut pending_bits, &mut outp));
        } else {
            try!(self.output_bit_plus_pending(1, &mut pending_bits, &mut outp));
        }

        // Flush accumulated bits and return the underlying writer.
        outp.flush().unwrap();
        Ok(outp.to_inner())
    }

}

/// An arithmetic decoder.
pub struct Decoder {
    state: State,
}

impl Decoder {
    /// Create a new decoder.  The decoder can only be used to
    /// decompress one data stream.
    pub fn new() -> Decoder {
        Decoder { state: State::new() }
    }

    pub fn preload(&mut self, counts: &[(u8, u64)]) {
        self.state.preload(counts);
    }
    
    pub fn debug_print(&self) {
        self.state.debug_print();
    }
    
    /// Decompress all data from the reader `input`, writing the
    /// decompressed data to the writer `output`.
    pub fn decompress<R, W>(mut self, input: R, mut output: W) -> Result<W, Error>
        where R: Read,
              W: Write {

        let mut inp = BitReader::new_with_extra(input, 32*2);
        
        let mut low: u64  = 0;
        let mut high: u64 = MAX_CODE;
        let mut value: u64 = try!(inp.read_bits(32));

        loop {
            let range: u64 = (high as u64) - (low as u64) + 1;
            let count: u64 = (((value as u64) - (low as u64) + 1) * self.state.get_count() - 1) / range;

            let (p, c) = self.state.get_symbol_and_update(count);
            
            if c == EOF {
                break;
            }

            let _ = try!(output.write(&[c as u8]));
            high = low + (range * p.high) / p.total - 1;
            low = low + (range * p.low) / p.total;
            loop {
                if high < ONE_HALF {
                    //do nothing, bit is a zero
                } else if low >= ONE_HALF {
                    value -= ONE_HALF;  //subtract one half from all three code values
                    low -= ONE_HALF;
                    high -= ONE_HALF;
                } else if low >= ONE_FOURTH && high < THREE_FOURTHS {
                    value -= ONE_FOURTH;
                    low -= ONE_FOURTH;
                    high -= ONE_FOURTH;
                } else {
                    break;
                }
                low <<= 1;
                high <<= 1;
                high += 1;
                value <<= 1;
                // let in_bit = match inp.read_bit() {
                //     Ok(true) => 1,
                //     Ok(false) => 0,
                //     Err(Error::UnexpectedEof) => break,
                //     Err(e)=> return Err(e),
                // };
                // value += in_bit;
                value += try!(inp.read_bits(1));
            }
        }

        // Return the underlying writer.
        Ok(output)
    }
}

/// Encode all data from `input` using arithmetic compression and
/// write the compressed stream to `output`.  On success, the output
/// is returned.
pub fn compress<R: Read, W: Write>(input: R, output: W) -> Result<W, Error> {
    let enc = Encoder::new();
    enc.compress(input, output)
}

/// Decode all data from `input` using arithmetic compression and
/// write the decompressed stream to `output`.  On success, the output
/// is returned.
pub fn decompress<R: Read, W: Write>(input: R, output: W) -> Result<W, Error> {
    let dec = Decoder::new();
    dec.decompress(input, output)
}


#[cfg(test)]
mod test {
    use ::std::collections::HashMap;
    use ::std::io::Cursor;
    use super::{State, Prob, compress, decompress, Encoder, Decoder};

    #[test]
    fn get_prob() {
        let mut st = State::new();
        assert_eq!(Prob{low: 0, high: 1, total: 257}, st.get_prob_and_update(0));
        assert_eq!(Prob{low: 0, high: 2, total: 258}, st.get_prob_and_update(0));
        assert_eq!(Prob{low: 0, high: 3, total: 259}, st.get_prob_and_update(0));
        st.downscale();
        assert_eq!(Prob{low: 0, high: 2, total: 258}, st.get_prob_and_update(0));
    }

    #[test]
    fn get_sym() {
        let mut st = State::new();
        st.get_prob_and_update(0);
        st.get_prob_and_update(0);
        st.get_prob_and_update(0);
        st.get_prob_and_update(0);
        let (_, sym) = st.get_symbol_and_update(1);
        assert_eq!(0, sym);
        for _ in 0..1000 {
            st.get_prob_and_update(127);
        }
        let (_, sym) = st.get_symbol_and_update(131);
        assert_eq!(126, sym);
        let (_, sym) = st.get_symbol_and_update(133);
        assert_eq!(127, sym);
        let (_, sym) = st.get_symbol_and_update(1134);
        assert_eq!(127, sym);
        let (_, sym) = st.get_symbol_and_update(1136);
        assert_eq!(128, sym);
    }

    #[test]
    fn compress0() {
        let input = b"The banana goat in the banana boat can hand bananas to the banana man.";
        let c = Cursor::new(&input[..]);
        let compressed = compress(c, vec![]).unwrap();

        let expected = [84, 20, 127, 73, 221, 155, 247, 51, 8, 76, 67, 84, 214,
                        189, 168, 202, 126, 193, 88, 87, 234, 14, 7, 135, 177,
                        86, 184, 223, 223, 100, 13, 248, 114, 125, 96, 36, 120,
                        126, 103, 24, 23, 28, 212, 225, 61, 76, 219, 4, 45, 243,
                        220, 172, 147, 235, 59, 193, 128];

        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_short() {
        let input = b"A";
        let c = Cursor::new(&input[..]);
        let compressed = compress(c, vec![]).unwrap();

        let expected = [65, 189, 128];

        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn compress_empty() {
        let input = b"";
        let c = Cursor::new(&input[..]);
        let compressed = compress(c, vec![]).unwrap();

        let expected = [255, 64];

        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn decompress0() {
        let input = [84, 20, 127, 73, 221, 155, 247, 51, 8, 76, 67, 84, 214,
                     189, 168, 202, 126, 193, 88, 87, 234, 14, 7, 135, 177,
                     86, 184, 223, 223, 100, 13, 248, 114, 125, 96, 36, 120,
                     126, 103, 24, 23, 28, 212, 225, 61, 76, 219, 4, 45, 243,
                     220, 172, 147, 235, 59, 193, 128];
        let expected = b"The banana goat in the banana boat can hand bananas to the banana man.";

        let decompressed = decompress(Cursor::new(&input[..]), vec![]).unwrap();

        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_short() {
        let input = [65, 189, 128];
        let expected = b"A";

        let decompressed = decompress(Cursor::new(&input[..]), vec![]).unwrap();

        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn decompress_empty() {
        let input = [255, 64];
        let expected = b"";

        let decompressed = decompress(Cursor::new(&input[..]), vec![]).unwrap();

        assert_eq!(&expected[..], &decompressed[..]);
    }

    fn calc_counts(bytes: &[u8]) -> Vec<(u8, u64)> {
        let mut hm: HashMap<u8, u64> = HashMap::new();
        for b in bytes {
            *hm.entry(*b).or_insert(0) += 1;
        }
        let mut ret = Vec::new();
        for (k, v) in hm.into_iter() {
            ret.push((k, v));
        }
        ret
    }
    
    #[test]
    fn compress_preloaded() {
        let input = b"The banana goat in the banana boat can hand bananas to the banana man.";
        let counts = calc_counts(input);

        let mut enc = Encoder::new();
        enc.preload(&counts);
        let compressed = enc.compress(Cursor::new(&input[..]), vec![]).unwrap();

        let expected = [77, 112, 63, 170, 109, 243, 149, 47, 92, 146, 19, 121,
                        134, 77, 28, 86, 255, 177, 88, 240, 33, 30, 78, 175,
                        172, 218, 16, 109, 0, 191, 105, 183, 38, 185, 17, 45,
                        93, 100, 84, 239, 217, 198, 246, 13, 184];

        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn decompress_preloaded() {
        let input = [77, 112, 63, 170, 109, 243, 149, 47, 92, 146, 19, 121,
                     134, 77, 28, 86, 255, 177, 88, 240, 33, 30, 78, 175,
                     172, 218, 16, 109, 0, 191, 105, 183, 38, 185, 17, 45,
                     93, 100, 84, 239, 217, 198, 246, 13, 184];
        let expected = b"The banana goat in the banana boat can hand bananas to the banana man.";

        let counts = calc_counts(&expected[..]);
        let c = Cursor::new(&input[..]);
        let mut dec = Decoder::new();
        dec.preload(&counts);
        let decompressed = dec.decompress(c, vec![]).unwrap();

        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_decompress() {
        let f = include_bytes!("arith.rs");
        let original = &f[..];
        
        let c = Cursor::new(&original[..]);
        let compressed = compress(c, vec![]).unwrap();
        
        let c = Cursor::new(&compressed[..]);
        let decompressed = decompress(c, vec![]).unwrap();
        assert_eq!(&original[..], &decompressed[..]);
    }
}

