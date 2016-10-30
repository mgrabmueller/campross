// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Binary arithmetic encoder.
//!
//! This is an implentation of Moffat et al.'s binary arithmetic
//! encoder as presented in: Alistair Moffat, Radford M. Neal and Ian
//! H. Witten: Arithmetic Coding Revisited, ACM Transactions on
//! Information Systems, Vol 16, No 3, July 1998, pages 256-294.

use std::io::{Read, Write};
use std::io;

const B: usize = 60;
const F: usize = 30;

pub type Word = u64;

pub type Count = u32;
pub type Bit = usize;

pub struct Encoder<W> {
    inner: W,

    out_buf:     u8,
    out_bits:    usize,
    out_pending: usize,

    range: Word,
    low:   Word,
}

impl<W: Write> Encoder<W> {
    pub fn new(writer: W) -> Encoder<W> {
        Encoder{
            inner: writer,
            out_buf: 0,
            out_bits: 0,
            out_pending: 0,
            low: 0,
            range: 1 << (B - 1),
        }
    }

    fn out_flush(&mut self) -> io::Result<()> {
        let _ = try!(self.inner.write_all(&[self.out_buf]));
        self.out_buf  = 0;
        self.out_bits = 0;
        Ok(())
    }

    fn out_plus_pending(&mut self, bit: Bit) -> io::Result<()>{
        debug_assert!(bit <= 1);

        self.out_buf = (self.out_buf << 1) | (bit as u8);
        self.out_bits += 1;
        if self.out_bits == 8 {
            try!(self.out_flush());
        }
        let nbit = (1 - bit) as u8;
        while self.out_pending > 0 {
            self.out_buf = (self.out_buf << 1) | nbit;
            self.out_bits += 1;
            if self.out_bits == 8 {
                try!(self.out_flush());
            }
            self.out_pending -= 1;
        }
        Ok(())
    }

    /// Encode a byte, using a probability of 0.5 for ones and zeros
    /// alike.  This can be used to encode literals which don't have
    /// estimated probabilities.
    pub fn encode_byte(&mut self, mut byte: u8) -> io::Result<()> {
        for _ in 0..8 {
            try!(self.encode((byte >> 7) as Bit, 1, 1));
            byte <<= 1;
        }
        Ok(())
    }

    /// Encode a single bit.  `c0` gives the count of zeros and `c1`
    /// the number of ones in the model.
    pub fn encode(&mut self, bit: Bit, c0: Count, c1: Count) -> io::Result<()> {
        debug_assert!(bit <= 1);
        debug_assert!(c0 < (1 << F));
        debug_assert!(c1 < (1 << F));

        let (lps, c_lps) =
            if c0 < c1 {
                (0, c0)
            } else {
                (1, c1)
            };
        let r = self.range / ((c0 + c1) as Word);
        let r_lps = r * c_lps as Word;
        if bit == lps {
            self.low = self.low + self.range - r_lps;
            self.range = r_lps;
        } else {
            self.range = self.range - r_lps;
        }

        while self.range <= (1 << (B - 2)) {
            if self.low + self.range <= (1 << (B - 1)) {
                try!(self.out_plus_pending(0));
            } else if (1 << (B - 1)) <= self.low {
                try!(self.out_plus_pending(1));
                self.low = self.low - (1 << (B - 1));
            } else {
                self.out_pending += 1;
                self.low = self.low - (1 << (B - 2));
            }
            self.low = 2 * self.low;
            self.range = 2 * self.range;
        }

        Ok(())
    }

    /// Finish the encoder by writing all pending output to the
    /// underlying writer.
    pub fn finish(&mut self) -> io::Result<()> {
        // Output contents of low
        for _ in 0..B {
            let bit = ((self.low >> (B - 1)) & 1) as Bit;
            try!(self.out_plus_pending(bit));
            self.low <<= 1;
        }
        
        // Moffat et al.'s paper tells us that flushing the content of
        // L (self.low in our implementation) should be enough for
        // proper decoding.  For some reason, it does not work
        // (decoder sometimes outputs wrong last bit).  Writing two
        // additional zeros does work for all our tests.  Dunno why.
        try!(self.out_plus_pending(0));
        try!(self.out_plus_pending(0));
        
        if self.out_bits > 0 {
            try!(self.out_flush());
        }
        Ok(())
    }

