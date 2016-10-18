//! Routines for bitwise input/output.

use std::io::Read;
use std::io::Write;
use std::io;

pub struct BitReader<R> {
    inner: R,
    buf: u8,
    mask: u8,
}

impl<R: Read> BitReader<R> {
    /// Create a new `BitReader` from a `Read` instance.
    pub fn new(inner: R) -> BitReader<R> {
        BitReader{
            inner: inner,
            buf: 0,
            mask: 0x80,
        }
    }

    /// Read the next bit.
    pub fn read_bit(&mut self) -> io::Result<bool> {
        if self.mask == 0x80 {
            let mut b = [0u8; 1];
            let nread = try!(self.inner.read(&mut b[..]));
            if nread == 0 {
                return Ok(false);
            }
            self.buf = b[0];
        }
        let result = self.buf & self.mask;
        self.mask >>= 1;
        if self.mask == 0 {
            self.mask = 0x80;
        }
        Ok(result != 0)
    }

    /// Read the next `count` bits, as the least significant bits of
    /// the returned 64-bit value.  Note that the maximum number of
    /// bits to read in one call is 64.
    pub fn read_bits(&mut self, mut count: usize) -> io::Result<u64> {
        let mut result = 0;
        while count > 0 {
            let b = try!(self.read_bit());
            result <<= 1;
            if b {
                result |= 1;
            }
            count -= 1;
        }
        Ok(result)
    }
}

pub struct BitWriter<W> {
    inner: W,
    buf: u8,
    mask: u8,
}

impl<W: Write> BitWriter<W> {
    /// Create a bit writer from a `Write` instance.
    pub fn new(inner: W) -> BitWriter<W> {
        BitWriter{
            inner: inner,
            buf: 0,
            mask: 0x80,
        }
    }

    /// Write a bit to the underlying `Write` instance.
    pub fn write_bit(&mut self, bit: bool) -> io::Result<()> {
        if bit {
            self.buf |= self.mask;
        }
        self.mask >>= 1;
        if self.mask == 0 {
            try!(self.inner.write(&[self.buf]));
            self.mask = 0x80;
            self.buf = 0;
        }
        Ok(())
    }

    /// Write the `count` least significant bits from `value`.  Note
    /// that the maximum number of bits to write in one call is 64.
    pub fn write_bits(&mut self, value: usize, mut count: usize) -> io::Result<()> {
        while count > 0 {
            try!(self.write_bit((value & (1 << (count - 1))) != 0));
            count -= 1;
        }
        Ok(())
    }

    /// Flush any unwritten bits to the underlying `Write` instance
    /// and return it.
    pub fn flush(mut self) -> io::Result<W> {
        if self.mask != 0x80 {
            try!(self.inner.write(&[self.buf]));
        }
        Ok(self.inner)
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;
    use super::BitReader;
    use super::BitWriter;

    #[test]
    fn write_bit() {
        let out = vec![];
        let mut bf = BitWriter::new(out);
        bf.write_bit(true).unwrap();
        bf.write_bit(false).unwrap();
        bf.write_bit(true).unwrap();
        bf.write_bit(true).unwrap();
        bf.write_bit(false).unwrap();
        let o = bf.flush().unwrap();
        assert_eq!(vec![0b1011_0000], o);
    }
    
    #[test]
    fn write_bits() {
        let out = vec![];
        let mut bf = BitWriter::new(out);
        bf.write_bits(0b1011, 4).unwrap();
        bf.write_bits(0b000, 3).unwrap();
        bf.write_bits(0b0010, 4).unwrap();
        bf.write_bits(0b11111, 5).unwrap();
        bf.write_bits(0b11, 2).unwrap();
        bf.write_bits(0b11_0010_1010, 10).unwrap();
        let o = bf.flush().unwrap();
        assert_eq!(vec![0b1011_0000, 0b0101_1111, 0b1111_0010, 0b1010_0000], o);
    }
    
    #[test]
    fn read_bit() {
        let c = Cursor::new(vec![0b1111_0001, 0b0101_1100, 0b0000_0000]);
        let mut bf = BitReader::new(c);
        let b = bf.read_bit().unwrap();
        assert_eq!(true, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(true, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(true, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(true, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(false, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(false, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(false, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(true, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(false, b);
        let b = bf.read_bit().unwrap();
        assert_eq!(true, b);
    }

    #[test]
    fn read_bits() {
        let c = Cursor::new(vec![0b1111_0001, 0b0101_1100, 0b0000_0001]);
        let mut bf = BitReader::new(c);
        let b = bf.read_bits(5).unwrap();
        assert_eq!(0b11110, b);
        let b = bf.read_bits(2).unwrap();
        assert_eq!(0b00, b);
        let b = bf.read_bits(4).unwrap();
        assert_eq!(0b1010, b);
        let b = bf.read_bits(12).unwrap();
        assert_eq!(0b1110_0000_0000, b);
        let b = bf.read_bits(1).unwrap();
        assert_eq!(0b1, b);
    }

}
