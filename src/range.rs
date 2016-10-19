

type Symbol = u16;

const MIN: u64 = 0;
const MAX: u64 = 0x1000_0000_0000_0000;

const EOF: Symbol = 256;
const ESC: Symbol = 257;
const FLUSH: Symbol = 258;

pub fn encode(input: &[u8], output: Vec<u8>) {
    let ranges =
        [
            (EOF, 1),
            (ESC, 2),
            (FLUSH, 1)
        ];
}
