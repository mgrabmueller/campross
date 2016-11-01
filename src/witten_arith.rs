// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Arithmetic encoder.
//!
//! This is a direct translation of the C source in Ian H. Witten,
//! Radford M. Neal and John G. Cleary: Arithmetic Coding for Data
//! Compression, Communications of the ACM, Vol. 30, Number 6, June
//! 1987.  Only the adaptive model is included.

use std::io::{Read, Write, Bytes};
use std::io;

use error::Error;

// You can uncomment the following line and comment the line after to
// try out compression with a smaller word size.  The difference will
// be negligable (less than 0.01% on my experiments.
//const CODE_VALUE_BITS: usize = 16;
const CODE_VALUE_BITS: usize = 32;

type CodeValue = u64;

const TOP_VALUE: CodeValue = (1u64 << CODE_VALUE_BITS) - 1;

const FIRST_QTR: CodeValue = (TOP_VALUE / 4) + 1;
const HALF: CodeValue = 2 * FIRST_QTR;
const THIRD_QTR: CodeValue = 3 * FIRST_QTR;

const NO_OF_CHARS: usize = 256;

const EOF_SYMBOL: usize = NO_OF_CHARS + 1;
const NO_OF_SYMBOLS: usize = EOF_SYMBOL + 1;

// Using a max frequency of 2^14 - 1 actually gives better compression
// than bigger values like 2^30 - 1.  I suppose this is due to better
// locality.
const MAX_FREQUENCY: usize = (1 << 14) - 1;

type Symbol = usize;

struct Model {
    char_to_index: [usize; NO_OF_CHARS],
    index_to_char: [usize; NO_OF_SYMBOLS + 1],
    cum_freq: [usize; NO_OF_SYMBOLS + 1],
    freq: [usize; NO_OF_SYMBOLS + 1],

}

impl Model {
    fn new() -> Self {
        let mut m = Model {
            char_to_index: [0; NO_OF_CHARS],
            index_to_char: [0; NO_OF_SYMBOLS + 1],
            cum_freq: [0; NO_OF_SYMBOLS + 1],
            freq: [0; NO_OF_SYMBOLS + 1],
        };
        for i in 0..NO_OF_CHARS {
            m.char_to_index[i] = i + 1;
            m.index_to_char[i + 1] = i;
        }
        for i in 0..NO_OF_SYMBOLS + 1 {
            m.freq[i] = 1;
            m.cum_freq[i] = NO_OF_SYMBOLS - i;
        }
        m.freq[0] = 0;
        m
    }

    fn update(&mut self, symbol: Symbol) {
        if self.cum_freq[0] == MAX_FREQUENCY {
            let mut cum = 0;
            let mut i = NO_OF_SYMBOLS;
            while i > 0 {
                self.freq[i] = (self.freq[i] + 1) / 2;
                self.cum_freq[i] = cum;
                cum += self.freq[i];
                i -= 1;
            }
            self.freq[0] = (self.freq[0] + 1) / 2;
            self.cum_freq[0] = cum;
        }

        let mut i = symbol;
        while i > 0 && self.freq[i] == self.freq[i - 1] {
            i -= 1;
        }
        if i < symbol {
            let ch_i = self.index_to_char[i];
            let ch_symbol = self.index_to_char[symbol];
            self.index_to_char[i] = ch_symbol;
            self.index_to_char[symbol] = ch_i;
            self.char_to_index[ch_i] = symbol;
            self.char_to_index[ch_symbol] = i;
        }
        self.freq[i] += 1;
        while i > 0 {
            i -= 1;
            self.cum_freq[i] += 1;
        }
    }

}

/// Arithmetic encoder.
struct Encoder<W> {
    inner: W,

    model: Model,
    
    low: CodeValue,
    high: CodeValue,
    bits_to_follow: usize,

    buffer: u8,
    bits_to_go: usize,
}

impl<W: Write> Encoder<W> {
    pub fn new(output: W) -> Self {
        let enc = Encoder{
            inner: output,

            model: Model::new(),
            
            low: 0,
            high: TOP_VALUE,
            bits_to_follow: 0,

            buffer: 0,
            bits_to_go: 8,
        };
        enc
    }

