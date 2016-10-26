// Copyright 2016 Martin Grabmueller. See the LICENSE file at the
// top-level directory of this distribution for license information.

use std::cmp;

/// Implementation of a sliding window.  The sliding window is
/// represented by a byte vector that is double the size of the
/// sliding window, plus some look-ahead space:
///
/// +------------------------------+------------------------------+-----+
/// |abcdef                        |                              |     |
/// +------------------------------+------------------------------+-----+
///  p-----l
///
/// The number of bytes pushed into the window is recorded by an index
/// `limit`, which points one after the last element of the window.
/// Also, a current `position` is maintained. In the example above,
/// `position` points at the first element of the window ('a') and
/// `limit` point just after the last element, 'z'.
/// `position` is incrementing by calling `advance()`
///
/// The followin picture shows a situation where more elements were
/// pushed into the window, and the window has advanced a couple of
/// times.  The look-back-window as well as the look-ahead buffer are
/// indicated using the '-' characters below the characters.
///
/// +------------------------------+------------------------------+-----+
/// |abcdefghijklmnopqrstuvwxyz    |                              |     |
/// +------------------------------+------------------------------+-----+
///  --------------------p-----l
///
pub struct SlidingWindow {
    window: Vec<u8>,
    position: usize,
    limit: usize,
    window_size: usize,
    lookahead: usize,
    window_buffer_size: usize,
}

impl SlidingWindow {
    /// Create a new sliding window of `window_size` elements in to
    /// look back at and `lookahead` elements to look ahead to.
    pub fn new(window_size: usize, lookahead: usize) -> SlidingWindow {
        let buf_size = window_size * 2 + lookahead;
        SlidingWindow{
            window: Vec::with_capacity(buf_size),
            window_buffer_size: buf_size,
            position: 0,
            limit: 0,
            window_size: window_size,
            lookahead: lookahead,
        }
    }

    fn slide_down(&mut self) {
        assert!(self.position >= self.window_size);
        
        self.window.drain(0..self.window_size);
        self.position -= self.window_size;
        self.limit -= self.window_size;
    }

    /// Push one element to the end of the window.
    ///
    /// # Panics Panics when more than `lookahead` elements are pushed
    /// without consuming any elements.
    pub fn push(&mut self, b: u8) {
        assert!(self.position + self.lookahead >= self.limit);
        self.window.push(b);
        self.limit += 1;
    }

    /// Consume one element of the window.  This increments the
    /// pointer to the current element.
    ///
    /// # Panics
    /// Panics when the window is empty.
    pub fn advance(&mut self) {
        assert!(self.position < self.limit);
        if self.position >= 2 * self.window_size {
            self.slide_down();
        }
        self.position += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.position == self.limit
    }
    
    pub fn element(&self) -> u8 {
        assert!(self.position < self.limit);

        self.window[self.position]
    }

    /// Return a slice for the look-back window.
    pub fn window_slice(&self) -> &[u8] {
        let f =
            if self.position > self.window_size {
                self.position - self.window_size
            } else {
                0
            };
        &self.window[f..self.position]
    }

    /// Return a slice for the look-ahead buffer.
    pub fn lookahead_slice(&self) -> &[u8] {
        let l = cmp::min(self.position + self.lookahead, self.limit);
        &self.window[self.position..l]
    }

    pub fn debug_print(&self) {
        println!("");
        for i in 0..self.window_buffer_size {
            if i == 0 {
                print!("|");
            } else if i == self.window_size {
                print!("|");
            } else if i == self.window_size * 2 {
                print!("|");
            } else {
                print!(" ");
            }
        }
        println!("|");
        for i in 0..self.window_buffer_size + 1 {
            if i == cmp::min(self.position + self.lookahead, self.limit) {
                print!("v");
            } else {
                print!(" ");
            }
        }
        println!("");
        for i in 0..self.limit {
            if self.window[i] >= 32 && self.window[i] < 128 {
                print!("{}", self.window[i] as char);
            } else {
                print!("?");
            }
        }
        for _ in self.limit..self.window_buffer_size {
            print!("~");
        }
        println!("");
        for i in 0..self.window_buffer_size + 1 {
            if i == self.position {
                print!("p");
            } else if i == self.limit {
                print!("|");
            } else if i + self.window_size >= self.position && i < self.position {
                print!("-");
            } else {
                print!(" ");
            }
        }
        println!("");
        for i in 0..self.window_buffer_size + 1 {
            if i == self.limit {
                print!("l");
            } else {
                print!(" ");
            }
        }
        println!("");
    }
}

