// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

//! Adapted from https://github.com/kazuho/rangecoder/blob/master/range_coder.hpp

use std::io::{Read, Write};
use std::num::Wrapping;

const TOP: u32 = 1 << 24;

pub struct RangeCoder<W> {
    ll: u32,
    rr: u32,
    buffer: u8,
    start: bool,
    carry_n: usize,
    counter: usize,
    inner: W,
}

impl<W: Write> RangeCoder<W> {
    pub fn new(output: W) -> RangeCoder<W> {
        RangeCoder {
            ll: 0,
            rr: 0xffffffff,
            buffer: 0,
            carry_n: 0,
            counter: 0,
            start: true,
            inner: output,
        }
    }
    
    pub fn encode(&mut self, symbol: u32, cum_freq: &[u32]) {
        let lo = cum_freq[symbol as usize];
        let hi = cum_freq[symbol as usize + 1];
        let total = cum_freq[cum_freq.len() - 1];
        let r = self.rr / total;
        println!("lo={} hi={} total={} r={}", lo, hi, total, r);
        if hi < total {
            self.rr = r * (hi - lo);
        } else {
            self.rr -= r * lo;
        }
        println!("rr={:08x}", self.rr);
        let new_ll = (Wrapping(self.ll) + Wrapping(r) * Wrapping(lo)).0;
        if new_ll < self.ll {
            self.buffer += 1;
            while self.carry_n != 0 {
                println!("{:2x}", self.buffer);
                let buffer = self.buffer;
                self.putc(buffer);
                self.buffer = 0;
                self.carry_n -= 1;
            }
        }
        self.ll = new_ll;
        println!("ll={:08x}", self.ll);
        while self.rr < TOP {
            let new_buffer = (self.ll > 24) as u8;
            if self.start {
                self.buffer = new_buffer;
                self.start = false;
            } else if new_buffer == 0xff {
                self.carry_n += 1;
            } else {
                println!("{:2x}", self.buffer);
                let buffer = self.buffer;
                self.putc(buffer);
                while self.carry_n != 0 {
                    println!("{:2x}", 0xffu8);
                    self.putc(0xff);
                    self.carry_n -= 1;
                }
                self.buffer = new_buffer;
            }
            self.ll <<= 8;
            self.rr <<= 8;
        }
        println!("final ll={:08x}", self.ll);
        self.counter += 1;
    }

    pub fn finish(&mut self) {
        let buffer = self.buffer;
        self.putc(buffer);
        println!("{:2x}", self.buffer);
        while self.carry_n != 0 {
            println!("{:2x}", 0xffu8);
            self.putc(0xff);
            self.carry_n -= 1;
        }
        let mut t = self.ll + self.rr;
        loop {
            let t8 = t >> 24;
            let l8 = self.ll >> 24;
            
            self.putc(l8 as u8);
            println!("{:2x}", l8);
            
            if t8 != l8 {
	        break;
            }
            t <<= 8;
            self.ll <<= 8;
        }
    }

    fn putc(&mut self, b: u8) {
        let nwritten = self.inner.write(&[b]).expect("could not write");
        assert_eq!(1, nwritten);
    }
    
    pub fn to_inner(self) -> W {
        self.inner
    }
}

pub struct RangeDecoder<R> {
    rr: u32,
    dd: u32,
    inner: R,
}

impl<R: Read> RangeDecoder<R> {
    pub fn new(input: R) -> RangeDecoder<R> {
        let mut rd = RangeDecoder {
            rr: 0xffffffff,
            dd: 0,
            inner: input,
        };
        for _ in 0..4 {
            rd.dd = (rd.dd << 8) | (rd.next() as u32);
        }
        println!("dd primed: {:08x}", rd.dd);
        rd
    }

    pub fn next(&mut self) -> u8 {
        let mut buf = [0u8; 1];
        let nread = self.inner.read(&mut buf[..]).expect("cannot read");
        if nread == 1 {
            buf[0]
        } else {
            0xff
        }
    }

    pub fn decode(&mut self, cum_freq: &[u32]) -> u32 {
        let total = cum_freq[cum_freq.len() - 1];
        
        println!("in: rr={:08x}, dd={:08x}", self.rr, self.dd);
        
        let r = self.rr / total;
        
        println!("r={} (total={}), dd/r={}", r, total, self.dd / r);
        
        let target_pos = ::std::cmp::min(total - 1, self.dd / r);
        
        println!("target_pos={}", target_pos);
        
        let mut index = 0;
        for i in 0..cum_freq.len() {
            if target_pos < cum_freq[i] {
                index = i - 1;
                break;
            }
        }
        
        println!("index={}", index);
        
        let lo = cum_freq[index];
        
        println!("lo={}", lo);
        
        let hi = cum_freq[index + 1];
        
        println!("hi={}", hi);

        self.dd -= r * lo;
        
        println!("dd={:08x}", self.dd);
        
        if hi != total {
            self.rr = r * (hi - lo);
        } else {
            self.rr -= r * lo;
        }
        
        println!("rr={:08x}", self.rr);
        
        while self.rr < TOP {
            self.rr <<= 8;
            self.dd = (self.dd << 8) | (self.next() as u32);
            
            println!("reloaded rr={:08x}", self.rr);
        }
        
        println!("out: symbol={}. rr={:08x}, dd={:08x}", index, self.rr, self.dd);
        
        index as u32
    }
}

#[cfg(test)]
mod test {
    use super::{RangeCoder, RangeDecoder};
    use ::std::io::Cursor;

    // Text: 0,1,0,2,3
    // Symbols:   0,1,2,3
    // Freq:      2,1,1,1
    // Cum. Freq: 0,2,3,4,5
    static CUM_FREQ: [u32; 5] = [0,1,2,3,4];

    #[test]
    fn encode_1() {
        let mut rc = RangeCoder::new(vec![]);

        rc.encode(0, &CUM_FREQ);
        rc.encode(1, &CUM_FREQ);
        rc.encode(0, &CUM_FREQ);
        rc.encode(2, &CUM_FREQ);
        rc.encode(3, &CUM_FREQ);
        rc.finish();

        let coded = rc.to_inner();
        let expected = [0x01u8, 0x2f];
        assert_eq!(&expected[..], &coded[..]);
        assert!(false);
   }

    #[test]
    fn decode_1() {
        let mut rc = RangeDecoder::new(Cursor::new(vec![0x01, 0x2f]));

        let c = rc.decode(&CUM_FREQ);
        assert_eq!(0, c);
        let c = rc.decode(&CUM_FREQ);
        assert_eq!(1, c);
        let c = rc.decode(&CUM_FREQ);
        assert_eq!(0, c);
        let c = rc.decode(&CUM_FREQ);
        assert_eq!(2, c);
        let c = rc.decode(&CUM_FREQ);
        assert_eq!(3, c);

        assert!(false);
   }
}