    fn encode_symbol(&mut self, symbol: Symbol) -> io::Result<()> {
        let range = (self.high - self.low) + 1;
        let total = self.model.cum_freq[0] as CodeValue;

        debug_assert!(total <= MAX_FREQUENCY as CodeValue);

        let hi_freq = self.model.cum_freq[symbol-1] as CodeValue;
        let lo_freq = self.model.cum_freq[symbol] as CodeValue;

        self.high = self.low + (range * hi_freq) / total - 1;
        self.low = self.low + (range * lo_freq) / total;

        loop {
            if self.high < HALF {
                try!(self.bit_plus_follow(0));
            } else if self.low >= HALF {
                try!(self.bit_plus_follow(1));
                self.low -= HALF;
                self.high -= HALF;
            } else if self.low >= FIRST_QTR && self.high < THIRD_QTR {
                self.bits_to_follow += 1;
                self.low -= FIRST_QTR;
                self.high -= FIRST_QTR;
            } else {
                break;
            }
            self.low = self.low << 1;
            self.high = (self.high << 1) + 1;
        }

        Ok(())
    }

    fn done_encoding(&mut self) -> io::Result<()> {
        self.bits_to_follow += 1;
        if self.low < FIRST_QTR {
            try!(self.bit_plus_follow(0));
        } else {
            try!(self.bit_plus_follow(1));
        }
        Ok(())
    }

    fn bit_plus_follow(&mut self, bit: usize) -> io::Result<()> {
        try!(self.output_bit(bit));
        while self.bits_to_follow > 0 {
            try!(self.output_bit(1 - bit));
            self.bits_to_follow -= 1;
        }
        Ok(())
    }

    fn output_bit(&mut self, bit: usize) -> io::Result<()> {
        self.buffer >>= 1;
        if bit != 0 {
            self.buffer |= 0x80;
        }
        self.bits_to_go -= 1;
        if self.bits_to_go == 0 {
            try!(self.inner.write_all(&[self.buffer]));
            self.bits_to_go = 8;
        }
        Ok(())
    }
    
    fn done_outputting_bits(&mut self) -> io::Result<()> {
        if self.bits_to_go < 8 {
            try!(self.inner.write_all(&[self.buffer >> self.bits_to_go]));
        }
        Ok(())
    }

    fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        for b in data {
            let symbol = self.model.char_to_index[*b as usize];
            try!(self.encode_symbol(symbol));
            self.model.update(symbol);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        try!(self.encode_symbol(EOF_SYMBOL));
        try!(self.done_encoding());
        self.done_outputting_bits()
    }
}

/// Arithmetic decoder.
struct Decoder<R> {
    inner: Bytes<R>,

    model: Model,
    
    value: CodeValue,
    low: CodeValue,
    high: CodeValue,

    buffer: u8,
    bits_to_go: usize,
    garbage_bits: usize,

    eof: bool,
}

impl<R: Read> Decoder<R> {
    pub fn new(input: R) -> io::Result<Self> {
        let mut dec = Decoder{
            inner: input.bytes(),

            model: Model::new(),
            
            value: 0,
            low: 0,
            high: TOP_VALUE,

            buffer: 0,
            bits_to_go: 0,
            garbage_bits: 0,

            eof: false,
        };
        for _ in 0..CODE_VALUE_BITS {
            dec.value = (dec.value << 1) | (try!(dec.input_bit()) as CodeValue);
        }
        Ok(dec)
    }