#[cfg(test)]
mod tests {
    use super::SlidingWindow;
    
    #[test]
    fn basics() {
        let mut w = SlidingWindow::new(20, 7);

        assert!(w.is_empty());
        assert_eq!(w.window_slice().len(), 0);
        assert_eq!(w.lookahead_slice().len(), 0);

        w.debug_print();
        
        w.push(b'a');

        assert!(!w.is_empty());
        w.debug_print();

        w.push(b'b');
        w.push(b'c');

        w.debug_print();

        assert_eq!(0, w.window_slice().len());
        assert_eq!(3, w.lookahead_slice().len());
        assert_eq!(b'a', w.element());
        assert_eq!(b'a', w.lookahead_slice()[0]);

        w.advance();
        
        w.debug_print();

        assert_eq!(b'b', w.element());
        assert_eq!(1, w.window_slice().len());
        assert_eq!(2, w.lookahead_slice().len());
        assert_eq!(b'a', w.window_slice()[0]);
        assert_eq!(b'b', w.lookahead_slice()[0]);

        for _ in 0..6 {
            w.push(b'x');
        }
        w.debug_print();
        assert_eq!(1, w.window_slice().len());
        assert_eq!(7, w.lookahead_slice().len());
        assert_eq!(b'a', w.window_slice()[0]);
        assert_eq!(b'b', w.lookahead_slice()[0]);

        for _ in 0..9 {
            w.advance();
            w.push(b'y');
        }
        w.debug_print();
        assert_eq!(10, w.window_slice().len());
        assert_eq!(7, w.lookahead_slice().len());

        for _ in 0..3 {
            w.advance();
        }
        w.debug_print();
        assert_eq!(13, w.window_slice().len());
        assert_eq!(5, w.lookahead_slice().len());
        assert_eq!(b'a', w.window_slice()[0]);
        assert_eq!(b'y', w.lookahead_slice()[0]);

        for _ in 0..10 {
            w.advance();
            w.push(b'z');
        }
        w.debug_print();
        assert_eq!(20, w.window_slice().len());
        assert_eq!(5, w.lookahead_slice().len());
        assert_eq!(b'x', w.window_slice()[0]);
        assert_eq!(b'z', w.lookahead_slice()[0]);

        for _ in 0..15 {
            w.advance();
            w.push(b'9');
        }
        w.debug_print();
        assert_eq!(20, w.window_slice().len());
        assert_eq!(5, w.lookahead_slice().len());
        assert_eq!(b'z', w.window_slice()[0]);
        assert_eq!(b'9', w.lookahead_slice()[0]);

        for _ in 0..3 {
            w.push(b'7');
        }
        w.debug_print();
        assert_eq!(20, w.window_slice().len());
        assert_eq!(7, w.lookahead_slice().len());
        assert_eq!(b'z', w.window_slice()[0]);
        assert_eq!(b'9', w.lookahead_slice()[0]);

        for _ in 0..3 {
            w.advance()
        }
        w.debug_print();
        assert_eq!(20, w.window_slice().len());
        assert_eq!(5, w.lookahead_slice().len());
        assert_eq!(b'z', w.window_slice()[0]);
        assert_eq!(b'9', w.lookahead_slice()[0]);

        w.push(b'R');
        w.debug_print();
        assert_eq!(20, w.window_slice().len());
        assert_eq!(6, w.lookahead_slice().len());
        assert_eq!(b'z', w.window_slice()[0]);
        assert_eq!(b'9', w.lookahead_slice()[0]);

        w.push(b'R');
    }

    #[test]
    #[should_panic]
    fn element_empty() {
        let w = SlidingWindow::new(20, 7);
        assert_eq!(b'x', w.element());
    }

    #[test]
    #[should_panic]
    fn push_full() {
        let mut w = SlidingWindow::new(20, 7);
        for _ in 0..9 {
            w.push(b'g');
        }
    }

    #[test]
    #[should_panic]
    fn advance_empty() {
        let mut w = SlidingWindow::new(20, 7);
        for _ in 0..7 {
            w.push(b'g');
        }
        for _ in 0..8 {
            w.advance();
        }
    }
}
