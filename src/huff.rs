
use std::io::{Read, Write, copy};
use error::Error;

pub fn compress<R: Read, W: Write>(mut input: R, mut output: W) -> Result<W, Error> {
    try!(copy(&mut input, &mut output));
    Ok(output)
}

pub fn decompress<R: Read, W: Write>(mut input: R, mut output: W) -> Result<W, Error> {
    try!(copy(&mut input, &mut output));
    Ok(output)
}

#[cfg(test)]
mod test {
    use ::std::io::Cursor;
    use super::{compress, decompress};
    
    #[test]
    fn compress_decompress() {
        let input = include_bytes!("huff.rs");
        let compressed = compress(Cursor::new(&input[..]), vec![]).unwrap();
        let decompressed = decompress(Cursor::new(&compressed[..]), vec![]).unwrap();
        assert_eq!(&input[..], &decompressed[..]);
    }
}