    fn input_bit(&mut self) -> io::Result<usize> {
        if self.bits_to_go == 0 {
            if let Some(b) = self.inner.next() {
                self.buffer = try!(b);
            } else {
                self.garbage_bits += 1;
                if self.garbage_bits > CODE_VALUE_BITS - 2 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                              "cannot read from bit stream"));
                } else {
                    self.buffer = 0xff;
                }
            }
            self.bits_to_go = 8;
        }
        let t = self.buffer & 1;
        self.buffer >>= 1;
        self.bits_to_go -= 1;
        Ok(t as usize)
    }
    
    fn decode_symbol(&mut self) -> io::Result<Symbol> {

        let range = self.high - self.low + 1;
        let total = self.model.cum_freq[0] as CodeValue;
        let cum = ((self.value - self.low + 1) * total - 1) / range;

        // Find symbol with the cumulative frequency that matches the
        // current interval.
        let mut symbol = 1;
        while self.model.cum_freq[symbol] as CodeValue > cum {
            symbol += 1;
        }

        let lo_freq = self.model.cum_freq[symbol] as CodeValue;
        let hi_freq = self.model.cum_freq[symbol - 1] as CodeValue;
        
        self.high = self.low + (range * hi_freq / total) - 1;
        self.low = self.low + (range * lo_freq / total);

        loop {
            if self.high < HALF {
                // do nothing
            } else if self.low >= HALF {
                self.value -= HALF;
                self.low -= HALF;
                self.high -= HALF;
            } else if self.low >= FIRST_QTR && self.high < THIRD_QTR {
                self.value -= FIRST_QTR;
                self.low -= FIRST_QTR;
                self.high -= FIRST_QTR;
            } else {
                break;
            }
            self.low = self.low << 1;
            self.high = (self.high << 1) + 1;
            self.value = (self.value << 1) + (try!(self.input_bit()) as CodeValue);
        }
        Ok(symbol)
    }
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
        if self.eof {
            return Ok(0);
        }
       
        let mut written = 0;
        while written < data.len()  {
            let symbol = try!(self.decode_symbol());
            if symbol == EOF_SYMBOL {
                self.eof = true;
                break;
            }
            let ch = self.model.index_to_char[symbol as usize];
            data[written] = ch as u8;
            written += 1;
            self.model.update(symbol);
        }
        Ok(written)
    }
}

/// Read all data from `input`, compress it using an order-0
/// arithmetic encoder and write the compressed data to `output`.
pub fn compress<R: Read, W: Write>(mut input: R, output: W) -> Result<W, Error> {
    let mut cw = Encoder::new(output);
    try!(io::copy(&mut input, &mut cw));
    try!(cw.flush());
    Ok(cw.into_inner())
}

/// Read all data from `input`, decompress it using an order-0
/// arithmetic encoder and write the decompressed data to `output`.
/// The data must be produced by the `compress` function.
pub fn decompress<R: Read, W: Write>(input: R, mut output: W) -> Result<W, Error> {
    let mut cr = try!(Decoder::new(input));
    try!(io::copy(&mut cr, &mut output));
    Ok(output)
}

#[cfg(test)]
mod test {
    use std::io::Cursor;
    use super::{compress, decompress};

    #[test]
    fn compress_empty() {
        let input = [];
        let compressed = compress(Cursor::new(&input), vec![]).unwrap();
        let expected = [128, 0];
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn decompress_empty() {
        let input = [128, 0];
        let decompressed = decompress(Cursor::new(&input), vec![]).unwrap();
        let expected: [u8;0] = [];
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_a() {
        let input = b"a";
        let compressed = compress(Cursor::new(&input), vec![]).unwrap();
        let expected = [121, 195, 0];
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn decompress_a() {
        let input = [121, 195, 0];
        let decompressed = decompress(Cursor::new(&input), vec![]).unwrap();
        let expected = b"a";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_aaa() {
        let input = b"aaaaaaaaa";
        let compressed = compress(Cursor::new(&input), vec![]).unwrap();
        let expected = [249, 253, 255, 255, 255, 255, 223, 126];
        assert_eq!(&expected[..], &compressed[..]);
    }

    #[test]
    fn decompress_aaa() {
        let input = [249, 253, 255, 255, 255, 255, 223, 126];
        let decompressed = decompress(Cursor::new(&input), vec![]).unwrap();
        let expected = b"aaaaaaaaa";
        assert_eq!(&expected[..], &decompressed[..]);
    }

    #[test]
    fn compress_decompress() {
        let f = include_bytes!("witten_arith.rs");
        let original = &f[..];
        let compressed = compress(Cursor::new(&original), vec![]).unwrap();
        let decompressed = decompress(Cursor::new(compressed), vec![]).unwrap();
            
        assert_eq!(&original[..], &decompressed[..]);
    }
}