    /// Extract the contained writer, consuming `self`.
    pub fn to_inner(self) -> W {
        self.inner
    }
}

pub struct Decoder<R> {
    inner: R,

    in_buf:  [u8; 1],
    in_bits: usize,

    range: Word,
    d: Word,
}

impl<R: Read> Decoder<R> {
    /// Create a new decoder from the given reader.  This operation
    /// will initiate decoding by reading in a word of data, therefore
    /// the result can be an error.
    pub fn new(reader: R) -> io::Result<Decoder<R>> {
        let mut d = Decoder{
            inner: reader,
            in_buf: [0; 1],
            in_bits: 0,
            d: 0,
            range: 1 << (B - 1),
        };
        for _ in 0..B {
            d.d = (d.d << 1) | (try!(d.get_bit()) as Word);
        }
        Ok(d)
    }

    fn get_bit(&mut self) -> io::Result<Bit> {
        if self.in_bits == 0 {
            let nread = try!(self.inner.read(&mut self.in_buf[..]));
            self.in_bits = 8;
            if nread < 1 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, ""));
            }
        }
        self.in_bits -= 1;
        let bit = (self.in_buf[0] >> 7) as Bit;
        self.in_buf[0] <<= 1;
        Ok(bit)
    }

    /// Decode a byte from the compressed stream, using a probability
    /// of 0.5 for ones and zeros alike.  This can be used to extract
    /// literal bytes from the stream when their probability is not
    /// known.
    pub fn decode_byte(&mut self) -> io::Result<u8> {
        let mut result = 0;
        for _ in 0..8 {
            let bit = try!(self.decode(1, 1));
            result = (result << 1) | bit as u8;
        }
        Ok(result)
    }

    /// Decode a single bit from the input. `c0` is the count of
    /// zeros, `c1` the count of ones in the model.
    pub fn decode(&mut self, c0: Count, c1: Count) -> io::Result<Bit> {
        debug_assert!(c0 < (1 << F));
        debug_assert!(c1 < (1 << F));
        debug_assert!((c0 + c1) < (1 << F));

        let (lps, c_lps) =
            if c0 < c1 {
                (0, c0)
            } else {
                (1, c1)
            };
        let r = self.range / ((c0 + c1) as Word);
        let r_lps = r * c_lps as Word;

        let bit;
        if self.d >= self.range - r_lps {
            bit = lps;
            self.d = self.d - (self.range - r_lps);
            self.range = r_lps;
        } else {
            bit = 1 - lps;
            self.range = self.range - r_lps;
        }
        while self.range <= (1 << (B - 2)) {
            self.range = 2 * self.range;
            self.d = (2 * self.d) | (try!(self.get_bit()) as Word);
        }

        Ok(bit)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use super::{Encoder, Decoder};

    #[test]
    fn encode_0() {
        let mut e = Encoder::new(vec![]);
        e.encode(1, 1, 1).unwrap();
        e.encode(0, 1, 1).unwrap();
        e.encode(1, 1, 1).unwrap();
        e.finish().unwrap();

        let o = e.to_inner();

        assert_eq!(vec![80, 0, 0, 0, 0, 0, 0, 0, 0], o);
    }

    #[test]
    fn decode_0() {
        let mut d = Decoder::new(Cursor::new(
            vec![80, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        
        let b = d.decode(1, 1).unwrap();
        assert_eq!(1, b);
        
        let b = d.decode(1, 1).unwrap();
        assert_eq!(0, b);
        
        let b = d.decode(1, 1).unwrap();
        assert_eq!(1, b);
    }

    #[test]
    fn encode_1() {
        let mut e = Encoder::new(vec![]);
        e.encode(0, 2, 1).unwrap();
        e.encode(1, 2, 1).unwrap();
        e.encode(0, 2, 1).unwrap();
        e.finish().unwrap();

        let o = e.to_inner();

        assert_eq!(vec![56, 227, 142, 56, 227, 142, 56, 240], o);
    }

    #[test]
    fn decode_1() {
        let mut d = Decoder::new(Cursor::new(
            vec![56, 227, 142, 56, 227, 142, 56, 240])).unwrap();
        
        let b = d.decode(2, 1).unwrap();
        assert_eq!(0, b);
        
        let b = d.decode(2, 1).unwrap();
        assert_eq!(1, b);
        
        let b = d.decode(2, 1).unwrap();
        assert_eq!(0, b);
    }
    
    #[test]
    fn encode_2() {
        let mut e = Encoder::new(vec![]);
        e.encode(1, 2, 1).unwrap();
        e.encode(1, 2, 1).unwrap();
        e.encode(1, 2, 1).unwrap();
        e.encode(1, 2, 1).unwrap();
        e.finish().unwrap();

        let o = e.to_inner();

        assert_eq!(vec![126, 107, 116, 240, 50, 145, 97, 251, 0], o);
    }

    #[test]
    fn decode_2() {
        let mut d = Decoder::new(Cursor::new(vec![126, 107, 116, 240, 50, 145, 97, 251, 0])).unwrap();
        
        let b = d.decode(2, 1).unwrap();
        assert_eq!(1, b);
        
        let b = d.decode(2, 1).unwrap();
        assert_eq!(1, b);
        
        let b = d.decode(2, 1).unwrap();
        assert_eq!(1, b);

        let b = d.decode(2, 1).unwrap();
        assert_eq!(1, b);
    }

    #[test]
    fn encode_3() {
        let mut e = Encoder::new(vec![]);
        for _ in 0..100 {
            e.encode(1, 1, 7).unwrap();
            e.encode(1, 1, 7).unwrap();
            e.encode(0, 1, 7).unwrap();
            e.encode(1, 1, 7).unwrap();
            e.encode(1, 1, 7).unwrap();
            e.encode(1, 1, 7).unwrap();
            e.encode(1, 1, 7).unwrap();
            e.encode(1, 1, 7).unwrap();
        }
        e.finish().unwrap();

        let o = e.to_inner();
        assert_eq!(
            vec![90, 45, 46, 155, 20, 36, 173, 47, 2, 136, 56, 106, 76,
                 39, 34, 243, 174, 18, 176, 28, 87, 111, 96, 65, 73,
                 122, 245, 55, 159, 169, 154, 174, 176, 116, 65, 55,
                 69, 35, 211, 175, 220, 114, 61, 99, 156, 183, 80, 147,
                 85, 36, 104, 238, 220, 92, 218, 235, 230, 177, 71, 199,
                 217, 64], o);
    }

    #[test]
    fn decode_3() {
        let mut d = Decoder::new(Cursor::new(
            vec![90, 45, 46, 155, 20, 36, 173, 47, 2, 136, 56, 106, 76,
                 39, 34, 243, 174, 18, 176, 28, 87, 111, 96, 65, 73,
                 122, 245, 55, 159, 169, 154, 174, 176, 116, 65, 55,
                 69, 35, 211, 175, 220, 114, 61, 99, 156, 183, 80, 147,
                 85, 36, 104, 238, 220, 92, 218, 235, 230, 177, 71, 199,
                 217, 64])).unwrap();

        for _ in 0..100 {
            let b = d.decode(1, 7).unwrap();
            assert_eq!(1, b);
            let b = d.decode(1, 7).unwrap();
            assert_eq!(1, b);
            let b = d.decode(1, 7).unwrap();
            assert_eq!(0, b);
            let b = d.decode(1, 7).unwrap();
            assert_eq!(1, b);
            let b = d.decode(1, 7).unwrap();
            assert_eq!(1, b);
            let b = d.decode(1, 7).unwrap();
            assert_eq!(1, b);
            let b = d.decode(1, 7).unwrap();
            assert_eq!(1, b);
            let b = d.decode(1, 7).unwrap();
            assert_eq!(1, b);
        }        
    }

    #[test]
    fn compress_decompress() {
        let f = include_bytes!("binarith.rs");
        let original = &f[..];
        let mut e = Encoder::new(vec![]);

        for b in original {
            e.encode_byte(*b).unwrap();
        }
        e.finish().unwrap();

        let o = e.to_inner();

        let mut d = Decoder::new(Cursor::new(o)).unwrap();
        for b in original {
            let decoded = d.decode_byte().unwrap();
            assert_eq!(*b, decoded);
        }
    }
    
}
