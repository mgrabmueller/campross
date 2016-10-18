

type Symbol = u16;

const EOF: Symbol = 256;
const ESC: Symbol = 257;
const FLUSH: Symbol = 258;

pub fn encode(input: &[u8], output: Vec<u8>) {
    let ranges =
        [
            (EOF,         0,  500_000),
            (ESC,   500_000,  750_000),
            (FLUSH, 750_000, 1_000_000)
        ];

}
